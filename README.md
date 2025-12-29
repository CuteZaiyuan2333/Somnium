# Somnium

Somnium 是一款基于 **Bevy Engine 0.17** 与 **egui 0.33** 构建的高性能、模块化应用开发框架及代码编辑器。作为 Verbium 的演进版本，Somnium 继承并强化了其核心的**静态插件架构**，并利用 Bevy 的 ECS 动力实现了更高的渲染效率与扩展性。

## 🚀 快速开始

Somnium 采用全自动化的构建流程。克隆项目后，直接运行：

```bash
cargo run --release
```

**默认行为**：
*   **初次启动**：仅启用 `plugin_manager`（即 Somnium Launcher 界面）。
*   **定制构建**：在启动器界面中选择项目路径，勾选需要的功能插件（如 Terminal, Code Editor, File Manager 等）。
*   **一键编译**：点击 **🔨 Build** 或 **▶ Run**，Somnium 将自动同步依赖并基于 Bevy 重新编译出完整功能的编辑器环境。

## 🏗 核心架构 (The Somnium Way)

Somnium 不仅仅是一个编辑器，它是一个基于数据驱动（ECS）的系统。

1.  **基于 Bevy 的底层**：利用 Bevy Engine 的 `wgpu` 渲染管线与强大的任务调度器，Somnium 可以轻松集成 3D 渲染、复杂动画或大规模并行数据处理。
2.  **静态插件架构 (Static Plugin Architecture)**：
    *   插件在编译期直接打入二进制文件。
    *   **性能**：享受 Rust 全程序优化 (LTO) 和零运行时抽象开销。
    *   **安全**：利用 Rust 编译期检查确保插件间的 ABI 兼容性。
3.  **自举设计 (Self-Bootstrapping)**：Somnium 的启动器逻辑本身就是一个 Somnium 插件，实现了“用 Somnium 开发 Somnium”的闭环。

## 🛠 功能特性

*   **集成启动器**：可视化管理项目、开关插件、自动同步 `Cargo.toml` 依赖。
*   **高性能渲染**：由 `bevy_egui` 驱动，支持复杂布局与流畅的 UI 交互。
*   **多标签页管理**：集成 `egui_dock`，支持灵活的窗口拆分与拖拽停靠。
*   **导出工具**：支持将编译好的特定版本（定制化功能集）导出为独立的生产环境。
*   **静态编译优势**：单执行文件部署，无需携带复杂的动态链接库。

## 🧩 插件生态

Somnium 保持了与 Verbium 插件协议的完全兼容。目前内置/可选插件包括：
*   **Code Editor**: 支持语法高亮、自动同步模式。
*   **File Manager**: 支持拖拽移动、重命名、文件操作。
*   **Terminal**: 内置集成终端。
*   **Plugin Manager**: 核心自举插件。

## 📜 许可证

本项目采用 [MIT License](LICENSE) 开源。
