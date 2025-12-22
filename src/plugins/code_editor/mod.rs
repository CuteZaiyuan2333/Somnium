use egui::{Ui, WidgetText};
use crate::{Tab, Plugin, AppCommand, TabInstance};

#[derive(Debug, Clone)]
pub struct CodeEditorTab {
    pub name: String,
    pub code: String,
    pub language: String,
}

impl TabInstance for CodeEditorTab {
    fn title(&self) -> WidgetText {
        format!(" {}", self.name).into()
    }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Language: {}", self.language));
            });
            ui.separator();

            // 基础编辑器，暂不强制启用高亮以防版本不匹配，
            // 但保留语法高亮槽位。
            let theme = egui::TextEdit::multiline(&mut self.code)
                .font(egui::TextStyle::Monospace)
                .code_editor()
                .lock_focus(true)
                .desired_width(f32::INFINITY);
            
            ui.add_sized(ui.available_size(), theme);
        });
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

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("New Code Editor").clicked() {
            control.push(AppCommand::OpenTab(Tab::new(Box::new(CodeEditorTab {
                name: "script.rs".into(),
                code: "fn main() {\n    println!(\"Hello Verbium!\");\n}".into(),
                language: "rs".into(),
            }))));
            ui.close_menu();
        }
    }
}

pub fn create() -> CodeEditorPlugin {
    CodeEditorPlugin
}
