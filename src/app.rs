use bevy::prelude::*;
use bevy_egui::EguiContexts;
use egui_dock::{DockArea, DockState, Style, TabViewer};
use crate::{Tab, Plugin, AppCommand, NotificationLevel};
use crate::plugins;

// ----------------------------------------------------------------------------
// Bevy Resources
// ----------------------------------------------------------------------------

#[derive(Resource)]
pub struct PluginRegistry {
    pub instances: Vec<Box<dyn Plugin>>,
}

#[derive(Resource)]
pub struct VerbiumDockState(pub DockState<Tab>);

#[derive(Resource, Default)]
pub struct CommandQueue {
    pub queue: Vec<AppCommand>,
}

#[derive(Resource, Default)]
pub struct NotificationState {
    pub notifications: Vec<NotificationInstance>,
}

pub struct NotificationInstance {
    pub message: String,
    pub level: NotificationLevel,
    pub remaining_time: f32,
}

#[derive(Resource, Default)]
pub struct ShowSettings(pub bool);

// ----------------------------------------------------------------------------
// TabViewer 实现 (保持不变)
// ----------------------------------------------------------------------------

struct VerbiumTabViewer<'a> {
    command_queue: &'a mut Vec<AppCommand>,
}

impl<'a> TabViewer for VerbiumTabViewer<'a> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(tab.id)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.instance.title()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        ui.push_id(tab.id, |ui| {
            tab.instance.ui(ui, self.command_queue);
        });
    }

    fn closeable(&mut self, _tab: &mut Self::Tab) -> bool {
        true
    }

    fn on_close(&mut self, _tab: &mut Self::Tab) -> egui_dock::tab_viewer::OnCloseResponse {
        egui_dock::tab_viewer::OnCloseResponse::Close
    }

    fn context_menu(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut Self::Tab,
        _surface: egui_dock::SurfaceIndex,
        _node: egui_dock::NodeIndex,
    ) {
        tab.instance.on_context_menu(ui, self.command_queue);
    }
}

// ----------------------------------------------------------------------------
// Bevy Systems
// ----------------------------------------------------------------------------

pub fn setup_verbium(mut commands: Commands) {
    let plugins = plugins::all_plugins();
    commands.insert_resource(PluginRegistry { instances: plugins });
    commands.insert_resource(VerbiumDockState(DockState::new(Vec::new())));
    commands.insert_resource(CommandQueue::default());
    commands.insert_resource(NotificationState::default());
    commands.insert_resource(ShowSettings(false));
}

pub fn update_plugins_system(
    mut registry: ResMut<PluginRegistry>,
    mut command_queue: ResMut<CommandQueue>,
) {
    for plugin in &mut registry.instances {
        plugin.update(&mut command_queue.queue);
    }
}

pub fn process_commands_system(
    mut command_queue: ResMut<CommandQueue>,
    mut dock_state: ResMut<VerbiumDockState>,
    mut registry: ResMut<PluginRegistry>,
    mut notification_state: ResMut<NotificationState>,
    mut show_settings: ResMut<ShowSettings>,
    mut contexts: EguiContexts,
) {
    let ctx = contexts.ctx_mut().expect("ctx");
    let mut i = 0;
    while i < command_queue.queue.len() {
        let cmd = &command_queue.queue[i];
        match cmd {
            AppCommand::OpenTab(tab) => {
                dock_state.0.main_surface_mut().push_to_focused_leaf(tab.clone());
            }
            AppCommand::TileAll => {
                let mut all_tabs = Vec::new();
                dock_state.0.retain_tabs(|tab| {
                    all_tabs.push(tab.clone());
                    true
                });
                if !all_tabs.is_empty() {
                    dock_state.0 = DockState::new(all_tabs);
                }
            }
            AppCommand::ResetLayout => {
                dock_state.0 = DockState::new(Vec::new());
            }
            AppCommand::CloseTab(title) => {
                dock_state.0.retain_tabs(|tab| {
                    tab.instance.title().text() != title
                });
            }
            AppCommand::OpenFile(path) => {
                for plugin in &mut registry.instances {
                    if let Some(instance) = plugin.try_open_file(path) {
                        dock_state.0.main_surface_mut().push_to_focused_leaf(Tab::new(instance));
                        break;
                    }
                }
            }
            AppCommand::RevealInShell(path) => {
                #[cfg(target_os = "windows")]
                {
                    use std::process::Command;
                    if path.is_file() {
                        let _ = Command::new("explorer").arg("/select,").arg(path).spawn();
                    } else {
                        let _ = Command::new("explorer").arg(path).spawn();
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    use std::process::Command;
                    let _ = Command::new("open").arg("-R").arg(path).spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    use std::process::Command;
                    let parent = if path.is_file() {
                        path.parent().unwrap_or(path)
                    } else {
                        path
                    };
                    let _ = Command::new("xdg-open").arg(parent).spawn();
                }
            }
            AppCommand::CopyToClipboard(text) => {
                ctx.copy_text(text.clone());
            }
            AppCommand::Notify { message, level } => {
                notification_state.notifications.push(NotificationInstance {
                    message: message.clone(),
                    level: level.clone(),
                    remaining_time: 4.0,
                });
            }
            AppCommand::ToggleSettings => {
                show_settings.0 = !show_settings.0;
            }
        }
        i += 1;
    }
    command_queue.queue.clear();
}

pub fn ui_system(
    mut contexts: EguiContexts,
    mut registry: ResMut<PluginRegistry>,
    mut dock_state: ResMut<VerbiumDockState>,
    mut command_queue: ResMut<CommandQueue>,
    mut notification_state: ResMut<NotificationState>,
    mut show_settings: ResMut<ShowSettings>,
    time: Res<Time>,
) {
    let ctx = contexts.ctx_mut().expect("ctx");
    let dt = time.delta_secs();

    // 0. 更新通知时间
    notification_state.notifications.retain_mut(|n| {
        n.remaining_time -= dt;
        n.remaining_time > 0.0
    });

    // 1. 顶部栏渲染
    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                for plugin in &mut registry.instances {
                    plugin.on_file_menu(ui, &mut command_queue.queue);
                }
            });

            ui.menu_button("Tab", |ui| {
                for plugin in &mut registry.instances {
                    plugin.on_tab_menu(ui, &mut command_queue.queue);
                }
            });

            for plugin in &mut registry.instances {
                plugin.on_menu_bar(ui, &mut command_queue.queue);
            }
        });
    });

    // 2. 全局 UI
    for plugin in &mut registry.instances {
        plugin.on_global_ui(ctx, &mut command_queue.queue);
    }

    // 3. 设置窗口
    if show_settings.0 {
        egui::Window::new("Settings")
            .open(&mut show_settings.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for plugin in &mut registry.instances {
                        let plugin_name = plugin.name().to_string();
                        ui.push_id(&plugin_name, |ui| {
                            ui.collapsing(&plugin_name, |ui| {
                                plugin.on_settings_ui(ui);
                            });
                        });
                    }
                });
            });
    }

    // 4. 中心 Dock 区域
    egui::CentralPanel::default().show(ctx, |ui| {
        let mut viewer = VerbiumTabViewer {
            command_queue: &mut command_queue.queue,
        };
        let style = Style::from_egui(ui.style().as_ref());

        DockArea::new(&mut dock_state.0)
            .style(style)
            .show_leaf_collapse_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_close_buttons(true)
            .show_inside(ui, &mut viewer);
    });

    // 5. 渲染通知
    let mut offset = egui::vec2(-10.0, -10.0);
    for (i, n) in notification_state.notifications.iter().enumerate() {
        let color = match n.level {
            NotificationLevel::Info => egui::Color32::from_rgb(100, 150, 255),
            NotificationLevel::Success => egui::Color32::from_rgb(100, 200, 100),
            NotificationLevel::Warning => egui::Color32::from_rgb(255, 200, 100),
            NotificationLevel::Error => egui::Color32::from_rgb(255, 100, 100),
        };

        let area_id = egui::Id::new("notification").with(i);
        egui::Area::new(area_id)
            .anchor(egui::Align2::RIGHT_BOTTOM, offset)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .fill(egui::Color32::from_rgba_premultiplied(30, 30, 30, 230))
                    .stroke(egui::Stroke::new(1.0, color))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let icon = match n.level {
                                NotificationLevel::Info => "ℹ",
                                NotificationLevel::Success => "✅",
                                NotificationLevel::Warning => "⚠",
                                NotificationLevel::Error => "❌",
                            };
                            ui.label(egui::RichText::new(icon).color(color).strong());
                            ui.label(&n.message);
                        });
                    });
            });
        offset.y -= 45.0;
    }
}

pub fn setup_fonts_system(mut contexts: EguiContexts) {
    let ctx = contexts.ctx_mut().expect("ctx");
    let mut fonts = egui::FontDefinitions::default();
    let mut font_loaded = false;

    #[cfg(target_os = "windows")]
    {
        let windows_fonts = [
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
            "C:\\Windows\\Fonts\\simsun.ttc",
            "C:\\Windows\\Fonts\\simsun.ttf",
        ];

        for path in windows_fonts {
            if std::path::Path::new(path).exists() {
                if let Ok(font_data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "chinese_font".to_owned(),
                        egui::FontData::from_owned(font_data).into(),
                    );
                    font_loaded = true;
                    break;
                }
            }
        }
    }

    if font_loaded {
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            vec.push("chinese_font".to_owned());
        }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            vec.push("chinese_font".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}
