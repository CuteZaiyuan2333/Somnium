use std::sync::Arc;
use std::io::Write;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize, MasterPty};
use vte::{Parser, Perform};
use egui::{Ui, WidgetText, Color32, FontId, Rect, Vec2, Key, Id, Sense, Pos2};
use egui::text::{LayoutJob, TextFormat};
use crate::{Tab, Plugin, AppCommand, TabInstance};

// ----------------------------------------------------------------------------
// Terminal Constants & Logic
// ----------------------------------------------------------------------------

const TERM_BG: Color32 = Color32::from_rgb(10, 10, 10);
const TERM_FG: Color32 = Color32::from_rgb(220, 220, 220);

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cell {
    c: char,
    fg: Color32,
    bg: Color32,
    inverse: bool,
    is_wide_continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self { c: ' ', fg: TERM_FG, bg: Color32::TRANSPARENT, inverse: false, is_wide_continuation: false }
    }
}

fn ansi_color(code: u8) -> Color32 {
    match code {
        0 => Color32::from_rgb(0, 0, 0),
        1 => Color32::from_rgb(205, 0, 0),
        2 => Color32::from_rgb(0, 205, 0),
        3 => Color32::from_rgb(205, 205, 0),
        4 => Color32::from_rgb(0, 0, 238),
        5 => Color32::from_rgb(205, 0, 205),
        6 => Color32::from_rgb(0, 205, 205),
        7 => Color32::from_rgb(229, 229, 229),
        8 => Color32::from_rgb(127, 127, 127),
        9 => Color32::from_rgb(255, 0, 0),
        10 => Color32::from_rgb(0, 255, 0),
        11 => Color32::from_rgb(255, 255, 0),
        12 => Color32::from_rgb(92, 92, 255),
        13 => Color32::from_rgb(255, 0, 255),
        14 => Color32::from_rgb(0, 255, 255),
        15 => Color32::from_rgb(255, 255, 255),
        _ => TERM_FG,
    }
}

// ----------------------------------------------------------------------------
// Terminal State
// ----------------------------------------------------------------------------

struct TerminalState {
    rows: usize,
    cols: usize,
    
    cursor_row: usize,
    cursor_col: usize,
    saved_cursor: (usize, usize),
    
    primary_grid: Vec<Vec<Cell>>,
    alt_grid: Vec<Vec<Cell>>,
    history: Vec<Vec<Cell>>, 
    is_alt_screen: bool,
    
    current_fg: Color32,
    current_bg: Color32,
    current_inverse: bool,
    cursor_visible: bool,
    application_cursor: bool,
}

impl TerminalState {
    fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor: (0, 0),
            primary_grid: vec![vec![Cell::default(); cols]; rows],
            alt_grid: vec![vec![Cell::default(); cols]; rows],
            history: Vec::new(),
            is_alt_screen: false,
            current_fg: TERM_FG,
            current_bg: Color32::TRANSPARENT,
            current_inverse: false,
            cursor_visible: true,
            application_cursor: false,
        }
    }

    fn grid_mut(&mut self) -> &mut Vec<Vec<Cell>> {
        if self.is_alt_screen { &mut self.alt_grid } else { &mut self.primary_grid }
    }

    fn grid(&self) -> &Vec<Vec<Cell>> {
        if self.is_alt_screen { &self.alt_grid } else { &self.primary_grid }
    }

    fn scroll_up(&mut self) {
        let cols = self.cols;
        let fg = self.current_fg;
        let bg = self.current_bg;
        let inverse = self.current_inverse;
        
        let top_row = self.grid_mut().remove(0);
        
        if !self.is_alt_screen {
            self.history.push(top_row);
            if self.history.len() > 5000 { 
                self.history.remove(0);
            }
        }

        self.grid_mut().push(vec![Cell { c: ' ', fg, bg, inverse, is_wide_continuation: false }; cols]);
        
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
        }
    }

    fn resize(&mut self, new_rows: usize, new_cols: usize) {
        if new_rows == 0 || new_cols == 0 { return; }
        
        fn resize_grid(grid: &mut Vec<Vec<Cell>>, r: usize, c: usize) {
             if r > grid.len() {
                for _ in 0..(r - grid.len()) {
                    grid.push(vec![Cell::default(); c]);
                }
            } else {
                grid.truncate(r);
            }
            for row in grid.iter_mut() {
                if c > row.len() {
                    row.resize(c, Cell::default());
                } else {
                    row.truncate(c);
                }
            }
        }

        resize_grid(&mut self.primary_grid, new_rows, new_cols);
        resize_grid(&mut self.alt_grid, new_rows, new_cols);

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
        let is_wide = c as u32 > 0x7f; 
        let width = if is_wide { 2 } else { 1 };

        if self.state.cursor_col + width > self.state.cols {
            self.state.cursor_col = 0;
            self.state.cursor_row += 1;
        }
        if self.state.cursor_row >= self.state.rows {
            self.state.scroll_up();
        }

        if self.state.cursor_row < self.state.rows {
            let row = self.state.cursor_row;
            let col = self.state.cursor_col;
            let fg = self.state.current_fg;
            let bg = self.state.current_bg;
            let inverse = self.state.current_inverse;
            let cols_limit = self.state.cols;

            let grid = self.state.grid_mut();
            
            grid[row][col] = Cell { c, fg, bg, inverse, is_wide_continuation: false };
            
            if is_wide && col + 1 < cols_limit {
                grid[row][col + 1] = Cell { c: ' ', fg, bg, inverse, is_wide_continuation: true };
                self.state.cursor_col += 2;
            } else {
                self.state.cursor_col += 1;
            }
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.state.cursor_col = 0,
            b'\n' => {
                self.state.cursor_row += 1;
                if self.state.cursor_row >= self.state.rows { self.state.scroll_up(); }
            }
            b'\x08' => { if self.state.cursor_col > 0 { self.state.cursor_col -= 1; } }
            b'\t' => {
                let next_tab = (self.state.cursor_col / 8 + 1) * 8;
                self.state.cursor_col = next_tab.min(self.state.cols - 1);
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &vte::Params, intermediates: &[u8], _ignore: bool, c: char) {
        let mut it = params.iter();
        match c {
            'm' => {
                while let Some(param) = it.next() {
                    match param[0] {
                        0 => {
                            self.state.current_fg = TERM_FG;
                            self.state.current_bg = Color32::TRANSPARENT;
                            self.state.current_inverse = false;
                        }
                        7 => self.state.current_inverse = true,
                        27 => self.state.current_inverse = false,
                        30..=37 => self.state.current_fg = ansi_color(param[0] as u8 - 30),
                        38 => { // Extended FG
                            match it.next().map(|p| p[0]) {
                                Some(5) => if let Some(p) = it.next() { self.state.current_fg = ansi_color(p[0] as u8); },
                                Some(2) => {
                                    let r = it.next().map(|p| p[0] as u8).unwrap_or(0);
                                    let g = it.next().map(|p| p[0] as u8).unwrap_or(0);
                                    let b = it.next().map(|p| p[0] as u8).unwrap_or(0);
                                    self.state.current_fg = Color32::from_rgb(r, g, b);
                                }
                                _ => {}
                            }
                        }
                        39 => self.state.current_fg = TERM_FG,
                        40..=47 => self.state.current_bg = ansi_color(param[0] as u8 - 40),
                        48 => { // Extended BG
                            match it.next().map(|p| p[0]) {
                                Some(5) => if let Some(p) = it.next() { self.state.current_bg = ansi_color(p[0] as u8); },
                                Some(2) => {
                                    let r = it.next().map(|p| p[0] as u8).unwrap_or(0);
                                    let g = it.next().map(|p| p[0] as u8).unwrap_or(0);
                                    let b = it.next().map(|p| p[0] as u8).unwrap_or(0);
                                    self.state.current_bg = Color32::from_rgb(r, g, b);
                                }
                                _ => {}
                            }
                        }
                        49 => self.state.current_bg = Color32::TRANSPARENT,
                        90..=97 => self.state.current_fg = ansi_color(param[0] as u8 - 90 + 8),
                        100..=107 => self.state.current_bg = ansi_color(param[0] as u8 - 100 + 8),
                        _ => {} 
                    }
                }
            }
            'H' | 'f' => {
                let row = params.iter().next().map(|p| p[0].saturating_sub(1) as usize).unwrap_or(0);
                let col = params.iter().nth(1).map(|p| p[0].saturating_sub(1) as usize).unwrap_or(0);
                self.state.cursor_row = row.min(self.state.rows - 1);
                self.state.cursor_col = col.min(self.state.cols - 1);
            }
            'G' => { // CHA - Cursor Horizontal Absolute
                let col = params.iter().next().map(|p| p[0].saturating_sub(1) as usize).unwrap_or(0);
                self.state.cursor_col = col.min(self.state.cols - 1);
            }
            'd' => { // VPA - Cursor Vertical Absolute
                let row = params.iter().next().map(|p| p[0].saturating_sub(1) as usize).unwrap_or(0);
                self.state.cursor_row = row.min(self.state.rows - 1);
            }
            'J' => {
                if params.iter().next().map(|p| p[0]).unwrap_or(0) == 2 {
                    let (c, r) = (self.state.cols, self.state.rows);
                    *self.state.grid_mut() = vec![vec![Cell::default(); c]; r];
                    self.state.cursor_row = 0; self.state.cursor_col = 0;
                }
            }
            'K' => {
                let row = self.state.cursor_row;
                if row < self.state.rows {
                    let col = self.state.cursor_col;
                    for cell in &mut self.state.grid_mut()[row][col..] { *cell = Cell::default(); }
                }
            }
            'X' => { // ECH - Erase Character
                let num = params.iter().next().map(|p| p[0] as usize).unwrap_or(1);
                let row = self.state.cursor_row;
                let col = self.state.cursor_col;
                let limit = (col + num).min(self.state.cols);
                for cell in &mut self.state.grid_mut()[row][col..limit] { *cell = Cell::default(); }
            }
            'P' => { // DCH - Delete Character
                let num = params.iter().next().map(|p| p[0] as usize).unwrap_or(1);
                let row = self.state.cursor_row;
                let col = self.state.cursor_col;
                if row < self.state.rows && col < self.state.cols {
                    let line = &mut self.state.grid_mut()[row];
                    for _ in 0..num {
                        if col < line.len() { line.remove(col); line.push(Cell::default()); }
                    }
                }
            }
            'h' if intermediates == b"?" => { 
                for p in params.iter() {
                    match p[0] {
                        25 => self.state.cursor_visible = true,
                        1 => self.state.application_cursor = true,
                        1049 => {
                            self.state.saved_cursor = (self.state.cursor_row, self.state.cursor_col);
                            self.state.is_alt_screen = true;
                            self.state.alt_grid = vec![vec![Cell::default(); self.state.cols]; self.state.rows];
                            self.state.cursor_row = 0; self.state.cursor_col = 0;
                        }
                        _ => {} 
                    }
                }
            }
            'l' if intermediates == b"?" => { 
                for p in params.iter() {
                    match p[0] {
                        25 => self.state.cursor_visible = false,
                        1 => self.state.application_cursor = false,
                        1049 => {
                            self.state.is_alt_screen = false;
                            self.state.cursor_row = self.state.saved_cursor.0;
                            self.state.cursor_col = self.state.saved_cursor.1;
                        }
                        _ => {} 
                    }
                }
            }
            'A' => self.state.cursor_row = self.state.cursor_row.saturating_sub(params.iter().next().map(|p| p[0] as usize).unwrap_or(1)),
            'B' => self.state.cursor_row = (self.state.cursor_row + params.iter().next().map(|p| p[0] as usize).unwrap_or(1)).min(self.state.rows - 1),
            'C' => self.state.cursor_col = (self.state.cursor_col + params.iter().next().map(|p| p[0] as usize).unwrap_or(1)).min(self.state.cols - 1),
            'D' => self.state.cursor_col = self.state.cursor_col.saturating_sub(params.iter().next().map(|p| p[0] as usize).unwrap_or(1)),
            _ => {} 
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _c: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

pub struct TerminalTab {
    state: Arc<Mutex<TerminalState>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<dyn Write + Send>>,
    last_size: (usize, usize),
    id: Id,
    scroll_offset: usize, 
}

static TERM_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl std::fmt::Debug for TerminalTab { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("TerminalTab").finish() } }

impl Clone for TerminalTab {
    fn clone(&self) -> Self {
        TerminalTab { state: self.state.clone(), master: self.master.clone(), writer: self.writer.clone(), last_size: self.last_size, id: self.id, scroll_offset: self.scroll_offset }
    }
}

impl TabInstance for TerminalTab {
    fn title(&self) -> WidgetText { " Terminal".into() }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        let font_id = FontId::monospace(14.0);
        let char_width = ui.fonts(|f| f.glyph_width(&font_id, 'M')).round();
        let char_size = Vec2::new(char_width, 18.0);
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        if response.clicked() { response.request_focus(); }

        let cols = (rect.width() / char_size.x).floor() as usize;
        let rows = (rect.height() / char_size.y).floor() as usize;

        if cols > 2 && rows > 2 && (cols != self.last_size.0 || rows != self.last_size.1) {
            let mut state = self.state.lock();
            state.resize(rows, cols);
            let _ = self.master.lock().resize(PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 });
            self.last_size = (cols, rows);
        }

        if response.has_focus() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::IMEAllowed(true));
            {
                let state = self.state.lock();
                let viewport_bottom_idx = (state.history.len() + state.grid().len()).saturating_sub(1 + self.scroll_offset);
                let viewport_top_idx = viewport_bottom_idx.saturating_sub(rows - 1);
                let cursor_data_idx = state.history.len() + state.cursor_row;
                if cursor_data_idx >= viewport_top_idx && cursor_data_idx <= viewport_bottom_idx {
                    let r_vis = cursor_data_idx - viewport_top_idx;
                    let cursor_pos = rect.min + Vec2::new(state.cursor_col as f32 * char_size.x, r_vis as f32 * char_size.y);
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::IMERect(Rect::from_min_size(cursor_pos, char_size)));
                }
            }

            ui.input(|i| {
                let mut w = self.writer.lock();
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) => { let _ = w.write_all(text.as_bytes()); }
                        egui::Event::Key { key, pressed: true, modifiers, ..} => {
                            let is_app_mode = self.state.lock().application_cursor;
                            let seq = match key {
                                Key::Enter => Some("\r"),
                                Key::Backspace => if modifiers.ctrl { Some("\x17") } else { Some("\x7f") },
                                Key::ArrowUp => if is_app_mode { Some("\x1bOA") } else { Some("\x1b[A") },
                                Key::ArrowDown => if is_app_mode { Some("\x1bOB") } else { Some("\x1b[B") },
                                Key::ArrowRight => if is_app_mode { Some("\x1bOC") } else { Some("\x1b[C") },
                                Key::ArrowLeft => if is_app_mode { Some("\x1bOD") } else { Some("\x1b[D") },
                                Key::Tab => Some("\t"),
                                Key::Escape => Some("\x1b"),
                                Key::C if modifiers.ctrl => Some("\x03"),
                                Key::D if modifiers.ctrl => Some("\x04"),
                                Key::L if modifiers.ctrl => Some("\x0c"),
                                Key::Z if modifiers.ctrl => Some("\x1a"),
                                _ => None,
                            };
                            if let Some(s) = seq { let _ = w.write_all(s.as_bytes()); }
                        }
                        _ => {} 
                    }
                }
                let _ = w.flush();
            });
        }
        
        if response.hovered() {
            ui.input(|i| {
                if i.raw_scroll_delta.y != 0.0 {
                    let rows_scrolled = (i.raw_scroll_delta.y / char_size.y).round() as isize;
                    if rows_scrolled > 0 { self.scroll_offset = self.scroll_offset.saturating_add(rows_scrolled as usize); }
                    else { self.scroll_offset = self.scroll_offset.saturating_sub((-rows_scrolled) as usize); }
                }
            });
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, TERM_BG);

        let state = self.state.lock();
        let history_len = state.history.len();
        let total_content_rows = history_len + state.grid().len();
        let max_scroll = history_len; 
        let current_scroll = self.scroll_offset.min(max_scroll);
        let viewport_bottom_idx = total_content_rows.saturating_sub(1 + current_scroll);
        let viewport_top_idx = viewport_bottom_idx.saturating_sub(rows - 1);
        
        for r_vis in 0..rows {
            let data_idx = viewport_top_idx + r_vis;
            let row_cells = if data_idx < history_len { state.history.get(data_idx) } else { state.grid().get(data_idx - history_len) };
            let line_pos = rect.min + Vec2::new(0.0, r_vis as f32 * char_size.y);
            painter.rect_filled(Rect::from_min_size(line_pos, Vec2::new(rect.width(), char_size.y)), 0.0, TERM_BG);

            if let Some(cells) = row_cells {
                let mut current_idx = 0;
                while current_idx < cells.len().min(cols) {
                    let start = current_idx;
                    let cell = cells[start];
                    if cell.is_wide_continuation { current_idx += 1; continue; }
                    let (mut fg, mut bg) = (cell.fg, cell.bg);
                    if cell.inverse {
                        let res_fg = if fg == Color32::TRANSPARENT { TERM_FG } else { fg };
                        let res_bg = if bg == Color32::TRANSPARENT { TERM_BG } else { bg };
                        fg = res_bg; bg = res_fg;
                    }
                    let mut text = String::new();
                    text.push(cell.c);
                    current_idx += 1;
                    while current_idx < cells.len().min(cols) {
                        let next = cells[current_idx];
                        if next.is_wide_continuation { current_idx += 1; continue; }
                        if next.fg != cell.fg || next.bg != cell.bg || next.inverse != cell.inverse { break; }
                        text.push(next.c);
                        current_idx += 1;
                    }
                    let segment_pos = line_pos + Vec2::new(start as f32 * char_size.x, 0.0);
                    if bg != Color32::TRANSPARENT && bg != TERM_BG {
                        painter.rect_filled(Rect::from_min_size(segment_pos, Vec2::new((current_idx - start) as f32 * char_size.x, char_size.y)), 0.0, bg);
                    }
                    let mut job = LayoutJob::default();
                    job.append(&text, 0.0, TextFormat { font_id: font_id.clone(), color: fg, ..Default::default() });
                    painter.galley(segment_pos, ui.fonts(|f| f.layout_job(job)), Color32::TRANSPARENT);
                }
            }

            if state.cursor_visible && data_idx == (history_len + state.cursor_row) {
                 painter.rect_filled(Rect::from_min_size(line_pos + Vec2::new(state.cursor_col as f32 * char_size.x, 0.0), char_size), 0.0, Color32::from_rgba_unmultiplied(200, 200, 200, 150));
            }
        }
        
        if history_len > 0 {
            let handle_h = ((rows as f32 / total_content_rows as f32).max(0.1) * rect.height()).min(rect.height());
            let handle_y = rect.min.y + ((1.0 - (current_scroll as f32 / max_scroll as f32)) * (rect.height() - handle_h));
            painter.rect_filled(Rect::from_min_size(Pos2::new(rect.max.x - 6.0, handle_y), Vec2::new(6.0, handle_h)), 3.0, Color32::from_gray(100));
        }
        ui.ctx().request_repaint();
    }
    fn box_clone(&self) -> Box<dyn TabInstance> { Box::new(self.clone()) }
}

pub struct TerminalPlugin;
impl Plugin for TerminalPlugin {
    fn name(&self) -> &str { "terminal" }
    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("Terminal").clicked() {
            if let Ok(tab) = create_terminal_tab() { control.push(AppCommand::OpenTab(Tab::new(Box::new(tab)))); }
            ui.close_menu();
        }
    }
}

fn create_terminal_tab() -> Result<TerminalTab, Box<dyn std::error::Error + Send + Sync>> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })?;
    #[cfg(windows)] let cmd = CommandBuilder::new("powershell.exe");
    #[cfg(not(windows))] let cmd = CommandBuilder::new("bash");
    let _child = pair.slave.spawn_command(cmd)?;
    let writer = pair.master.take_writer()?;
    let mut reader = pair.master.try_clone_reader()?;
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
                    for byte in &buffer[..n] { parser.advance(&mut handler, *byte); }
                }
                Err(_) => break,
            }
        }
    });
    Ok(TerminalTab { state, master: Arc::new(Mutex::new(pair.master)), writer: Arc::new(Mutex::new(writer)), last_size: (80, 24), id: Id::new("terminal_".to_string() + &TERM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string()), scroll_offset: 0 })
}
pub fn create() -> TerminalPlugin { TerminalPlugin }