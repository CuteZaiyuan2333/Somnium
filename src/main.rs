#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Hide console on Windows release builds

use eframe::egui;
use verbium::app::VerbiumApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("Verbium"),
        ..Default::default()
    };

    eframe::run_native(
        "Verbium",
        native_options,
        Box::new(|cc| Ok(Box::new(VerbiumApp::new(cc)))),
    )
}