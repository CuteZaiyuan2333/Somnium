use egui::{Ui, WidgetText, Context};
use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};

pub mod plugins;
pub mod app;

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

// ----------------------------------------------------------------------------
// Tab 抽象
// ----------------------------------------------------------------------------

/// 插件必须实现这个 Trait 来定义自己的标签页内容
pub trait TabInstance: Debug + Send + Sync {
    fn title(&self) -> WidgetText;
    fn ui(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>);
    /// 标签页右键菜单钩子
    fn on_context_menu(&mut self, _ui: &mut Ui, _control: &mut Vec<AppCommand>) {}
    /// 用于克隆 Trait 对象
    fn box_clone(&self) -> Box<dyn TabInstance>;
}

/// 包装器，用于在 egui_dock 中持有动态生成的 Tab
pub struct Tab {
    pub instance: Box<dyn TabInstance>,
    pub id: u64,
}

impl Tab {
    pub fn new(instance: Box<dyn TabInstance>) -> Self {
        Self {
            instance,
            id: NEXT_TAB_ID.fetch_add(1, Ordering::SeqCst),
        }
    }
}

impl Clone for Tab {
    fn clone(&self) -> Self {
        Tab {
            instance: self.instance.box_clone(),
            id: self.id,
        }
    }
}

impl Debug for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tab")
            .field("id", &self.id)
            .field("title", &self.instance.title().text())
            .finish()
    }
}

// ----------------------------------------------------------------------------
// 命令系统
// ----------------------------------------------------------------------------

pub enum AppCommand {
    /// 打开一个新的标签页
    OpenTab(Tab),
    /// 强制将所有标签页合并到主窗口
    TileAll,
    /// 重置为初始布局
    ResetLayout,
    /// 关闭指定标题的标签页（简单示例）
    CloseTab(String),
    /// 请求打开指定路径的文件
    OpenFile(std::path::PathBuf),
    /// 切换设置窗口
    ToggleSettings,
}

// ----------------------------------------------------------------------------
// 插件接口
// ----------------------------------------------------------------------------

pub trait Plugin {
    /// 插件唯一标识名
    fn name(&self) -> &str;

    /// 声明依赖项：返回此插件依赖的其它插件名称列表
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }

    /// 尝试打开文件
    /// 如果插件支持该文件类型，返回一个新的 Tab 实例；否则返回 None
    fn try_open_file(&mut self, _path: &std::path::Path) -> Option<Box<dyn TabInstance>> {
        None
    }

    /// 注入到设置窗口的 UI
    fn on_settings_ui(&mut self, _ui: &mut Ui) {}
    
    /// 注入到 "File" 菜单的内容
    fn on_file_menu(&mut self, _ui: &mut Ui, _control: &mut Vec<AppCommand>) {}

    /// 注入到 "Tab" 菜单的内容
    fn on_tab_menu(&mut self, _ui: &mut Ui, _control: &mut Vec<AppCommand>) {}

    /// 在菜单栏注册自定义的顶级菜单或直接放置按钮
    fn on_menu_bar(&mut self, _ui: &mut Ui, _control: &mut Vec<AppCommand>) {}
    
    /// 渲染全局 UI (例如弹窗 Window)
    fn on_global_ui(&mut self, _ctx: &Context, _control: &mut Vec<AppCommand>) {}

    /// 每帧逻辑更新
    fn update(&mut self, _control: &mut Vec<AppCommand>) {}
}