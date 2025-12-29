# Somnium 系统架构设计

本文档详细描述了 Somnium 编辑器框架的核心架构原则、模块划分及通信机制。

## 1. 核心设计原则

### 1.1 静态插件架构 (Static Plugin Architecture)
与常见的动态链接库 (DLL/so) 或 WASM 插件系统不同，Somnium 选择将插件**静态编译**到最终的可执行文件中。
- **优点**：
    - **性能**：零运行时开销，享受全程序优化 (LTO)。
    - **安全**：利用 Rust 的编译期类型检查，避免 ABI 兼容性问题。
    - **简单**：无需复杂的动态加载器或沙盒环境。
- **缺点**：添加/移除插件需要重新编译（由 Launcher 自动化处理）。

### 1.2 响应式注入 (Reactive Injection)
主程序 (Host) 不持有特定插件的引用，而是定义了一系列**挂载点 (Hooks)**。在渲染每一帧时，主程序会遍历所有已注册的插件，调用其对应的方法，允许插件将自己的 UI “画”到指定位置。基于 **Bevy 0.17** 的 ECS 架构，UI 系统被调度在 `EguiPrimaryContextPass` 中以确保渲染上下文的稳定性。

## 2. 系统模块

### 2.1 核心层 (Core / Kernel)
位于 `src/lib.rs` 和 `src/app.rs`。
- **职责**：
    - 管理 Bevy 引擎生命周期与渲染。
    - 管理 Docking 布局 (`egui_dock`)。
    - 维护插件列表与加载顺序 (`src/plugins/mod.rs` 拓扑排序)。
    - 消息分发 (Command Dispatch)。
- **特点**：基于 ECS 驱动，UI 逻辑与核心指令处理逻辑分离。

### 2.2 插件层 (Plugins)
位于 `src/plugins/`。每个子目录为一个独立插件。
- **Core Plugin**：提供基础功能（退出、布局重置、关于页面）。
- **User Plugins**：实现具体业务（如 Code Editor, File Manager）。

### 2.3 Launcher
位于 `launcher/`。
- **职责**：
    - 扫描插件目录 (`plugin.toml`).
    - 修改根目录 `Cargo.toml` 注入依赖。
    - 生成注册代码。
    - 调用 `cargo` 命令构建项目。

## 3. 通信与交互机制

为了保持插件解耦，Verbium 采用了基于消息队列的中介者模式。

### 3.1 AppCommand 协议
所有状态变更必须通过 `AppCommand` 枚举发送到命令队列，由主程序在帧末统一处理。

```rust
pub enum AppCommand {
    OpenTab(Tab),            // 直接打开一个 Tab 实例
    CloseTab(String),        // 根据标题关闭 Tab
    TileAll,                 // 平铺布局
    ResetLayout,             // 重置布局
    OpenFile(PathBuf),       // 请求打开文件
    RevealInShell(PathBuf),  // 在系统文件管理器中定位
    CopyToClipboard(String), // 写入剪贴板
    Notify { message: String, level: NotificationLevel }, // 全局通知
    ToggleSettings,          // 打开设置面板
}
```

### 3.2 异步 I/O 与反馈模式
为保证 UI 流畅，插件处理耗时操作（如读取大文件）应遵循以下规范：
1. **异步执行**：通过 `std::thread::spawn` 或异步 Runtime 执行 I/O。
2. **状态流转**：UI 层面应实现 `Loading` 占位状态并显示 Spinner。
3. **全局通知**：操作结果（保存成功、删除失败等）必须通过 `AppCommand::Notify` 进行反馈。

## 4. 插件规范
- **元数据绑定**：插件 `name()` 必须引用 `generated.rs` 中自动生成的常量，禁止硬编码。
- **系统隔离**：严禁在插件内直接调用平台特定代码（如 `explorer`），必须通过 `AppCommand` 委托宿主执行。
