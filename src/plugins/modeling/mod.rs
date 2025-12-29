use egui::{Ui, WidgetText, UiKind};
use crate::{Plugin, TabInstance, AppCommand, Tab};

#[derive(Default)]
pub struct ModelingPlugin;

pub fn create() -> ModelingPlugin {
    ModelingPlugin::default()
}

impl Plugin for ModelingPlugin {
    fn name(&self) -> &str {
        "modeling"
    }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("SDF Base Model").clicked() {
            let tab = Tab::new(Box::new(ModelingTab::default()));
            control.push(AppCommand::OpenTab(tab));
            ui.close_kind(UiKind::Menu);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelingTab;

impl TabInstance for ModelingTab {
    fn title(&self) -> WidgetText {
        "SDF Modeler".into()
    }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        ui.centered_and_justified(|ui| {
            ui.heading("[Empty]");
        });
    }

    fn box_clone(&self) -> Box<dyn TabInstance> {
        Box::new(self.clone())
    }
}
