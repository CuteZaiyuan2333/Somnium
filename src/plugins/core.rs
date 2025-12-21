use egui::Ui;
use crate::Tab;
use crate::Plugin;

pub struct CorePlugin;

impl Default for CorePlugin {
    fn default() -> Self {
        Self
    }
}

impl Plugin for CorePlugin {
    fn name(&self) -> &str {
        "core"
    }

    fn on_top_panel(&mut self, ui: &mut Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("New").clicked() {
                // Future: Send command to open Tab::CoreEditor
                ui.close_menu();
            }
            if ui.button("Open").clicked() {
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                // In a real app, we would send a shutdown signal or use ui.ctx().send_viewport_cmd(...)
                ui.close_menu();
            }
        });

        ui.menu_button("Edit", |ui| {
            if ui.button("Cut").clicked() { ui.close_menu(); }
            if ui.button("Copy").clicked() { ui.close_menu(); }
            if ui.button("Paste").clicked() { ui.close_menu(); }
        });

        ui.menu_button("About", |ui| {
            ui.label("Verbium Core Plugin");
            ui.label("Version 0.1.0");
        });
    }
}
