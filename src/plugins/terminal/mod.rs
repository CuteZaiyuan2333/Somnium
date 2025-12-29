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
                if self.history.len() > 5000 { self.history.remove(0); }
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
        let is_wide = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) > 1;
        let width = if is_wide { 2 } else { 1 };
        
        let cols = self.state.cols;
        if self.state.cursor_col + width > cols {
            self.state.cursor_col = 0;
            self.state.cursor_row += 1;
        }
        if self.state.cursor_row > self.state.scroll_bottom {
            self.state.cursor_row = self.state.scroll_bottom;
            self.state.scroll_up();
        }

        let r = self.state.cursor_row;
        let c_idx = self.state.cursor_col;
        if r < self.state.rows {
            let cell_style = Cell {
                c,
                fg: self.state.current_fg,
                bg: self.state.current_bg,
                bold: self.state.current_bold,
                italic: self.state.current_italic,
                underline: self.state.current_underline,
                inverse: self.state.current_inverse,
                is_wide_continuation: false,
            };

            let grid = self.state.grid_mut();
            grid[r][c_idx] = cell_style;

            if is_wide && c_idx + 1 < cols {
                let mut continuation = cell_style;
                continuation.c = ' ';
                continuation.is_wide_continuation = true;
                grid[r][c_idx + 1] = continuation;
            }
            self.state.cursor_col += width;
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
            'X' => { // ECH - Erase Character
                let n = p(0).max(1);
                let (cols, r, c) = (self.state.cols, self.state.cursor_row, self.state.cursor_col);
                let grid = self.state.grid_mut();
                if r < grid.len() {
                    for i in 0..n {
                        if c + i < cols {
                            grid[r][c + i] = Cell::default();
                        }
                    }
                }
            }
            '@' => { // ICH - Insert Character
                let n = p(0).max(1);
                let (_cols, r, c) = (self.state.cols, self.state.cursor_row, self.state.cursor_col);
                let grid = self.state.grid_mut();
                if r < grid.len() {
                    let row = &mut grid[r];
                    for _ in 0..n {
                        row.insert(c, Cell::default());
                        row.pop(); // Remove from end to maintain width
                    }
                }
            }
            'P' => { // DCH - Delete Character
                let n = p(0).max(1);
                let (_cols, r, c) = (self.state.cols, self.state.cursor_row, self.state.cursor_col);
                let grid = self.state.grid_mut();
                if r < grid.len() {
                    let row = &mut grid[r];
                    for _ in 0..n {
                        if c < row.len() {
                            row.remove(c);
                            row.push(Cell::default()); // Add to end
                        }
                    }
                }
            }
            'L' => { // IL - Insert Line
                let n = p(0).max(1);
                let (top, bottom) = (self.state.scroll_top, self.state.scroll_bottom);
                let r = self.state.cursor_row;
                let cols = self.state.cols; // Capture cols before mut borrow
                if r >= top && r <= bottom {
                    let grid = self.state.grid_mut();
                    for _ in 0..n {
                        grid.remove(bottom);
                        grid.insert(r, vec![Cell::default(); cols]);
                    }
                }
            }
            'M' => { // DL - Delete Line
                let n = p(0).max(1);
                let (top, bottom) = (self.state.scroll_top, self.state.scroll_bottom);
                let r = self.state.cursor_row;
                let cols = self.state.cols; // Capture cols before mut borrow
                if r >= top && r <= bottom {
                    let grid = self.state.grid_mut();
                    for _ in 0..n {
                        grid.remove(r);
                        grid.insert(bottom, vec![Cell::default(); cols]);
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
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    last_size: (usize, usize),
    ctx: egui::Context,
    input_buffer: String,
    is_composing: bool,
    selection_start: Option<(usize, usize)>,
    selection_end: Option<(usize, usize)>,
    drag_start: Option<(usize, usize)>,
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
            ctx: self.ctx.clone(),
            input_buffer: String::new(),
            is_composing: false,
            selection_start: None,
            selection_end: None,
            drag_start: None,
        }
    }
}

impl TabInstance for TerminalTab {
    fn title(&self) -> WidgetText { "Terminal".into() }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        let font_id = FontId::monospace(14.0);
        let char_size = ui.fonts_mut(|f| {
            let width = f.glyph_width(&font_id, 'M');
            let height = f.row_height(&font_id);
            Vec2::new(width, height)
        });

        // 1. Calculate PTY size based on available area
        let available_size = ui.available_size();
        let cols = (available_size.x / char_size.x).floor() as usize;
        let rows = (available_size.y / char_size.y).floor() as usize;
        
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

        let state_lock = self.state.lock();
        let history_len = state_lock.history.len();
        let grid_len = state_lock.rows;
        let total_rows = history_len + grid_len;
        drop(state_lock);

        let mut output_to_write = String::new();

        // 2. Use ScrollArea for native scrolling and scrollbar
        egui::ScrollArea::vertical()
            .id_salt("terminal_scroll")
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show_viewport(ui, |ui, viewport| {
                let content_size = Vec2::new(available_size.x, total_rows as f32 * char_size.y);
                let (rect, response) = ui.allocate_at_least(content_size, Sense::click_and_drag());
                
                // 1. Invisible Input Overlay (Bottom Layer)
                // We place this BEFORE the background is drawn so that the terminal's 
                // background covers the TextEdit's default blinking cursor.
                let input_rect = viewport.translate(rect.min.to_vec2());
                
                // Use a scope to make the TextEdit truly invisible (cursor, selection, etc.)
                let input_response = ui.scope(|ui| {
                    ui.visuals_mut().selection.bg_fill = Color32::TRANSPARENT;
                    ui.visuals_mut().selection.stroke.color = Color32::TRANSPARENT;
                    // Some egui versions use fg_stroke for the cursor
                    ui.visuals_mut().widgets.active.fg_stroke.color = Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.hovered.fg_stroke.color = Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.fg_stroke.color = Color32::TRANSPARENT;

                    ui.put(
                        input_rect,
                        egui::TextEdit::multiline(&mut self.input_buffer)
                            .id_salt(response.id)
                            .frame(false)
                            .text_color(Color32::TRANSPARENT)
                            .cursor_at_end(true)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                    )
                }).inner;

                if response.clicked() {
                    ui.memory_mut(|m| m.request_focus(response.id));
                }

                // 2. Render Background (Middle Layer)
                // This will overwrite/cover anything drawn by the TextEdit above.
                let painter = ui.painter_at(rect);
                painter.rect_filled(viewport.translate(rect.min.to_vec2()), 0.0, TERM_BG);

                // 3. Handle Input Events
                if input_response.has_focus() || input_response.lost_focus() {
                    if let Some(pos) = input_response.interact_pointer_pos() {
                        let rel_pos = pos - rect.min;
                        let col = (rel_pos.x / char_size.x).floor() as usize;
                        let row_idx = (rel_pos.y / char_size.y).floor() as usize;
                        
                        if input_response.drag_started() {
                            self.drag_start = Some((row_idx, col));
                            self.selection_start = Some((row_idx, col));
                            self.selection_end = Some((row_idx, col));
                        } else if input_response.dragged() {
                            if let Some(_) = self.drag_start {
                                self.selection_end = Some((row_idx, col));
                            }
                        } else if input_response.clicked() {
                            self.selection_start = None;
                            self.selection_end = None;
                            self.drag_start = None;
                        }
                    }
                }

                if input_response.has_focus() || input_response.lost_focus() {
                    let mut writer = self.writer.lock();
                    let state = self.state.lock();
                    let is_app_mode = state.application_cursor;
                    drop(state);
        
                    let mut text_to_copy = None;
                    ui.input(|i| {
                        for event in &i.events {
                            match event {
                                egui::Event::Ime(ime_event) => {
                                    match ime_event {
                                        egui::ImeEvent::Preedit(text) => { self.is_composing = !text.is_empty(); }
                                        egui::ImeEvent::Commit(text) => { self.is_composing = false; output_to_write.push_str(&text); }
                                        egui::ImeEvent::Disabled => { self.is_composing = false; }
                                        _ => {}
                                    }
                                }
                                egui::Event::Text(text) => {
                                    let is_handled_control = if text.len() == 1 {
                                        let c = text.chars().next().unwrap();
                                        c == '\n' || c == '\r' || c == '\t' || c == '\x08' || c == '\x7f' || c == '\x1b'
                                    } else { false };
                                    if !is_handled_control { output_to_write.push_str(&text.replace("\n", "\r")); }
                                }
                                egui::Event::Paste(text) => {
                                    if !self.is_composing { output_to_write.push_str(&text.replace("\n", "\r")); }
                                }
                                egui::Event::Copy => {
                                    if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                                        let (s, e) = if start <= end { (start, end) } else { (end, start) };
                                        let state = self.state.lock();
                                        let mut text = String::new();
                                        let history_len = state.history.len();
                                        let total_rows = history_len + state.rows;
                                        for r in s.0..=e.0 {
                                            if r >= total_rows { break; }
                                            let cells = if r < history_len { &state.history[r] } else { &state.grid()[r - history_len] };
                                            let c_start = if r == s.0 { s.1 } else { 0 };
                                            let c_end = if r == e.0 { (e.1 + 1).min(cells.len()) } else { cells.len() };
                                            for c in c_start..c_end {
                                                if c < cells.len() {
                                                    let cell = &cells[c];
                                                    if !cell.is_wide_continuation { text.push(cell.c); }
                                                } else { text.push(' '); }
                                            }
                                            if r != e.0 { text.push('\n'); }
                                        }
                                        text_to_copy = Some(text);
                                    }
                                }
                                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                                    if self.is_composing { continue; }
                                    if *key == Key::C && modifiers.ctrl {
                                        if self.selection_start.is_some() && self.selection_end.is_some() { continue; }
                                    }
                                    let seq = match key {
                                        Key::Enter => Some("\r".to_string()),
                                        Key::Backspace => Some("\x7f".to_string()),
                                        Key::Tab => Some("\t".to_string()),
                                        Key::Escape => Some("\x1b".to_string()),
                                        Key::ArrowUp => Some(if is_app_mode { "\x1bOA" } else { "\x1b[A" }.to_string()),
                                        Key::ArrowDown => Some(if is_app_mode { "\x1bOB" } else { "\x1b[B" }.to_string()),
                                        Key::ArrowRight => Some(if is_app_mode { "\x1bOC" } else { "\x1b[C" }.to_string()),
                                        Key::ArrowLeft => Some(if is_app_mode { "\x1bOD" } else { "\x1b[D" }.to_string()),
                                        Key::Home => Some(if is_app_mode { "\x1bOH" } else { "\x1b[H" }.to_string()),
                                        Key::End => Some(if is_app_mode { "\x1bOF" } else { "\x1b[F" }.to_string()),
                                        Key::PageUp => Some("\x1b[5~".to_string()),
                                        Key::PageDown => Some("\x1b[6~".to_string()),
                                        Key::Insert => Some("\x1b[2~".to_string()),
                                        Key::Delete => Some("\x1b[3~".to_string()),
                                        _ if modifiers.ctrl => {
                                            match key {
                                                Key::A => Some("\x01".to_string()), Key::B => Some("\x02".to_string()),
                                                Key::C => Some("\x03".to_string()), Key::D => Some("\x04".to_string()),
                                                Key::E => Some("\x05".to_string()), Key::F => Some("\x06".to_string()),
                                                Key::G => Some("\x07".to_string()), Key::H => Some("\x08".to_string()),
                                                Key::I => Some("\x09".to_string()), Key::J => Some("\x0a".to_string()),
                                                Key::K => Some("\x0b".to_string()), Key::L => Some("\x0c".to_string()),
                                                Key::M => Some("\x0d".to_string()), Key::N => Some("\x0e".to_string()),
                                                Key::O => Some("\x0f".to_string()), Key::P => Some("\x10".to_string()),
                                                Key::Q => Some("\x11".to_string()), Key::R => Some("\x12".to_string()),
                                                Key::S => Some("\x13".to_string()), Key::T => Some("\x14".to_string()),
                                                Key::U => Some("\x15".to_string()), Key::W => Some("\x17".to_string()),
                                                Key::X => Some("\x18".to_string()), Key::Y => Some("\x19".to_string()),
                                                Key::Z => Some("\x1a".to_string()), Key::OpenBracket => Some("\x1b".to_string()),
                                                Key::Backslash => Some("\x1c".to_string()), Key::CloseBracket => Some("\x1d".to_string()),
                                                _ => None,
                                            }
                                        }
                                        _ => None,
                                    };
                                    if let Some(s) = seq { output_to_write.push_str(&s); }
                                }
                                _ => {}
                            }
                        }
                    });
                    if let Some(text) = text_to_copy { ui.ctx().copy_text(text); }
                    if !self.is_composing { self.input_buffer.clear(); }
                    if !output_to_write.is_empty() { let _ = writer.write_all(output_to_write.as_bytes()); }
                }

                // Render visible content
                let state = self.state.lock();
                let history = &state.history;
                let grid = state.grid();
                let start_row = (viewport.min.y / char_size.y).floor() as usize;
                let end_row = (viewport.max.y / char_size.y).ceil() as usize;

                for row_idx in start_row..end_row.min(total_rows) {
                    let cells = if row_idx < history.len() { &history[row_idx] } else { &grid[row_idx - history.len()] };
                    let row_pos = rect.min + Vec2::new(0.0, row_idx as f32 * char_size.y);
                    
                    if let (Some(s), Some(e)) = (self.selection_start, self.selection_end) {
                        let (start, end) = if s <= e { (s, e) } else { (e, s) };
                        if row_idx >= start.0 && row_idx <= end.0 {
                            let c_start = if row_idx == start.0 { start.1 } else { 0 };
                            let c_end = if row_idx == end.0 { (end.1 + 1).min(cols) } else { cols };
                            if c_start < c_end {
                                let sel_rect = Rect::from_min_size(
                                    row_pos + Vec2::new(c_start as f32 * char_size.x, 0.0),
                                    Vec2::new((c_end - c_start) as f32 * char_size.x, char_size.y)
                                );
                                painter.rect_filled(sel_rect, 0.0, Color32::from_rgba_premultiplied(100, 100, 150, 100));
                            }
                        }
                    }

                    // Background and Text rendering
                    let mut c_idx = 0;
                    while c_idx < cells.len().min(cols) {
                        let cell = &cells[c_idx];
                        let mut bg = cell.bg;
                        if cell.inverse { bg = if cell.fg == Color32::TRANSPARENT { TERM_FG } else { cell.fg }; }
                        let start_x = c_idx;
                        c_idx += 1;
                        while c_idx < cells.len().min(cols) {
                            let next = &cells[c_idx];
                            let mut next_bg = next.bg;
                            if next.inverse { next_bg = if next.fg == Color32::TRANSPARENT { TERM_FG } else { next.fg }; }
                            if next_bg != bg { break; }
                            c_idx += 1;
                        }
                        if bg != Color32::TRANSPARENT && bg != TERM_BG {
                            let bg_rect = Rect::from_min_size(row_pos + Vec2::new(start_x as f32 * char_size.x, 0.0), Vec2::new((c_idx - start_x) as f32 * char_size.x, char_size.y));
                            painter.rect_filled(bg_rect, 0.0, bg);
                        }
                    }

                    for (c_idx, cell) in cells.iter().enumerate().take(cols) {
                        if cell.is_wide_continuation || cell.c == ' ' { continue; }
                        let mut fg = cell.fg;
                        if cell.inverse { fg = if cell.bg == Color32::TRANSPARENT { TERM_BG } else { cell.bg }; }
                        if fg == Color32::TRANSPARENT { fg = TERM_FG; }
                        let cell_pos = row_pos + Vec2::new(c_idx as f32 * char_size.x, 0.0);
                        let mut job = LayoutJob::default();
                        job.append(&cell.c.to_string(), 0.0, TextFormat { font_id: font_id.clone(), color: fg, ..Default::default() });
                        painter.galley(cell_pos, ui.fonts_mut(|f| f.layout_job(job)), Color32::TRANSPARENT);
                    }

                    if state.cursor_visible && (row_idx == (history.len() + state.cursor_row)) {
                        let cursor_pos = row_pos + Vec2::new(state.cursor_col as f32 * char_size.x, 0.0);
                        painter.rect_filled(Rect::from_min_size(cursor_pos, char_size), 0.0, Color32::from_gray(200).linear_multiply(0.5));
                    }
                }
            });

        ui.ctx().request_repaint();
    }

    fn box_clone(&self) -> Box<dyn TabInstance> {
        Box::new(self.clone())
    }
}

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn name(&self) -> &str { crate::plugins::PLUGIN_NAME_TERMINAL }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("New Terminal").clicked() {
            if let Ok(tab) = create_terminal_tab(ui.ctx().clone()) {
                control.push(AppCommand::OpenTab(Tab::new(Box::new(tab))));
            }
            ui.close();
        }
    }
}

fn create_terminal_tab(ctx: egui::Context) -> anyhow::Result<TerminalTab> {
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
    let ctx_thread = ctx.clone();

    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        let mut parser = Parser::new();
        while let Ok(n) = reader.read(&mut buffer) {
            if n == 0 { break; }
            {
                let mut s = s_thread.lock();
                let mut handler = LogHandler { state: &mut *s };
                for byte in &buffer[..n] {
                    parser.advance(&mut handler, *byte);
                }
            }
            ctx_thread.request_repaint();
        }
    });

    Ok(TerminalTab {
        state,
        writer: Arc::new(Mutex::new(writer)),
        master: Arc::new(Mutex::new(pair.master)),
        last_size: (80, 24),
        ctx,
        input_buffer: String::new(),
        is_composing: false,
        selection_start: None,
        selection_end: None,
        drag_start: None,
    })
}


pub fn create() -> TerminalPlugin {
    TerminalPlugin
}
