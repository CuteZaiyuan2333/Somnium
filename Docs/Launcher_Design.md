# Somnium Launcher 设计规范

Launcher 是 Somnium 生态系统的重要组成部分，负责管理静态插件的编译配置。

## 1. 核心职责

由于 Rust 不支持运行时反射加载源码，Somnium 采用 **Launcher 辅助构建** 与 **Build Script 自动代码生成** 的双重策略：
1.  **扫描**：遍历 `src/plugins/` 发现可用插件。
2.  **配置**：主程序的 `build.rs` 会自动读取 `plugin.toml` 并生成 `src/plugins/generated.rs`，包含插件加载逻辑、名称常量及一致性校验。
3.  **注入**：Launcher 修改根目录 `Cargo.toml` 注入插件所需的外部依赖。
4.  **构建**：调用 `cargo build` 或 `cargo run` 启动编辑器。

## 2. 插件发现协议

Launcher 通过检测 `src/plugins/*/plugin.toml` 来识别插件。

### plugin.toml 格式
```toml
[plugin]
name = "my_plugin"       # 唯一标识符（对应 Rust 模块名）
display_name = "我的插件"
version = "0.1.0"
description = "插件描述"
dependencies = ["core"]  # 内部插件依赖顺序

[external_dependencies]
# 将会被注入到根 Cargo.toml 的 [dependencies] 中
serde = "1.0"
rand = { version = "0.8", features = ["small_rng"] }
```

## 3. 依赖注入逻辑

为了避免用户手动修改 `Cargo.toml` 导致冲突，Launcher 拥有 `Cargo.toml` 的部分管理权。

Launcher 会在 `Cargo.toml` 中寻找如下标记：
```toml
# --- BEGIN PLUGIN DEPENDENCIES ---
# ... Launcher 自动生成的内容 ...
# --- END PLUGIN DEPENDENCIES ---
```

## 4. 元数据共享逻辑

核心代码通过 `build.rs` 提取 `plugin.toml` 中的 `name` 字段，并生成类似 `PLUGIN_NAME_MY_PLUGIN` 的常量。
- **好处**：Launcher 修改配置与插件 `name()` 方法实现完全解耦，通过常量在编译期强制对齐。
- **校验**：生成的 `get_extra_plugins` 函数会在实例化时通过 `assert_eq!` 验证插件名是否符合配置，防止拓扑排序崩溃。

## 5. UI 交互流

1.  **启动**：Launcher 读取当前配置。
2.  **列表**：左侧显示所有扫描到的插件，复选框表示是否启用（即是否加入 `default` features）。
3.  **同步**：用户点击 "Sync & Run" 后：
    - 更新 `Cargo.toml`。
    - 触发 `cargo run`（其 build.rs 会更新 `generated.rs`）。
    - 将 stdout/stderr 输出流重定向到 Launcher 的日志控制台。