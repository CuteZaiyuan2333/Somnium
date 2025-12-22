use eframe::egui;
use std::path::{Path, PathBuf};
use std::fs;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
use toml_edit::{DocumentMut, value};

#[derive(Serialize, Deserialize, Clone, Debug)]
struct PluginConfig {
    #[serde(default)]
    enabled: bool,
}

#[derive(Deserialize, Debug, Clone)]
struct PluginMeta {
    plugin: PluginInfo,
    #[serde(default)]
    external_dependencies: Option<toml::Table>,
}

#[derive(Deserialize, Debug, Clone)]
struct PluginInfo {
    name: String,
    display_name: String,
    version: String,
    author: String,
    description: String,
}

struct PluginEntry {
    id: String,
    meta: PluginMeta,
    enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct LauncherConfig {
    project_dir: Option<PathBuf>,
    #[serde(default)]
    enabled_plugins: Vec<String>,
}

struct LauncherApp {
    config: LauncherConfig,
    plugins: Vec<PluginEntry>,
    logs: Arc<Mutex<String>>,
    is_running: Arc<Mutex<bool>>,
}

impl LauncherApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = Self::load_config().unwrap_or_default();
        let mut app = Self {
            config,
            plugins: Vec::new(),
            logs: Arc::new(Mutex::new(String::new())),
            is_running: Arc::new(Mutex::new(false)),
        };
        app.refresh_plugins();
        app
    }

    fn load_config() -> anyhow::Result<LauncherConfig> {
        let path = Path::new("launcher_config.toml");
        if path.exists() {
            let content = fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(LauncherConfig::default())
        }
    }

    fn save_config(&self) -> anyhow::Result<()> {
        let content = toml::to_string(&self.config)?;
        fs::write("launcher_config.toml", content)?;
        Ok(())
    }

    fn refresh_plugins(&mut self) {
        self.plugins.clear();
        let Some(main_dir) = &self.config.project_dir else { return; };
        
        let plugins_dir = main_dir.join("src/plugins");
        if let Ok(entries) = fs::read_dir(plugins_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let toml_path = path.join("plugin.toml");
                    if toml_path.exists() {
                        if let Ok(content) = fs::read_to_string(toml_path) {
                            if let Ok(meta) = toml::from_str::<PluginMeta>(&content) {
                                let id = meta.plugin.name.clone();
                                let enabled = self.config.enabled_plugins.contains(&id);
                                self.plugins.push(PluginEntry { id, meta, enabled });
                            }
                        }
                    }
                }
            }
        }
    }

    fn is_plugin_enabled_in_cargo(&self, id: &str) -> bool {
        let Some(main_dir) = &self.config.project_dir else { return false; };
        let cargo_path = main_dir.join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(cargo_path) {
            let feature_name = format!("plugin_{}", id);
            content.contains(&format!("\"{}\"", feature_name)) 
        } else {
            false
        }
    }

    fn sync_and_run(&mut self, release: bool) {
        if *self.is_running.lock().unwrap() {
            return;
        }

        let Some(main_dir) = self.config.project_dir.clone() else { return; };
        
        // Update enabled plugins in config before sync
        self.config.enabled_plugins = self.plugins.iter()
            .filter(|p| p.enabled)
            .map(|p| p.id.clone())
            .collect();
        let _ = self.save_config();

        let logs = self.logs.clone();
        let is_running = self.is_running.clone();
        
        // 1. Sync Cargo.toml
        if let Err(e) = self.sync_cargo_toml() {
            let mut l = logs.lock().unwrap();
            l.push_str(&format!("Error syncing Cargo.toml: {}\n", e));
            return;
        }

        *is_running.lock().unwrap() = true;
        {
            let mut l = logs.lock().unwrap();
            l.clear();
            l.push_str("Starting build...\n");
        }

        std::thread::spawn(move || {
            let mut args = vec!["run"];
            if release {
                args.push("--release");
            }

            let mut child = Command::new("cargo")
                .args(&args)
                .current_dir(&main_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Failed to start cargo");

            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();

            let logs_clone = logs.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().flatten() {
                    let mut l = logs_clone.lock().unwrap();
                    l.push_str(&line);
                    l.push('\n');
                }
            });

            let logs_clone = logs.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    let mut l = logs_clone.lock().unwrap();
                    l.push_str(&line);
                    l.push('\n');
                }
            });

            let _ = child.wait();
            *is_running.lock().unwrap() = false;
            let mut l = logs.lock().unwrap();
            l.push_str("\nProcess finished.\n");
        });
    }

    fn sync_cargo_toml(&self) -> anyhow::Result<()> {
        let Some(main_dir) = &self.config.project_dir else { return Err(anyhow::anyhow!("No project dir")); };
        let cargo_path = main_dir.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_path)?;
        let mut doc = content.parse::<DocumentMut>()?;

        // 1. Update features
        let mut enabled_features = Vec::new();
        let mut all_plugin_features = Vec::new();

        for plugin in &self.plugins {
            let feat = format!("plugin_{}", plugin.id);
            all_plugin_features.push(feat.clone());
            if plugin.enabled {
                enabled_features.push(feat);
            }
        }

        // Update [features] section
        if let Some(features) = doc.get_mut("features").and_then(|v| v.as_table_mut()) {
            let keys: Vec<String> = features.iter().map(|(k, _)| k.to_string()).collect();
            for k in keys {
                if k.starts_with("plugin_") {
                    features.remove(&k);
                }
            }

            for feat in &all_plugin_features {
                features.insert(feat, value(toml_edit::Array::new()));
            }

            let mut default_array = toml_edit::Array::new();
            for feat in enabled_features {
                default_array.push(feat);
            }
            features.insert("default", value(default_array));
        }

        // 2. Update dependencies using toml_edit instead of raw string replacement
        let mut deps_to_add = Vec::new();
        for plugin in &self.plugins {
            if plugin.enabled {
                if let Some(deps) = &plugin.meta.external_dependencies {
                    for (name, value) in deps {
                        deps_to_add.push((plugin.id.clone(), name.clone(), value.clone()));
                    }
                }
            }
        }

        let mut dep_string = String::from("\n");
        for (plugin_id, name, value) in deps_to_add {
            dep_string.push_str(&format!("# From {}\n", plugin_id));
            dep_string.push_str(&format!("{} = {}\n", name, value));
        }

        let mut final_content = doc.to_string();
        let begin_dep = "# --- BEGIN PLUGIN DEPENDENCIES ---";
        let end_dep = "# --- END PLUGIN DEPENDENCIES ---";

        if let (Some(start_idx), Some(end_idx)) = (final_content.find(begin_dep), final_content.find(end_dep)) {
            final_content.replace_range((start_idx + begin_dep.len())..end_idx, &dep_string);
        }

        fs::write(cargo_path, final_content)?;
        Ok(())
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_header").show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Verbium Launcher");
            });
            
            ui.horizontal(|ui| {
                ui.label("Verbium Project:");
                let dir_str = self.config.project_dir.as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Not Selected".to_string());
                
                if ui.button(&dir_str).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.config.project_dir = Some(path);
                        let _ = self.save_config();
                        self.refresh_plugins();
                    }
                }
                
                ui.separator();

                if ui.button("Refresh List").clicked() {
                    self.refresh_plugins();
                }
                if ui.button("Import .verbium").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Verbium Plugin", &["verbium", "zip"])
                        .pick_file() {
                            let _ = self.import_plugin(path);
                            self.refresh_plugins();
                        }
                }
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("bottom_actions").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let running = *self.is_running.lock().unwrap();
                ui.add_enabled_ui(!running && self.config.project_dir.is_some(), |ui| {
                    if ui.button("Sync & Run (Debug)").clicked() {
                        self.sync_and_run(false);
                    }
                    if ui.button("Sync & Run (Release)").clicked() {
                        self.sync_and_run(true);
                    }
                });
                if running {
                    ui.spinner();
                    ui.label("Cargo is running...");
                }
                if self.config.project_dir.is_none() {
                    ui.label("⚠ Please select Verbium project directory first.");
                }
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    ui.label("Plugins:");
                    let mut changed = false;
                    egui::ScrollArea::vertical()
                        .id_salt("plugin_list")
                        .show(ui, |ui| {
                            for plugin in &mut self.plugins {
                                if ui.checkbox(&mut plugin.enabled, &plugin.meta.plugin.display_name).changed() {
                                    changed = true;
                                }
                            }
                        });
                    
                    if changed {
                        self.config.enabled_plugins = self.plugins.iter()
                            .filter(|p| p.enabled)
                            .map(|p| p.id.clone())
                            .collect();
                        let _ = self.save_config();
                    }
                });

                columns[1].vertical(|ui| {
                    ui.label("Console Output:");
                    let logs = self.logs.lock().unwrap();
                    egui::Frame::canvas(ui.style()).show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("console")
                            .stick_to_bottom(true)
                            .max_height(ui.available_height()) // Limit to remaining space
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut logs.as_str())
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY)
                                        .layouter(&mut |ui, string, _wrap_width| {
                                            ui.fonts(|f| f.layout_no_wrap(
                                                string.to_string(),
                                                egui::TextStyle::Monospace.resolve(ui.style()),
                                                egui::Color32::LIGHT_GRAY
                                            ))
                                        })
                                );
                            });
                    });
                });
            });
        });

        if *self.is_running.lock().unwrap() {
            ctx.request_repaint();
        }
    }
}

impl LauncherApp {
    fn import_plugin(&self, path: PathBuf) -> anyhow::Result<()> {
        let Some(main_dir) = &self.config.project_dir else { return Err(anyhow::anyhow!("No project dir selected")); };
        let file = fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        
        let mut plugin_name = None;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file.name() == "plugin.toml" {
                let mut content = String::new();
                use std::io::Read;
                file.read_to_string(&mut content)?;
                let meta: PluginMeta = toml::from_str(&content)?;
                plugin_name = Some(meta.plugin.name);
                break;
            }
        }

        if let Some(name) = plugin_name {
            let dest_dir = main_dir.join("src/plugins").join(&name);
            if !dest_dir.exists() {
                fs::create_dir_all(&dest_dir)?;
            }
            archive.extract(&dest_dir)?;
            return Ok(());
        }

        Err(anyhow::anyhow!("Invalid .verbium file: plugin.toml not found"))
    }
}
fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Verbium Launcher",
        native_options,
        Box::new(|cc| Ok(Box::new(LauncherApp::new(cc))))
    )
}
