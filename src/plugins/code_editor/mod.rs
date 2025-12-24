use egui::{Ui, WidgetText};
use crate::{Tab, Plugin, AppCommand, TabInstance};

#[derive(Debug, Clone)]
pub struct CodeEditorTab {
    pub name: String,
    pub path: Option<std::path::PathBuf>,
    pub code: String,
    pub language: String,
    pub is_dirty: bool,
    pub sync_mode: bool,
    pub last_sync_time: f64,
}

impl CodeEditorTab {
    fn new(name: String, path: Option<std::path::PathBuf>, code: String, language: String) -> Self {
        Self {
            name,
            path,
            code,
            language,
            is_dirty: false,
            sync_mode: false,
            last_sync_time: 0.0,
        }
    }

    fn save(&mut self) {
        if let Some(path) = &self.path {
            if std::fs::write(path, &self.code).is_ok() {
                self.is_dirty = false;
            }
        } else {
            self.save_as();
        }
    }

    fn save_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&self.name)
            .save_file() 
        {
            if std::fs::write(&path, &self.code).is_ok() {
                self.path = Some(path.clone());
                self.name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                self.is_dirty = false;
                
                // 根据新扩展名更新语言
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                self.language = match ext {
                    "rs" => "rs",
                    "py" => "py",
                    "js" | "ts" => "js",
                    "html" => "html",
                    "css" => "css",
                    "json" => "json",
                    "md" => "md",
                    "toml" => "toml",
                    "c" | "h" => "c",
                    "cpp" | "hpp" | "cc" | "cxx" => "cpp",
                    _ => "txt",
                }.to_string();
            }
        }
    }
}

impl TabInstance for CodeEditorTab {
    fn title(&self) -> WidgetText {
        let mut title = format!("{} {}", if self.is_dirty { "📝" } else { "" }, self.name);
        if self.is_dirty {
            title.push('*');
        }
        title.into()
    }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        let language = self.language.clone();
        let mut layouter = move |ui: &egui::Ui, string: &str, wrap_width: f32| {
            let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
            let mut layout_job = egui_extras::syntax_highlighting::highlight(
                ui.ctx(),
                ui.style(),
                &theme,
                string,
                &language,
            );
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };

        // 处理同步模式逻辑
        if self.sync_mode {
            let current_time = ui.input(|i| i.time);
            if current_time - self.last_sync_time > 1.0 {
                if let Some(path) = &self.path {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if content != self.code {
                            self.code = content;
                            self.is_dirty = false;
                        }
                    }
                }
                self.last_sync_time = current_time;
            }
            // 确保 UI 持续刷新以检查同步
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }

        ui.vertical(|ui| {
            // 快捷键监听: Ctrl + S 保存 (同步模式下禁用)
            if !self.sync_mode && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
                self.save();
            }

            egui::ScrollArea::both()
                .id_salt("code_editor_scroll")
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        // 1. 优化的行号显示
                        let text_style = egui::TextStyle::Monospace;
                        let line_count = self.code.lines().count().max(1);
                        
                        let mut line_numbers_str = String::new();
                        for i in 1..=line_count {
                            line_numbers_str.push_str(&format!("{}\n", i));
                        }

                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(line_numbers_str)
                                    .font(egui::FontId::monospace(12.0))
                                    .color(ui.visuals().weak_text_color())
                            )
                        );

                        ui.separator();

                        // 2. 编辑器主体
                        ui.add_enabled_ui(!self.sync_mode, |ui| {
                            let editor = egui::TextEdit::multiline(&mut self.code)
                                .font(text_style)
                                .code_editor()
                                .lock_focus(true)
                                .desired_width(f32::INFINITY)
                                .layouter(&mut layouter);

                            let response = ui.add_sized(ui.available_size(), editor);
                            if response.changed() {
                                self.is_dirty = true;
                            }
                        });
                    });
                });
        });
    }

    fn on_context_menu(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        if ui.add_enabled(!self.sync_mode, egui::Button::new("💾 Save")).clicked() {
            self.save();
            ui.close_menu();
        }
        if ui.button("📂 Save As...").clicked() {
            self.save_as();
            ui.close_menu();
        }
        ui.separator();
        
        let sync_text = if self.sync_mode { "🔄 Sync Mode: ON" } else { "🔄 Sync Mode: OFF" };
        if ui.checkbox(&mut self.sync_mode, sync_text).clicked() {
            if self.sync_mode {
                self.last_sync_time = ui.input(|i| i.time);
            }
            ui.close_menu();
        }
    }

    fn box_clone(&self) -> Box<dyn TabInstance> {
        Box::new(self.clone())
    }
}

pub struct CodeEditorPlugin;

impl Plugin for CodeEditorPlugin {
    fn name(&self) -> &str { "code_editor" }

    fn dependencies(&self) -> Vec<String> {
        vec!["core".to_string()]
    }

    fn try_open_file(&mut self, path: &std::path::Path) -> Option<Box<dyn TabInstance>> {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        
        // 映射扩展名到语法高亮 ID
        let language = match ext {
            "rs" => "rs",
            "py" => "py",
            "js" | "ts" => "js",
            "html" => "html",
            "css" => "css",
            "json" => "json",
            "md" => "md",
            "toml" => "toml",
            "c" | "h" => "c",
            "cpp" | "hpp" | "cc" | "cxx" => "cpp",
            _ => "txt",
        };

        // 如果是已知文本格式或没有扩展名（可能是 README 等）
        if !language.is_empty() || ext.is_empty() {
             if let Ok(content) = std::fs::read_to_string(path) {
                 return Some(Box::new(CodeEditorTab::new(
                     path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                     Some(path.to_path_buf()),
                     content,
                     language.to_string(),
                 )));
             }
        }
        None
    }

    fn on_settings_ui(&mut self, ui: &mut Ui) {
        ui.label("Editor Settings");
        ui.label("• Ctrl + S to save current file.");
        ui.label("• Syntax highlighting is automatically applied based on extension.");
        ui.label("• Right-click tab for Sync Mode (Read-only follow file).");
    }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("New Code File").clicked() {
            control.push(AppCommand::OpenTab(Tab::new(Box::new(CodeEditorTab::new(
                "untitled".into(),
                None,
                String::new(),
                "rs".into(),
            )))));
            ui.close_menu();
        }
    }
}

pub fn create() -> CodeEditorPlugin {
    CodeEditorPlugin
}
