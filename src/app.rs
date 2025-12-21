use eframe::egui;
use egui_dock::{DockArea, DockState, Style, TabViewer};
use crate::{Tab, Plugin, AppCommand};
use crate::plugins::core::CorePlugin;

// ----------------------------------------------------------------------------
// TabViewer 实现
// ----------------------------------------------------------------------------
struct VerbiumTabViewer;

impl TabViewer for VerbiumTabViewer {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        tab.ui(ui);
    }

    // 重新启用关闭按钮
    fn closeable(&mut self, _tab: &mut Self::Tab) -> bool {
        true
    }

    fn on_close(&mut self, _tab: &mut Self::Tab) -> bool {
        true // 确认关闭
    }
}

// ----------------------------------------------------------------------------
// Main Application State
// ----------------------------------------------------------------------------
pub struct VerbiumApp {
    dock_state: DockState<Tab>,
    plugins: Vec<Box<dyn Plugin>>,
    command_queue: Vec<AppCommand>,
}

impl VerbiumApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let dock_state = DockState::new(vec![Tab::Empty]);
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(CorePlugin::default()),
        ];

        Self {
            dock_state,
            plugins,
            command_queue: Vec::new(),
        }
    }

    fn process_commands(&mut self) {
        let commands: Vec<AppCommand> = self.command_queue.drain(..).collect();
        for cmd in commands {
            match cmd {
                AppCommand::OpenTab(tab) => {
                    self.dock_state.main_surface_mut().push_to_focused_leaf(tab);
                }
                AppCommand::TileAll => {
                    let mut all_tabs = Vec::new();
                    self.dock_state.retain_tabs(|tab| {
                        all_tabs.push(tab.clone());
                        true
                    });
                    if !all_tabs.is_empty() {
                        self.dock_state = DockState::new(all_tabs);
                    }
                }
                AppCommand::ResetLayout => {
                    self.dock_state = DockState::new(vec![Tab::Empty]);
                }
            }
        }
    }
}

impl eframe::App for VerbiumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 顶部栏
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                for plugin in &mut self.plugins {
                    plugin.on_top_panel(ui, &mut self.command_queue);
                }
            });
        });

        // 2. 处理指令
        self.process_commands();

        // 3. 底部栏
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Verbium test version");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("MIT");
                });
            });
        });

        // 4. 中心 Dock 区域
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut viewer = VerbiumTabViewer;
            let style = Style::from_egui(ui.style().as_ref());

            DockArea::new(&mut self.dock_state)
                .style(style)
                .show_window_collapse_buttons(false)
                .show_window_close_buttons(false) // 禁用悬浮窗容器的 X 按钮
                .show_close_buttons(true)        // 保留标签页本身的 X 按钮
                .show_inside(ui, &mut viewer);
        });
    }
}
