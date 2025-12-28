use bevy::prelude::*;
use bevy_egui::EguiPlugin;
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
        .add_systems(Startup, (setup_fonts_system, setup_verbium))
        // 每帧更新逻辑
        .add_systems(Update, (
            update_plugins_system,
            ui_system,
            process_commands_system,
        ).chain()) // 使用 chain 确保顺序执行，减少指令延迟
        .run();
}
