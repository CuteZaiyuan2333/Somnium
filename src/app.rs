use eframe::egui;
use egui_dock::{DockArea, DockState, Style, TabViewer};
use crate::{Tab, Plugin};
use crate::plugins::core::CorePlugin;

// ----------------------------------------------------------------------------
// TabViewer 实现 (Adapter for egui_dock)
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
}

// ----------------------------------------------------------------------------
// Main Application State
// ----------------------------------------------------------------------------
pub struct VerbiumApp {
    dock_state: DockState<Tab>,
    plugins: Vec<Box<dyn Plugin>>,
}

impl VerbiumApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // 1. 初始化 Dock 系统，默认显示一个 Empty 窗口
        let dock_state = DockState::new(vec![Tab::Empty]);

        // 2. 加载插件 (这里是手动硬编码，模拟“重编译加载”)
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(CorePlugin::default()),
        ];

        Self {
            dock_state,
            plugins,
        }
    }
}

impl eframe::App for VerbiumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --------------------------------------------------------------------
        // 1. Top Panel (Menu Bar)
        // --------------------------------------------------------------------
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // 让每个插件在顶部菜单栏绘制
                for plugin in &mut self.plugins {
                    plugin.on_top_panel(ui);
                }
            });
        });

        // --------------------------------------------------------------------
        // 2. Bottom Panel (Status Bar)
        // --------------------------------------------------------------------
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Verbium test version");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("MIT");
                });
            });
        });

        // --------------------------------------------------------------------
        // 3. Central Panel (Dock Area)
        // --------------------------------------------------------------------
        egui::CentralPanel::default().show(ctx, |ui| {
            // 我们需要创建一个 TabViewer 实例来渲染 Dock
            let mut viewer = VerbiumTabViewer;
            DockArea::new(&mut self.dock_state)
                .style(Style::from_egui(ui.style().as_ref()))
                .show_inside(ui, &mut viewer);
        });
    }
}
