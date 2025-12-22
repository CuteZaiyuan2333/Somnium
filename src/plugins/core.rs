use egui::Ui;
use crate::{Plugin, AppCommand};

// ----------------------------------------------------------------------------
// Core Plugin
// ----------------------------------------------------------------------------

pub struct CorePlugin {
    show_about: bool,
}

impl Default for CorePlugin {
    fn default() -> Self {
        Self { 
            show_about: false,
        }
    }
}

impl Plugin for CorePlugin {
    fn name(&self) -> &str { "core" }

    // Core 不依赖任何东西
    fn dependencies(&self) -> Vec<String> { Vec::new() }

    fn on_file_menu(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        if ui.button("Quit").clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("Tile All").clicked() {
            control.push(AppCommand::TileAll);
            ui.close_menu();
        }
        if ui.button("Reset Layout").clicked() {
            control.push(AppCommand::ResetLayout);
            ui.close_menu();
        }
    }

    fn on_menu_bar(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        if ui.button("About").clicked() {
            self.show_about = true;
        }
    }

    fn on_global_ui(&mut self, ctx: &egui::Context, _control: &mut Vec<AppCommand>) {
        egui::Window::new("About Verbium")
            .open(&mut self.show_about)
            .show(ctx, |ui| {
                ui.heading("Verbium");
                ui.label("A plugin-based extensible editor framework.");
                ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
            });
    }
}