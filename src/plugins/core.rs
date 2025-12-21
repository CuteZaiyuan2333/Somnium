use egui::Ui;
use crate::{Tab, Plugin, AppCommand};

pub struct CorePlugin {
    new_file_counter: usize,
}

impl Default for CorePlugin {
    fn default() -> Self {
        Self { new_file_counter: 1 }
    }
}

impl Plugin for CorePlugin {
    fn name(&self) -> &str {
        "core"
    }

    fn on_top_panel(&mut self, ui: &mut Ui, ctx: &mut Vec<AppCommand>) {
        ui.menu_button("File", |ui| {
            if ui.button("New Editor").clicked() {
                let name = format!("Untitled-{}", self.new_file_counter);
                self.new_file_counter += 1;
                ctx.push(AppCommand::OpenTab(Tab::CoreEditor(name)));
                ui.close_menu();
            }
            if ui.button("New Terminal").clicked() {
                ctx.push(AppCommand::OpenTab(Tab::CoreTerminal));
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                ui.close_menu();
            }
        });

        ui.menu_button("Window", |ui| {
            if ui.button("Tile All Tabs").clicked() {
                ctx.push(AppCommand::TileAll);
                ui.close_menu();
            }
            if ui.button("Reset Layout").clicked() {
                ctx.push(AppCommand::ResetLayout);
                ui.close_menu();
            }
        });

        ui.menu_button("About", |ui| {
            ui.label("Verbium Core Plugin");
            ui.label("MIT License");
        });
    }
}
