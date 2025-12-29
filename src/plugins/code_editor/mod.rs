use egui::{Ui, WidgetText};
use crate::{Tab, Plugin, AppCommand, TabInstance};
use std::sync::Arc;
use parking_lot::RwLock;

#[derive(Debug, Clone)]
enum EditorState {
    Loading(Arc<RwLock<Option<Result<String, String>>>>),
    Ready,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct CodeEditorTab {
    pub name: String,
    pub path: Option<std::path::PathBuf>,
    pub code: String,
    pub language: String,
    pub is_dirty: bool,
    pub sync_mode: bool,
    pub last_sync_time: f64,
    state: EditorState,
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
            state: EditorState::Ready,
        }
    }

    fn save(&mut self, control: &mut Vec<AppCommand>) {
        if let EditorState::Ready = self.state {
            if let Some(path) = &self.path {
                match std::fs::write(path, &self.code) {
                    Ok(_) => {
                        self.is_dirty = false;
                        control.push(AppCommand::Notify {
                            message: format!("Saved {}", self.name),
                            level: crate::NotificationLevel::Success,
                        });
                    }
                    Err(e) => {
                        control.push(AppCommand::Notify {
                            message: format!("Save failed: {}", e),
                            level: crate::NotificationLevel::Error,
                        });
                    }
                }
            } else {
                self.save_as(control);
            }
        }
    }

    fn save_as(&mut self, control: &mut Vec<AppCommand>) {
        if let EditorState::Ready = self.state {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&self.name)
                .save_file() 
            {
                match std::fs::write(&path, &self.code) {
                    Ok(_) => {
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

                        control.push(AppCommand::Notify {
                            message: format!("Saved as {}", self.name),
                            level: crate::NotificationLevel::Success,
                        });
                    }
                    Err(e) => {
                        control.push(AppCommand::Notify {
                            message: format!("Save As failed: {}", e),
                            level: crate::NotificationLevel::Error,
                        });
                    }
                }
            }
        }
    }
}

impl TabInstance for CodeEditorTab {
    fn title(&self) -> WidgetText {
        let mut title = match self.state {
            EditorState::Loading(_) => format!("⏳ {}", self.name),
            EditorState::Error(_) => format!("❌ {}", self.name),
            EditorState::Ready => format!("{} {}", if self.is_dirty { "📝" } else { "" }, self.name),
        };
        
        if self.is_dirty {
            title.push('*');
        }
        title.into()
    }

    fn ui(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        // 状态检查
        let loaded_content = if let EditorState::Loading(ref result_store) = self.state {
            let store = result_store.clone();
            let guard = store.read();
            guard.as_ref().cloned()
        } else {
            None
        };

        if let Some(res) = loaded_content {
            match res {
                Ok(content) => {
                    self.code = content;
                    self.state = EditorState::Ready;
                }
                Err(e) => {
                    self.state = EditorState::Error(e);
                }
            }
        }

        if let EditorState::Loading(_) = self.state {
            // 仍在加载
            ui.centered_and_justified(|ui| {
                ui.spinner();
                ui.label("Loading file...");
            });
            ui.ctx().request_repaint(); // 持续刷新以检查状态
            return;
        }

        if let EditorState::Error(ref e) = self.state {
            ui.centered_and_justified(|ui| {
                ui.label(format!("Failed to load file:\n{}", e));
            });
            return;
        }

        // 只有 Ready 状态才执行后续逻辑
        let language = self.language.clone();
        let mut layouter = move |ui: &egui::Ui, string: &dyn egui::TextBuffer, wrap_width: f32| {
            let string = string.as_str();
            let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
            let mut layout_job = egui_extras::syntax_highlighting::highlight(
                ui.ctx(),
                ui.style(),
                &theme,
                string,
                &language,
            );
            layout_job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(layout_job))
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
                self.save(control);
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

    fn on_context_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        // 加载或错误时不显示完整菜单
        if let EditorState::Ready = self.state {
            if ui.add_enabled(!self.sync_mode, egui::Button::new("💾 Save")).clicked() {
                self.save(control);
                ui.close();
            }
            if ui.button("📂 Save As...").clicked() {
                self.save_as(control);
                ui.close();
            }
            ui.separator();
            
            let sync_text = if self.sync_mode { "🔄 Sync Mode: ON" } else { "🔄 Sync Mode: OFF" };
            if ui.checkbox(&mut self.sync_mode, sync_text).clicked() {
                if self.sync_mode {
                    self.last_sync_time = ui.input(|i| i.time);
                }
                ui.close();
            }
        } else {
             ui.label("Please wait for file to load...");
        }
    }

    fn box_clone(&self) -> Box<dyn TabInstance> {
        Box::new(self.clone())
    }
}

pub struct CodeEditorPlugin;

impl Plugin for CodeEditorPlugin {
    fn name(&self) -> &str { crate::plugins::PLUGIN_NAME_CODE_EDITOR }

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
            let path_owned = path.to_path_buf();
            let result_store = Arc::new(RwLock::new(None));
            let result_store_clone = result_store.clone();

            std::thread::spawn(move || {
                let res = std::fs::read_to_string(&path_owned).map_err(|e| e.to_string());
                *result_store_clone.write() = Some(res);
            });

            return Some(Box::new(CodeEditorTab {
                name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                path: Some(path.to_path_buf()),
                code: String::new(),
                language: language.to_string(),
                is_dirty: false,
                sync_mode: false,
                last_sync_time: 0.0,
                state: EditorState::Loading(result_store),
            }));
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
            ui.close();
        }
    }
}

pub fn create() -> CodeEditorPlugin {
    CodeEditorPlugin
}