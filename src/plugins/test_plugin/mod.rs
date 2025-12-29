use egui::{Ui, WidgetText};
use crate::{Tab, Plugin, AppCommand, TabInstance};

#[derive(Debug, Clone)]
pub struct TestTab {
    text: String,
}

impl TabInstance for TestTab {
    fn title(&self) -> WidgetText { "TESTTAB".into() }
    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        ui.label("This is a test tab from Test Plugin.");
        ui.text_edit_multiline(&mut self.text);
    }
    fn box_clone(&self) -> Box<dyn TabInstance> { Box::new(self.clone()) }
}

pub struct TestPlugin;

impl Plugin for TestPlugin {
    fn name(&self) -> &str { crate::plugins::PLUGIN_NAME_TEST_PLUGIN }

    fn dependencies(&self) -> Vec<String> {
        vec!["core".to_string()]
    }

    fn on_file_menu(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        if ui.button("TEST (Plugin Item)").clicked() {
            println!("Test plugin menu item clicked!");
        }
    }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("New TESTTAB").clicked() {
            control.push(AppCommand::OpenTab(Tab::new(Box::new(TestTab {
                text: "Hello from test plugin!".into(),
            }))));
            ui.close();
        }
    }
}

pub fn create() -> TestPlugin {
    TestPlugin
}
