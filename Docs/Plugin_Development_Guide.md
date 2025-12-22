# Verbium 插件开发指南 (v0.1.0)

欢迎加入 Verbium 生态系统！Verbium 是一个旨在提供极致扩展性的编辑器框架。本指南将帮助您理解 Verbium 的架构，并指导您如何编写、集成和分发自己的插件。

---

## 1. 核心架构：响应式注入

Verbium 采用 **响应式注入 (Reactive Injection)** 架构。插件不直接修改软件的逻辑或持有 UI 组件，而是通过以下两个机制与主程序交互：

1.  **生命周期钩子 (Hooks)**：主程序在渲染的不同阶段（菜单栏、全局区域、逻辑更新）主动询问插件：“你有什么想显示或执行的吗？”
2.  **命令通信 (Commanding)**：插件通过一个异步队列发送指令（`AppCommand`），请求主程序执行诸如“打开新标签页”或“关闭软件”等关键操作。

---

## 2. 快速开始：创建一个新插件

### 第一步：创建插件目录
所有插件必须放在 `src/plugins/` 目录下。假设我们要创建一个名为 `hello_world` 的插件：

```bash
mkdir src/plugins/hello_world
```

### 第二步：编写插件入口
在 `src/plugins/hello_world/mod.rs` 中编写以下代码：

```rust
use egui::{Ui, WidgetText};
use crate::{Plugin, AppCommand};

pub struct HelloWorldPlugin;

impl Plugin for HelloWorldPlugin {
    fn name(&self) -> &str { "hello_world" }

    // 注入到 File 菜单
    fn on_file_menu(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        if ui.button("Hello World Info").clicked() {
            println!("Hello from Plugin!");
        }
    }
}

// 必须导出此函数，供系统自动注册使用
pub fn create() -> HelloWorldPlugin {
    HelloWorldPlugin
}
```

### 第三步：声明依赖 (可选)
如果你的插件需要外部 crate，请在 `src/plugins/hello_world/plugin.toml` 中声明：
```toml
[plugin]
name = "hello_world"
version = "0.1.0"

[external_dependencies]
serde = "1.0"
# 推荐使用这种 Inline Table 格式以确保构建兼容性
egui_extras = { version = "0.29.1", features = ["syntect"] }
```

### 第四步：编译运行
无需任何手动注册。确保 Launcher 中已勾选该插件，然后运行：
```bash
cargo run
```
`build.rs` 会自动检测到新文件夹并将其编译进程序。

---

## 3. 进阶：自定义标签页 (Tabs)

如果你的插件需要显示一个完整的工作界面，你需要实现 `TabInstance` Trait。

### 定义 Tab
```rust
#[derive(Debug, Clone)]
pub struct MyCustomTab {
    pub content: String,
}

impl crate::TabInstance for MyCustomTab {
    fn title(&self) -> egui::WidgetText { "My Custom Tab".into() }

    fn ui(&mut self, ui: &mut egui::Ui, control: &mut Vec<crate::AppCommand>) {
        ui.heading("Welcome to my tab!");
        ui.text_edit_multiline(&mut self.content);
        
        if ui.button("Close this tab").clicked() {
            control.push(crate::AppCommand::CloseTab(self.title().text()));
        }
    }

    fn box_clone(&self) -> Box<dyn crate::TabInstance> {
        Box::new(self.clone())
    }
}
```

### 从插件打开 Tab
```rust
impl Plugin for HelloWorldPlugin {
    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("Open My Tab").clicked() {
            let new_tab = MyCustomTab { content: String::new() };
            control.push(AppCommand::OpenTab(crate::Tab(Box::new(new_tab))));
        }
    }
}
```

---

## 4. 插件依赖管理 (Dependencies)

如果你的插件依赖于另一个插件定义的菜单项排序、公共组件或逻辑顺序，你可以声明依赖关系。

### 声明依赖
在 `Plugin` 实现中重写 `dependencies` 方法：

```rust
impl Plugin for MyAdvancedPlugin {
    fn name(&self) -> &str { "advanced_plugin" }

    fn dependencies(&self) -> Vec<String> {
        // 确保 "core" 插件在本项目之前加载
        vec!["core".to_string()]
    }
}
```

### 依赖的作用
1.  **加载顺序**：Verbium 会对所有插件进行拓扑排序。被依赖的插件将优先调用 `update` 和各种 `on_menu` 钩子。
2.  **UI 注入顺序**：在同一个菜单（如 `File`）中，先加载的插件项会排在上方，后加载的（依赖者）会排在下方。
3.  **安全性**：如果检测到循环依赖，Verbium 会在控制台发出警告。

---

## 5. API 深度参考

### 4.1 `Plugin` 生命周期方法

| 方法 | 触发时机 | 用途 |
| :--- | :--- | :--- |
| `on_file_menu` | 用户点击 "File" 菜单时 | 注入文件操作相关的按钮。 |
| `on_tab_menu` | 用户点击 "Tab" 菜单时 | 注入新建、管理标签页的按钮。 |
| `on_menu_bar` | 渲染主菜单栏时 | 注册新的顶级菜单（如 `Help`, `Tools`）。 |
| `on_global_ui` | 每帧渲染时（背景） | 渲染弹窗 (`egui::Window`) 或全局悬浮通知。 |
| `update` | 每帧渲染前 | 处理后台逻辑、计算、或是定时任务。 |

### 4.2 `AppCommand` 命令列表

| 命令 | 描述 |
| :--- | :--- |
| `OpenTab(Tab)` | 在焦点处打开一个新标签页。 |
| `CloseTab(String)` | 根据标题名称关闭特定的标签页。 |
| `TileAll` | 将所有标签页强制平铺到当前主窗口。 |
| `ResetLayout` | 清空所有布局信息，恢复到初始状态。 |

---

## 5. 常见问题 (FAQ)

#### Q: 为什么我的插件没有加载？
**A:** 确保文件夹中包含 `mod.rs` 且导出了正确的 `create()` 函数。另外请检查文件夹名称是否符合 Rust 的模块命名规范（小写、下划线）。

#### Q: 插件可以互相通信吗？
**A:** 目前推荐通过 `AppCommand` 进行间接通信。未来将引入全局事件总线 (Event Bus) 供插件订阅。

#### Q: 插件如何处理持久化数据？
**A:** 目前插件可以自行在 `update` 逻辑中读写本地文件。我们计划在 `Plugin` 接口中增加 `on_save/on_load` 钩子以支持统一的配置管理。

---

## 6. 开发最佳实践

1.  **UI 友善性**：尽量不要在 `on_menu_bar` 中添加过多的顶级按钮，优先考虑注入到已有的 `File` 或 `Tab` 菜单。
2.  **避免阻塞**：`update` 方法在主线程调用，请勿在此处执行耗时的 I/O 操作（建议使用 `std::thread` 或 `tokio`）。
3.  **错误处理**：插件内部崩溃可能会导致整个应用退出，请务必处理好所有的 `Result`。

---
*文档版本: 0.1.0*
*Verbium 开发团队 2025*
