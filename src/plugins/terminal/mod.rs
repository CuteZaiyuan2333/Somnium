use std::sync::Arc;
use std::io::Write;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize, MasterPty};
use vte::{Parser, Perform};
use egui::{Ui, WidgetText, Color32, FontId, Rect, Vec2, Key};
use crate::{Tab, Plugin, AppCommand, TabInstance};

// ----------------------------------------------------------------------------
// Terminal Buffer & Parser
// ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Cell {
    c: char,
    fg: Color32,
}

impl Default for Cell {
    fn default() -> Self {
        Self { c: ' ', fg: Color32::LIGHT_GRAY }
    }
}

struct TerminalState {
    rows: usize,
    cols: usize,
    cursor_row: usize,
    cursor_col: usize,
    grid: Vec<Vec<Cell>>,
}

impl TerminalState {
    fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            cursor_row: 0,
            cursor_col: 0,
            grid: vec![vec![Cell::default(); cols]; rows],
        }
    }

    fn scroll_up(&mut self) {
        self.grid.remove(0);
        self.grid.push(vec![Cell::default(); self.cols]);
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
        }
    }

    fn resize(&mut self, new_rows: usize, new_cols: usize) {
        if new_rows == 0 || new_cols == 0 { return; }
        
        // 调整行数
        if new_rows > self.rows {
            for _ in 0..(new_rows - self.rows) {
                self.grid.push(vec![Cell::default(); self.cols]);
            }
        } else {
            self.grid.truncate(new_rows);
        }

        // 调整列数
        for row in &mut self.grid {
            if new_cols > self.cols {
                row.resize(new_cols, Cell::default());
            } else {
                row.truncate(new_cols);
            }
        }

        self.rows = new_rows;
        self.cols = new_cols;
        self.cursor_row = self.cursor_row.min(self.rows - 1);
        self.cursor_col = self.cursor_col.min(self.cols - 1);
    }
}

struct LogHandler<'a> {
    state: &'a mut TerminalState,
}

impl<'a> Perform for LogHandler<'a> {
    fn print(&mut self, c: char) {
        if self.state.cursor_col >= self.state.cols {
            self.state.cursor_col = 0;
            self.state.cursor_row += 1;
        }
        if self.state.cursor_row >= self.state.rows {
            self.state.scroll_up();
        }
        if self.state.cursor_row < self.state.rows && self.state.cursor_col < self.state.cols {
            self.state.grid[self.state.cursor_row][self.state.cursor_col] = Cell { c, fg: Color32::LIGHT_GRAY };
            self.state.cursor_col += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.state.cursor_col = 0,
            b'\n' => {
                self.state.cursor_row += 1;
                if self.state.cursor_row >= self.state.rows {
                    self.state.scroll_up();
                }
            }
            b'\x08' => { // Backspace
                if self.state.cursor_col > 0 {
                    self.state.cursor_col -= 1;
                }
            }
            b'\x07' => {} // Bell
            b'\t' => { // Tab
                let next_tab = (self.state.cursor_col / 8 + 1) * 8;
                self.state.cursor_col = next_tab.min(self.state.cols - 1);
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &vte::Params, _intermediates: &[u8], _ignore: bool, c: char) {
        match c {
            'H' | 'f' => { // Cursor position
                let mut row = 0;
                let mut col = 0;
                let mut it = params.iter();
                if let Some(p) = it.next() { row = p[0].saturating_sub(1) as usize; }
                if let Some(p) = it.next() { col = p[0].saturating_sub(1) as usize; }
                self.state.cursor_row = row.min(self.state.rows - 1);
                self.state.cursor_col = col.min(self.state.cols - 1);
            }
            'J' => { // Erase in display
                // 简化处理：如果是 2，清除全部
                self.state.grid = vec![vec![Cell::default(); self.state.cols]; self.state.rows];
            }
            'K' => { // Erase in line
                // 简化处理：清除当前行
                if self.state.cursor_row < self.state.rows {
                    for cell in &mut self.state.grid[self.state.cursor_row] {
                        *cell = Cell::default();
                    }
                }
            }
            'A' => { // Cursor Up
                let amt = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.state.cursor_row = self.state.cursor_row.saturating_sub(amt);
            }
            'B' => { // Cursor Down
                let amt = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.state.cursor_row = (self.state.cursor_row + amt).min(self.state.rows - 1);
            }
            'C' => { // Cursor Forward
                let amt = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.state.cursor_col = (self.state.cursor_col + amt).min(self.state.cols - 1);
            }
            'D' => { // Cursor Backward
                let amt = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.state.cursor_col = self.state.cursor_col.saturating_sub(amt);
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _c: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

// ----------------------------------------------------------------------------
// Tab Instance
// ----------------------------------------------------------------------------

pub struct TerminalTab {
    state: Arc<Mutex<TerminalState>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<dyn Write + Send>>,
    last_size: (usize, usize),
}

impl std::fmt::Debug for TerminalTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalTab").finish()
    }
}

impl Clone for TerminalTab {
    fn clone(&self) -> Self {
        TerminalTab {
            state: self.state.clone(),
            master: self.master.clone(),
            writer: self.writer.clone(),
            last_size: self.last_size,
        }
    }
}

impl TabInstance for TerminalTab {
    fn title(&self) -> WidgetText { " Terminal".into() }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        let font_id = FontId::monospace(14.0);
        let char_size = Vec2::new(ui.fonts(|f| f.glyph_width(&font_id, 'M')), 18.0);
        
        let available_size = ui.available_size();
        let cols = (available_size.x / char_size.x).floor() as usize;
        let rows = (available_size.y / char_size.y).floor() as usize;

        if cols > 2 && rows > 2 && (cols != self.last_size.0 || rows != self.last_size.1) {
            let mut state = self.state.lock();
            state.resize(rows, cols);
            let _ = self.master.lock().resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
            self.last_size = (cols, rows);
        }

        // 处理键盘输入
        ui.input(|i| {
            let mut w = self.writer.lock();
            for event in &i.events {
                match event {
                    egui::Event::Text(text) => {
                        let _ = w.write_all(text.as_bytes());
                    }
                    egui::Event::Key { key, pressed: true, ..} => {
                        let seq = match key {
                            Key::Enter => Some("\r"),
                            Key::Backspace => Some("\x08"),
                            Key::ArrowUp => Some("\x1b[A"),
                            Key::ArrowDown => Some("\x1b[B"),
                            Key::ArrowRight => Some("\x1b[C"),
                            Key::ArrowLeft => Some("\x1b[D"),
                            Key::Tab => Some("\t"),
                            _ => None,
                        };
                        if let Some(s) = seq {
                            let _ = w.write_all(s.as_bytes());
                        }
                    }
                    _ => {}
                }
            }
            let _ = w.flush();
        });

        // 渲染背景
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(20, 20, 20));

        // 渲染网格
        let state = self.state.lock();
        for r in 0..state.rows {
            for c in 0..state.cols {
                let cell = state.grid[r][c];
                if cell.c != ' ' {
                    let pos = rect.min + Vec2::new(c as f32 * char_size.x, r as f32 * char_size.y);
                    ui.painter().text(pos, egui::Align2::LEFT_TOP, cell.c, font_id.clone(), cell.fg);
                }
            }
        }
        
        // 渲染光标 (必须在锁定内获取位置)
        let cursor_pos = rect.min + Vec2::new(state.cursor_col as f32 * char_size.x, state.cursor_row as f32 * char_size.y);
        ui.painter().rect_filled(Rect::from_min_size(cursor_pos, char_size), 0.0, Color32::from_rgba_unmultiplied(200, 200, 200, 120));
        
        ui.ctx().request_repaint();
    }

    fn box_clone(&self) -> Box<dyn TabInstance> {
        Box::new(self.clone())
    }
}

// ----------------------------------------------------------------------------
// Plugin Implementation
// ----------------------------------------------------------------------------

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn name(&self) -> &str { "terminal" }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("Terminal").clicked() {
            match create_terminal_tab() {
                Ok(tab) => {
                    control.push(AppCommand::OpenTab(Tab::new(Box::new(tab))));
                }
                Err(e) => {
                    eprintln!("Failed to create terminal: {}", e);
                }
            }
            ui.close_menu();
        }
    }
}

fn create_terminal_tab() -> Result<TerminalTab, Box<dyn std::error::Error + Send + Sync>> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    #[cfg(windows)]
    let cmd = CommandBuilder::new("powershell.exe");
    #[cfg(not(windows))]
    let cmd = CommandBuilder::new("bash");

    let _child = pair.slave.spawn_command(cmd)?;

    let writer = pair.master.take_writer()?;
    let mut reader = pair.master.try_clone_reader()?;
    let master = pair.master;
    
    let state = Arc::new(Mutex::new(TerminalState::new(24, 80)));
    let state_for_thread = state.clone();

    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        let mut parser = Parser::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let mut state = state_for_thread.lock();
                    let mut handler = LogHandler { state: &mut *state };
                    for byte in &buffer[..n] {
                        parser.advance(&mut handler, *byte);
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(TerminalTab {
        state,
        master: Arc::new(Mutex::new(master)),
        writer: Arc::new(Mutex::new(writer)),
        last_size: (80, 24),
    })
}

pub fn create() -> TerminalPlugin {
    TerminalPlugin
}