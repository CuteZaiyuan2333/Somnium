# Somnium 插件开发指南

本指南将帮助你编写 Somnium 插件。插件是 Somnium 的一等公民，可以控制从菜单栏到编辑器面板的几乎所有内容。

## 1. 插件结构

在 `src/plugins/` 下创建一个新文件夹（例如 `my_tool`），并包含以下文件：

### 1.1 `mod.rs` (逻辑实现)
必须包含一个实现了 `Plugin` trait 的结构体，以及一个导出的 `create()` 函数。

**关键规范**：
- `name()` 方法**严禁硬编码**字符串，必须引用 `crate::plugins::PLUGIN_NAME_...` 常量（由 build.rs 自动生成）。
- **egui 0.33 适配**：菜单关闭请使用 `ui.close()` 而非旧版的 `ui.close_menu()`。
- **ID 盐值**：`ScrollArea` 等组件请使用 `id_salt` 替代旧版的 `id_source`。

```rust
use egui::Ui;
use crate::{Plugin, AppCommand};

pub struct MyToolPlugin;

impl Plugin for MyToolPlugin {
    // 引用自动生成的常量，保持与 plugin.toml 同步
    fn name(&self) -> &str { crate::plugins::PLUGIN_NAME_MY_TOOL }
    
    // 实现钩子方法...
}

pub fn create() -> MyToolPlugin {
    MyToolPlugin
}
```

### 1.2 `plugin.toml` (元数据与依赖)
必须包含。用于定义插件 ID 和外部 crate 依赖。

```toml
[plugin]
name = "my_tool"
version = "0.1.0"
description = "A sample tool"

[external_dependencies]
serde = { version = "1.0", features = ["derive"] }
```

---

## 2. API 参考 (Trait Hooks)

### UI 注入类
| 方法 | 描述 |
| :--- | :--- |
| `on_file_menu` | 注入内容到顶部 "File" 菜单。 |
| `on_tab_menu` | 注入内容到顶部 "Tab" 菜单。 |
| `on_menu_bar` | 在菜单栏添加自定义的顶级菜单（如 "Tools", "Help"）。 |
| `on_global_ui` | 绘制全局覆盖层（如弹窗）。注：Toast 通知请使用 `Notify` 指令。 |
| `on_settings_ui` | 绘制插件的配置选项到全局设置窗口中。 |

---

## 3. 开发规范与最佳实践

### 3.1 异步 I/O (Async I/O)
严禁在 `ui()` 或 `try_open_file()` 中执行同步阻塞操作。
- **模式**：启动线程处理 I/O，并让 Tab 处于 `Loading` 状态。
- **反馈**：使用 `control.push(AppCommand::Notify { ... })` 告知用户操作结果。

### 3.2 操作系统抽象 (OS Abstraction)
严禁直接使用 `std::process::Command` 或平台特定的 Shell 命令。
- **文件管理**：使用 `AppCommand::RevealInShell(path)`。
- **剪贴板**：使用 `AppCommand::CopyToClipboard(text)` 或 `ui.ctx().copy_text(text)`。

### 3.3 通知系统 (Notification)
不要在插件内自己写弹窗逻辑，除非是复杂的交互界面。对于简单的结果反馈，使用全局通知：
```rust
control.push(AppCommand::Notify {
    message: "文件已成功保存".into(),
    level: crate::NotificationLevel::Success,
});
```
---