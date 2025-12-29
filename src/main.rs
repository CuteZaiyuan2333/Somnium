use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use verbium::app::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Verbium (Bevy)".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        // 初始化
        .add_systems(Startup, (setup_camera, setup_verbium))
        // 核心逻辑更新
        .add_systems(Update, (
            update_plugins_system,
            process_commands_system,
        ).chain())
        // UI 渲染逻辑 (在 EguiPrimaryContextPass 中执行以确保 Context 已就绪)
        .add_systems(EguiPrimaryContextPass, (
            setup_fonts_system,
            ui_system,
        ).chain())
        .run();
}
