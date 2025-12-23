use std::sync::Arc;
use std::io::{Write, Read};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize, MasterPty};
use vte::{Parser, Perform};
use egui::{Ui, WidgetText, Color32, FontId, Rect, Vec2, Key, Sense};
use egui::text::{LayoutJob, TextFormat};
use crate::{Tab, Plugin, AppCommand, TabInstance};

// ----------------------------------------------------------------------------
// Constants & Colors
// ----------------------------------------------------------------------------

const TERM_BG: Color32 = Color32::from_rgb(15, 15, 15);
const TERM_FG: Color32 = Color32::from_rgb(210, 210, 210);

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cell {
    c: char,
    fg: Color32,
    bg: Color32,
    bold: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
    is_wide_continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self { 
            c: ' ',
            fg: TERM_FG,
            bg: Color32::TRANSPARENT,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
            is_wide_continuation: false 
        }
    }
}

fn ansi_color(code: u8) -> Color32 {
    match code {
        0 => Color32::from_rgb(0, 0, 0),        // Black
        1 => Color32::from_rgb(205, 0, 0),      // Red
        2 => Color32::from_rgb(0, 205, 0),      // Green
        3 => Color32::from_rgb(205, 205, 0),    // Yellow
        4 => Color32::from_rgb(0, 0, 238),      // Blue
        5 => Color32::from_rgb(205, 0, 205),    // Magenta
        6 => Color32::from_rgb(0, 205, 205),    // Cyan
        7 => Color32::from_rgb(229, 229, 229),  // White
        8 => Color32::from_rgb(127, 127, 127),  // Bright Black
        9 => Color32::from_rgb(255, 0, 0),      // Bright Red
        10 => Color32::from_rgb(0, 255, 0),     // Bright Green
        11 => Color32::from_rgb(255, 255, 0),   // Bright Yellow
        12 => Color32::from_rgb(92, 92, 255),   // Bright Blue
        13 => Color32::from_rgb(255, 0, 255),   // Bright Magenta
        14 => Color32::from_rgb(0, 255, 255),   // Bright Cyan
        15 => Color32::from_rgb(255, 255, 255), // Bright White
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
    current_bold: bool,
    current_italic: bool,
    current_underline: bool,
    current_inverse: bool,
    
    cursor_visible: bool,
    application_cursor: bool,

    scroll_top: usize,
    scroll_bottom: usize,
    
    dirty: bool,
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
            current_bold: false,
            current_italic: false,
            current_underline: false,
            current_inverse: false,
            cursor_visible: true,
            application_cursor: false,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            dirty: true,
        }
    }

    fn grid_mut(&mut self) -> &mut Vec<Vec<Cell>> {
        if self.is_alt_screen { &mut self.alt_grid } else { &mut self.primary_grid }
    }

    fn grid(&self) -> &Vec<Vec<Cell>> {
        if self.is_alt_screen { &self.alt_grid } else { &self.primary_grid }
    }

    fn scroll_up(&mut self) {
        let (top, bottom) = (self.scroll_top, self.scroll_bottom);
        let (r, c) = (self.rows, self.cols);
        let is_alt = self.is_alt_screen;
        
        if top >= bottom || bottom >= r { return; }

        let grid = if is_alt { &mut self.alt_grid } else { &mut self.primary_grid };

        if top == 0 && bottom == r - 1 {
            let old_row = grid.remove(0);
            grid.push(vec![Cell::default(); c]);
            if !is_alt {
                self.history.push(old_row);
                if self.history.len() > 1000 { self.history.remove(0); }
            }
        } else {
            grid.remove(top);
            grid.insert(bottom, vec![Cell::default(); c]);
        }
        self.dirty = true;
    }

    fn resize(&mut self, new_rows: usize, new_cols: usize) {
        if new_rows == 0 || new_cols == 0 { return; }
        if new_rows == self.rows && new_cols == self.cols { return; }

        let resize_one = |grid: &mut Vec<Vec<Cell>>| {
            grid.truncate(new_rows);
            while grid.len() < new_rows {
                grid.push(vec![Cell::default(); new_cols]);
            }
            for row in grid.iter_mut() {
                row.truncate(new_cols);
                while row.len() < new_cols {
                    row.push(Cell::default());
                }
            }
        };

        resize_one(&mut self.primary_grid);
        resize_one(&mut self.alt_grid);

        self.rows = new_rows;
        self.cols = new_cols;
        self.scroll_top = 0;
        self.scroll_bottom = new_rows.saturating_sub(1);
        self.cursor_row = self.cursor_row.min(new_rows - 1);
        self.cursor_col = self.cursor_col.min(new_cols - 1);
        self.dirty = true;
    }
}

// ----------------------------------------------------------------------------
// ANSI Logic (LogHandler)
// ----------------------------------------------------------------------------

struct LogHandler<'a> {
    state: &'a mut TerminalState,
}

impl<'a> Perform for LogHandler<'a> {
    fn print(&mut self, c: char) {
        let cols = self.state.cols;
        if self.state.cursor_col >= cols {
            self.state.cursor_col = 0;
            self.state.cursor_row += 1;
        }
        if self.state.cursor_row > self.state.scroll_bottom {
            self.state.cursor_row = self.state.scroll_bottom;
            self.state.scroll_up();
        }

        let r = self.state.cursor_row;
        let c_idx = self.state.cursor_col;
        if r < self.state.rows && c_idx < cols {
            let cell = Cell {
                c,
                fg: self.state.current_fg,
                bg: self.state.current_bg,
                bold: self.state.current_bold,
                italic: self.state.current_italic,
                underline: self.state.current_underline,
                inverse: self.state.current_inverse,
                is_wide_continuation: false,
            };
            self.state.grid_mut()[r][c_idx] = cell;
            self.state.cursor_col += 1;
            self.state.dirty = true;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.state.cursor_col = 0,
            b'\n' | b'\x0b' | b'\x0c' => {
                self.state.cursor_row += 1;
                if self.state.cursor_row > self.state.scroll_bottom {
                    self.state.cursor_row = self.state.scroll_bottom;
                    self.state.scroll_up();
                }
            }
            b'\x08' => { if self.state.cursor_col > 0 { self.state.cursor_col -= 1; } }
            b'\t' => {
                let next = (self.state.cursor_col / 8 + 1) * 8;
                self.state.cursor_col = next.min(self.state.cols - 1);
            }
            7 => { /* Bell */ } // This is ASCII BEL character
            _ => {} // Other control characters are ignored for now
        }
        self.state.dirty = true;
    }

    fn csi_dispatch(&mut self, params: &vte::Params, intermediates: &[u8], _ignore: bool, c: char) {
        let p = |idx| params.iter().nth(idx).map(|v| v[0] as usize).unwrap_or(0);

        match c {
            'm' => { // SGR
                let mut it = params.iter();
                while let Some(param) = it.next() {
                    match param[0] {
                        0 => {
                            self.state.current_fg = TERM_FG;
                            self.state.current_bg = Color32::TRANSPARENT;
                            self.state.current_bold = false;
                            self.state.current_italic = false;
                            self.state.current_underline = false;
                            self.state.current_inverse = false;
                        }
                        1 => self.state.current_bold = true,
                        3 => self.state.current_italic = true,
                        4 => self.state.current_underline = true,
                        7 => self.state.current_inverse = true,
                        22 => self.state.current_bold = false,
                        23 => self.state.current_italic = false,
                        24 => self.state.current_underline = false,
                        27 => self.state.current_inverse = false,
                        30..=37 => self.state.current_fg = ansi_color(param[0] as u8 - 30),
                        38 => {
                            match it.next().map(|v| v[0]) {
                                Some(5) => if let Some(v) = it.next() { self.state.current_fg = ansi_color(v[0] as u8); },
                                Some(2) => {
                                    let r = it.next().map(|v| v[0] as u8).unwrap_or(0);
                                    let g = it.next().map(|v| v[0] as u8).unwrap_or(0);
                                    let b = it.next().map(|v| v[0] as u8).unwrap_or(0);
                                    self.state.current_fg = Color32::from_rgb(r, g, b);
                                }
                                _ => {} // Ignore unsupported SGR color modes
                            }
                        }
                        39 => self.state.current_fg = TERM_FG,
                        40..=47 => self.state.current_bg = ansi_color(param[0] as u8 - 40),
                        48 => {
                            match it.next().map(|v| v[0]) {
                                Some(5) => if let Some(v) = it.next() { self.state.current_bg = ansi_color(v[0] as u8); },
                                Some(2) => {
                                    let r = it.next().map(|v| v[0] as u8).unwrap_or(0);
                                    let g = it.next().map(|v| v[0] as u8).unwrap_or(0);
                                    let b = it.next().map(|v| v[0] as u8).unwrap_or(0);
                                    self.state.current_bg = Color32::from_rgb(r, g, b);
                                }
                                _ => {} // Ignore unsupported SGR color modes
                            }
                        }
                        49 => self.state.current_bg = Color32::TRANSPARENT,
                        90..=97 => self.state.current_fg = ansi_color(param[0] as u8 - 90 + 8),
                        100..=107 => self.state.current_bg = ansi_color(param[0] as u8 - 100 + 8),
                        _ => {} // Ignore unsupported SGR parameters
                    }
                }
            }
            'H' | 'f' => {
                let r = p(0).saturating_sub(1);
                let col = p(1).saturating_sub(1);
                self.state.cursor_row = r.min(self.state.rows - 1);
                self.state.cursor_col = col.min(self.state.cols - 1);
            }
            'A' => self.state.cursor_row = self.state.cursor_row.saturating_sub(p(0).max(1)),
            'B' => self.state.cursor_row = (self.state.cursor_row + p(0).max(1)).min(self.state.rows - 1),
            'C' => self.state.cursor_col = (self.state.cursor_col + p(0).max(1)).min(self.state.cols - 1),
            'D' => self.state.cursor_col = self.state.cursor_col.saturating_sub(p(0).max(1)),
            'G' => self.state.cursor_col = p(0).saturating_sub(1).min(self.state.cols - 1),
            'd' => self.state.cursor_row = p(0).saturating_sub(1).min(self.state.rows - 1),
            'J' => {
                let (rows, cols, r, c) = (self.state.rows, self.state.cols, self.state.cursor_row, self.state.cursor_col);
                let grid = self.state.grid_mut();
                match p(0) {
                    0 => {
                        for col in c..cols { grid[r][col] = Cell::default(); }
                        for row in (r + 1)..rows { for col in 0..cols { grid[row][col] = Cell::default(); } }
                    }
                    1 => {
                        for row in 0..r { for col in 0..cols { grid[row][col] = Cell::default(); } }
                        for col in 0..=c.min(cols - 1) { grid[r][col] = Cell::default(); }
                    }
                    2 | 3 => { for row in 0..rows { for col in 0..cols { grid[row][col] = Cell::default(); } } } // 3 clears entire screen and moves cursor to home
                    _ => {} // Ignore unsupported erase modes
                }
            }
            'K' => {
                let (cols, r, c) = (self.state.cols, self.state.cursor_row, self.state.cursor_col);
                let grid = self.state.grid_mut();
                if r < grid.len() {
                    match p(0) {
                        0 => for col in c..cols { grid[r][col] = Cell::default(); },
                        1 => for col in 0..=c.min(cols - 1) { grid[r][col] = Cell::default(); },
                        2 => for col in 0..cols { grid[r][col] = Cell::default(); },
                        _ => {} // Ignore unsupported erase modes
                    }
                }
            }
            'r' => {
                let top = p(0).saturating_sub(1);
                let bot = if p(1) == 0 { self.state.rows } else { p(1) }.saturating_sub(1);
                self.state.scroll_top = top;
                self.state.scroll_bottom = bot.min(self.state.rows - 1);
            }
            'h' if intermediates == b"?" => {
                for param in params.iter() {
                    match param[0] {
                        1 => self.state.application_cursor = true,
                        25 => self.state.cursor_visible = true,
                        1049 => {
                            self.state.saved_cursor = (self.state.cursor_row, self.state.cursor_col);
                            self.state.is_alt_screen = true;
                            let (rows, cols) = (self.state.rows, self.state.cols);
                            self.state.alt_grid = vec![vec![Cell::default(); cols]; rows];
                            self.state.cursor_row = 0; self.state.cursor_col = 0;
                        }
                        _ => {} // Ignore unsupported DECSET modes
                    }
                }
            }
            'l' if intermediates == b"?" => {
                for param in params.iter() {
                    match param[0] {
                        1 => self.state.application_cursor = false,
                        25 => self.state.cursor_visible = false,
                        1049 => {
                            self.state.is_alt_screen = false;
                            self.state.cursor_row = self.state.saved_cursor.0.min(self.state.rows - 1);
                            self.state.cursor_col = self.state.saved_cursor.1.min(self.state.cols - 1);
                        }
                        _ => {} // Ignore unsupported DECRST modes
                    }
                }
            }
            _ => {} // Ignore unsupported CSI sequences
        }
        self.state.dirty = true;
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => self.state.saved_cursor = (self.state.cursor_row, self.state.cursor_col),
            b'8' => {
                self.state.cursor_row = self.state.saved_cursor.0.min(self.state.rows - 1);
                self.state.cursor_col = self.state.saved_cursor.1.min(self.state.cols - 1);
            }
            b'M' => { // Reverse Index
                if self.state.cursor_row == self.state.scroll_top {
                    // Scroll down
                    let (top, bottom) = (self.state.scroll_top, self.state.scroll_bottom);
                    let cols = self.state.cols;
                    let grid = self.state.grid_mut();
                    grid.remove(bottom);
                    grid.insert(top, vec![Cell::default(); cols]);
                } else {
                    self.state.cursor_row = self.state.cursor_row.saturating_sub(1);
                }
            }
            _ => {} // Ignore unsupported ESC sequences
        }
        self.state.dirty = true;
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _c: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
}

// ----------------------------------------------------------------------------
// Tab Implementation
// ----------------------------------------------------------------------------

pub struct TerminalTab {
    state: Arc<Mutex<TerminalState>>,
    writer: Arc<Mutex<Box<dyn Write + Send>> >,
    master: Arc<Mutex<Box<dyn MasterPty + Send>> >,
    last_size: (usize, usize),
    scroll_offset: usize,
}

impl std::fmt::Debug for TerminalTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalTab").finish()
    }
}

impl Clone for TerminalTab {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            writer: self.writer.clone(),
            master: self.master.clone(),
            last_size: self.last_size,
            scroll_offset: self.scroll_offset,
        }
    }
}

impl TabInstance for TerminalTab {
    fn title(&self) -> WidgetText { "Terminal".into() }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        let font_id = FontId::monospace(14.0);
        let char_size = ui.fonts(|f| {
            let width = f.glyph_width(&font_id, 'M');
            let height = f.row_height(&font_id);
            Vec2::new(width, height)
        });

        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        
        if response.clicked() {
            ui.memory_mut(|m| m.request_focus(response.id));
        }

        // Resize
        let cols = (rect.width() / char_size.x).floor() as usize;
        let rows = (rect.height() / char_size.y).floor() as usize;
        if cols > 0 && rows > 0 && (cols != self.last_size.0 || rows != self.last_size.1) {
            self.state.lock().resize(rows, cols);
            let _ = self.master.lock().resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
            self.last_size = (cols, rows);
        }

        // Input
        if response.has_focus() {
            let mut writer = self.writer.lock();
            let mut state = self.state.lock();
            let is_app_mode = state.application_cursor;

            ui.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(t) => {
                            let _ = writer.write_all(t.as_bytes());
                        }
                        egui::Event::Key { key, pressed: true, modifiers, .. } => {
                            let seq = match key {
                                Key::Enter => Some("\r".to_string()),
                                Key::Backspace => Some("\x7f".to_string()),
                                Key::Tab => Some("\t".to_string()),
                                Key::Escape => Some("\x1b".to_string()),
                                Key::ArrowUp => Some(if is_app_mode { "\x1bOA" } else { "\x1b[A" }.to_string()),
                                Key::ArrowDown => Some(if is_app_mode { "\x1bOB" } else { "\x1b[B" }.to_string()),
                                Key::ArrowRight => Some(if is_app_mode { "\x1bOC" } else { "\x1b[C" }.to_string()),
                                Key::ArrowLeft => Some(if is_app_mode { "\x1bOD" } else { "\x1b[D" }.to_string()),
                                _ if modifiers.ctrl => {
                                    let k = format!("{:?}", key);
                                    if k.len() == 1 {
                                        let c = k.chars().next().unwrap();
                                        if c >= 'A' && c <= 'Z' {
                                            Some(((c as u8 - b'A' + 1) as char).to_string())
                                        } else { None }
                                    } else {
                                        match key {
                                            Key::C => Some("\x03".to_string()),
                                            Key::D => Some("\x04".to_string()),
                                            Key::L => Some("\x0c".to_string()),
                                            Key::Z => Some("\x1a".to_string()),
                                            _ => None
                                        }
                                    }
                                }
                                _ => None,
                            };
                            if let Some(s) = seq {
                                let _ = writer.write_all(s.as_bytes());
                            }
                        }
                        _ => {} // Ignore other input events
                    }
                }
            });
            state.dirty = false;
        }

        // Scrolling
        if response.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta.y);
            if delta != 0.0 {
                let lines = (delta / char_size.y).round() as isize;
                let state = self.state.lock();
                let history_len = state.history.len();
                if lines > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_add(lines as usize);
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_sub((-lines) as usize);
                }
                self.scroll_offset = self.scroll_offset.min(history_len);
            }
        }

        // Render
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, TERM_BG);

        let state = self.state.lock();
        let history = &state.history;
        let grid = state.grid();
        
        let total_rows = history.len() + grid.len();
        let visible_start = total_rows.saturating_sub(rows + self.scroll_offset);
        
        for r in 0..rows {
            let row_idx = visible_start + r;
            if row_idx >= total_rows { break; }
            
            let cells = if row_idx < history.len() {
                &history[row_idx]
            } else {
                &grid[row_idx - history.len()]
            };

            let row_rect = Rect::from_min_size(
                rect.min + Vec2::new(0.0, r as f32 * char_size.y),
                Vec2::new(rect.width(), char_size.y)
            );

            let mut x_offset = 0.0;
            let mut i = 0;
            while i < cells.len().min(cols) {
                let start_idx = i;
                let cell = &cells[i];
                let mut text = String::new();
                text.push(cell.c);
                i += 1;
                
                // Group cells with same style
                while i < cells.len().min(cols) && 
                      cells[i].fg == cell.fg && 
                      cells[i].bg == cell.bg && 
                      cells[i].bold == cell.bold &&
                      cells[i].italic == cell.italic &&
                      cells[i].underline == cell.underline &&
                      cells[i].inverse == cell.inverse {
                    text.push(cells[i].c);
                    i += 1;
                }

                let (mut fg, mut bg) = (cell.fg, cell.bg);
                if cell.inverse {
                    std::mem::swap(&mut fg, &mut bg);
                    if fg == Color32::TRANSPARENT { fg = TERM_BG; }
                    if bg == Color32::TRANSPARENT { bg = TERM_FG; }
                }

                let text_pos = row_rect.min + Vec2::new(x_offset, 0.0);
                let segment_width = (i - start_idx) as f32 * char_size.x;
                
                if bg != Color32::TRANSPARENT {
                    painter.rect_filled(
                        Rect::from_min_size(text_pos, Vec2::new(segment_width, char_size.y)),
                        0.0,
                        bg
                    );
                }

                let mut job = LayoutJob::default();
                let format = TextFormat {
                    font_id: font_id.clone(),
                    color: fg,
                    ..Default::default()
                };
                if cell.bold { /* bold is usually handled by font but egui doesn't have easy bold monospace fallback usually */ }
                if cell.italic { /* same for italic */ }
                if cell.underline { /* can be added to format */ }
                
                job.append(&text, 0.0, format);
                painter.galley(text_pos, ui.fonts(|f| f.layout_job(job)), Color32::TRANSPARENT);
                
                x_offset += segment_width;
            }

            // Draw cursor
            if state.cursor_visible && !state.is_alt_screen && (row_idx == history.len() + state.cursor_row) {
                let cursor_pos = row_rect.min + Vec2::new(state.cursor_col as f32 * char_size.x, 0.0);
                painter.rect_filled(Rect::from_min_size(cursor_pos, char_size), 0.0, Color32::from_gray(200).linear_multiply(0.5));
            }
        }

        ui.ctx().request_repaint();
    }

    fn box_clone(&self) -> Box<dyn TabInstance> {
        Box::new(self.clone())
    }
}

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn name(&self) -> &str { "terminal" }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("New Terminal").clicked() {
            if let Ok(tab) = create_terminal_tab() {
                control.push(AppCommand::OpenTab(Tab::new(Box::new(tab))));
            }
            ui.close_menu();
        }
    }
}

fn create_terminal_tab() -> anyhow::Result<TerminalTab> {
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

    let mut _child = pair.slave.spawn_command(cmd)?;
    
    let writer = pair.master.take_writer()?;
    let mut reader = pair.master.try_clone_reader()?;
    
    let state = Arc::new(Mutex::new(TerminalState::new(24, 80)));
    let s_thread = state.clone();

    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        let mut parser = Parser::new();
        while let Ok(n) = reader.read(&mut buffer) {
            if n == 0 { break; }
            let mut s = s_thread.lock();
            let mut handler = LogHandler { state: &mut *s };
            for byte in &buffer[..n] {
                parser.advance(&mut handler, *byte);
            }
        }
    });

    Ok(TerminalTab {
        state,
        writer: Arc::new(Mutex::new(writer)),
        master: Arc::new(Mutex::new(pair.master)),
        last_size: (80, 24),
        scroll_offset: 0,
    })
}

pub fn create() -> TerminalPlugin {
    TerminalPlugin
}
