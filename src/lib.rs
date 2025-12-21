use egui::{Ui, WidgetText};

pub mod plugins;
pub mod app;

// ----------------------------------------------------------------------------
// 1. 全局 Tab 定义 (The Registry of Windows)
// ----------------------------------------------------------------------------
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
            Tab::CoreEditor(name) => format!("Editor - {}", name).into(),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Tab::Empty => {
                ui.label("This is an empty tab for testing.");
            }
            Tab::CoreTerminal => {
                ui.heading("Core Terminal");
                ui.code("> echo 'Hello AI'");
            }
            Tab::CoreEditor(name) => {
                ui.heading(format!("Editing: {}", name));
                ui.text_edit_multiline(&mut "Some code goes here...".to_string());
            }
        }
    }
}

// ----------------------------------------------------------------------------
// 2. 插件接口 (The Plugin Interface)
// ----------------------------------------------------------------------------
pub trait Plugin {
    fn name(&self) -> &str;
    fn on_top_panel(&mut self, ui: &mut Ui);
}