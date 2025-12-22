# Verbium 插件分发与 Launcher 集成规范

## 1. 插件打包格式 (.verbium)

`.verbium` 文件本质上是一个标准的 **ZIP 压缩包**。其内部结构必须如下：

```text
my_plugin.verbium (ZIP)
├── plugin.toml          # 必须：插件元数据与依赖声明
├── mod.rs               # 必须：插件入口代码
└── ...                  # 其他可选的源码文件
```

## 2. 插件描述文件 (plugin.toml) 规范

Launcher 将解析此文件来决定如何修改主程序的配置。

```toml
[plugin]
name = "my_plugin"       # 唯一 ID，建议使用小写下划线（将对应 Cargo Feature）
display_name = "我的插件"
version = "1.0.0"
author = "Author Name"
description = "描述信息"
dependencies = ["core"]  # 依赖的其他插件 ID

[external_dependencies]
# 外部 Rust 库依赖。Launcher 会将其合并到主程序的 [dependencies] 中
# 强烈建议使用 Inline Table 格式（如下所示）以确保 TOML 作用域安全
serde = "1.0"
egui_extras = { version = "0.29.1", features = ["syntax_highlighting"] }
```

## 3. Launcher 的操作流程建议

当用户点击“同步/构建”时，Launcher 应执行：

1.  **解析清单**：使用 `toml_edit` 解析根目录 `Cargo.toml`。
2.  **清理并注入依赖**：定位 `BEGIN/END PLUGIN` 标记，将已启用插件的 `external_dependencies` 以 Inline Table 格式注入。
3.  **管理特性**：动态更新 `[features]` 表，为每个插件创建 `plugin_ID` 项，并同步更新 `default` 特性列表。
4.  **执行构建**：保存文件并执行 `cargo build` 或 `cargo run`。

## 4. 主程序条件编译逻辑

主程序通过 `build.rs` 自动生成带有特性门控的代码：
- 如果启用了 `plugin_my_plugin` 特性，则 `mod my_plugin` 会被编译。
- 如果未启用，该目录下的代码将完全被编译器忽略，不会产生任何开销或错误。
