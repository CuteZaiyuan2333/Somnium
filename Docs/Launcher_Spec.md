# Verbium Launcher Specification

## 1. Goal
Provide a user-friendly interface to manage plugins and build the Verbium editor without manual editing of configuration files.

## 2. Directory Structure (Launcher Project)
The launcher will reside in a subdirectory (e.g., `verbium_launcher/`) or as a separate workspace member.

## 3. Data Schema

### 3.1 launcher_config.toml
Stored in the project root.
```toml
[plugins]
code_editor = true
test_plugin = false
```

### 3.2 .verbium Format (Reminder)
- `plugin.toml`: Metadata and external dependencies.
- `mod.rs`: Plugin logic.

## 4. Operational Logic

### 4.1 Plugin Discovery
1. Scan `src/plugins/`.
2. For each directory, check for `plugin.toml`.
3. Extract `display_name`, `version`, `author`, `description`.

### 4.2 Syncing Cargo.toml
The launcher modifies `Cargo.toml` within designated markers:
```toml
# --- BEGIN PLUGIN DEPENDENCIES ---
# Managed by Verbium Launcher
serde = "1.0"
# --- END PLUGIN DEPENDENCIES ---
```
Logic:
1. Iterate through enabled plugins.
2. Collect all `external_dependencies` from their `plugin.toml`.
3. **Crucial**: Convert all dependencies to **Inline Table** format (e.g., `crate = { version = "..." }`) before injection. This prevents one plugin's dependency table from "swallowing" subsequent dependencies (TOML Scope Leakage).
4. Use `toml_edit` to parse the manifest and replace the content between markers.
5. Ensure `default` features in `Cargo.toml` match the enabled plugins list.

### 4.3 Building and Running
1. Capture `cargo` output using piped `stdout`.
2. Display output in the Launcher UI.
3. Handle errors by highlighting the console output.

## 5. UI Design (Mockup)
- **Left Panel:** List of plugins with checkboxes. Clicking a plugin shows details on the right.
- **Right Panel (Top):** Plugin details (Name, Version, Author, Description).
- **Right Panel (Bottom):** Log console showing cargo output.
- **Bottom Bar:** "Sync & Build", "Sync & Run", "Release Build", "Import .verbium".
