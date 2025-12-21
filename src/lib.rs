use egui::{Ui, WidgetText};

pub mod plugins;
pub mod app;

#[derive(Clone, Debug)]
pub enum AppCommand {
    /// 打开一个新的标签页
    OpenTab(Tab),
    /// 强制将所有标签页合并到主窗口
    TileAll,
    /// 重置为初始布局
    ResetLayout,
}

#[derive(Clone, Debug)]
pub enum Tab {
    Empty,
    CoreTerminal,
    CoreEditor(String),
}

impl Tab {
    pub fn title(&self) -> WidgetText {
        match self {
            Tab::Empty => "Empty".into(),
            Tab::CoreTerminal => "Terminal".into(),
            Tab::CoreEditor(name) => format!("📝 {}", name).into(),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Tab::Empty => {
                ui.centered_and_justified(|ui| {
                    ui.label("Verbium Layout Engine\nDrag tabs to split the screen.");
                });
            }
            Tab::CoreTerminal => {
                ui.heading("Terminal");
                ui.monospace("> _");
            }
            Tab::CoreEditor(name) => {
                ui.heading(format!("Editing: {}", name));
                ui.text_edit_multiline(&mut "".to_string());
            }
        }
    }
}

pub trait Plugin {
    fn name(&self) -> &str;
    fn on_top_panel(&mut self, ui: &mut Ui, ctx: &mut Vec<AppCommand>);
}
