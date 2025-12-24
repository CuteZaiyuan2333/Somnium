use eframe::egui;
use egui_dock::{DockArea, DockState, Style, TabViewer};
use crate::{Tab, Plugin, AppCommand};
use crate::plugins;

// ----------------------------------------------------------------------------
// TabViewer 实现
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

    fn on_close(&mut self, _tab: &mut Self::Tab) -> bool {
        true
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
// Font Setup
// ----------------------------------------------------------------------------

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 尝试加载系统字体以支持中文
    // 优先寻找常见的系统路径
    let mut font_loaded = false;

    #[cfg(target_os = "windows")]
    {
        let windows_fonts = [
            "C:\\Windows\\Fonts\\msyh.ttc",   // 微软雅黑
            "C:\\Windows\\Fonts\\msyh.ttf",
            "C:\\Windows\\Fonts\\simsun.ttc", // 宋体
            "C:\\Windows\\Fonts\\simsun.ttf",
        ];

        for path in windows_fonts {
            if std::path::Path::new(path).exists() {
                if let Ok(font_data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "chinese_font".to_owned(),
                        egui::FontData::from_owned(font_data),
                    );
                    font_loaded = true;
                    break;
                }
            }
        }
    }

    // 如果加载成功，将其设为备选字体
    if font_loaded {
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            vec.push("chinese_font".to_owned());
        }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            vec.push("chinese_font".to_owned());
        }
    }

    // 设置字体
    ctx.set_fonts(fonts);
}

// ----------------------------------------------------------------------------
// Main Application State
// ----------------------------------------------------------------------------
pub struct VerbiumApp {
    dock_state: DockState<Tab>,
    plugins: Vec<Box<dyn Plugin>>,
    command_queue: Vec<AppCommand>,
    show_settings: bool,
}

impl VerbiumApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        let dock_state = DockState::new(Vec::new());
        // 使用自动化注册函数
        let plugins = plugins::all_plugins();

        let app = Self {
            dock_state,
            plugins,
            command_queue: Vec::new(),
            show_settings: false,
        };
        app
    }

    fn process_commands(&mut self) {
        // 使用 while 循环处理，防止指令执行中产生新指令被遗漏
        let mut i = 0;
        while i < self.command_queue.len() {
            let cmd = &self.command_queue[i];
            match cmd {
                AppCommand::OpenTab(tab) => {
                    self.dock_state.main_surface_mut().push_to_focused_leaf(tab.clone());
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
                    self.dock_state = DockState::new(Vec::new());
                }
                AppCommand::CloseTab(title) => {
                    self.dock_state.retain_tabs(|tab| {
                        tab.instance.title().text() != title
                    });
                }
                AppCommand::OpenFile(path) => {
                    for plugin in &mut self.plugins {
                        if let Some(instance) = plugin.try_open_file(path) {
                            self.dock_state.main_surface_mut().push_to_focused_leaf(Tab::new(instance));
                            break;
                        }
                    }
                }
                AppCommand::ToggleSettings => {
                    self.show_settings = !self.show_settings;
                }
            }
            i += 1;
        }
        self.command_queue.clear();
    }
}

impl eframe::App for VerbiumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 插件逻辑更新
        for plugin in &mut self.plugins {
            plugin.update(&mut self.command_queue);
        }

        // 2. 顶部栏渲染
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // 标准 "File" 菜单
                ui.menu_button("File", |ui| {
                    for plugin in &mut self.plugins {
                        plugin.on_file_menu(ui, &mut self.command_queue);
                    }
                });

                // 标准 "Tab" 菜单
                ui.menu_button("Tab", |ui| {
                    for plugin in &mut self.plugins {
                        plugin.on_tab_menu(ui, &mut self.command_queue);
                    }
                });

                // 插件自定义的顶级菜单项
                for plugin in &mut self.plugins {
                    plugin.on_menu_bar(ui, &mut self.command_queue);
                }
            });
        });

        // 3. 全局 UI
        for plugin in &mut self.plugins {
            plugin.on_global_ui(ctx, &mut self.command_queue);
        }

        // Settings Window
        if self.show_settings {
            egui::Window::new("Settings")
                .open(&mut self.show_settings)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for plugin in &mut self.plugins {
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

        // 4. 处理指令
        self.process_commands();

        // 5. 中心 Dock 区域
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut viewer = VerbiumTabViewer {
                command_queue: &mut self.command_queue,
            };
            let style = Style::from_egui(ui.style().as_ref());

            DockArea::new(&mut self.dock_state)
                .style(style)
                .show_window_collapse_buttons(false) // 移除悬浮窗三角形收起按钮
                .show_window_close_buttons(false)
                .show_close_buttons(true)
                .show_inside(ui, &mut viewer);
        });
    }
}
