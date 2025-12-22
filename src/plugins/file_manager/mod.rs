use std::path::{Path, PathBuf};
use std::collections::HashSet;
use egui::{Ui, WidgetText, CollapsingHeader};
use walkdir::WalkDir;
use crate::{Plugin, AppCommand, TabInstance, Tab};

// ----------------------------------------------------------------------------
// Tab Instance
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FileExplorerTab {
    root_path: Option<PathBuf>,
    expanded_nodes: HashSet<PathBuf>,
}

impl FileExplorerTab {
    fn new() -> Self {
        Self {
            root_path: None,
            expanded_nodes: HashSet::new(),
        }
    }

    fn render_tree(&mut self, ui: &mut Ui, path: PathBuf) {
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        if path.is_dir() {
            let is_expanded = self.expanded_nodes.contains(&path);
            
            let header = CollapsingHeader::new(format!("📁 {}", name))
                .id_salt(&path) // Updated from id_source to id_salt
                .open(Some(is_expanded));

            let response = header.show(ui, |ui| {
                if let Ok(entries) = std::fs::read_dir(&path) {
                    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
                    // Directories first, then sort by name
                    paths.sort_by(|a, b| {
                        let a_is_dir = a.is_dir();
                        let b_is_dir = b.is_dir();
                        if a_is_dir != b_is_dir {
                            b_is_dir.cmp(&a_is_dir)
                        } else {
                            a.cmp(b)
                        }
                    });

                    for child_path in paths {
                        self.render_tree(ui, child_path);
                    }
                }
            });

            if response.header_response.clicked() {
                if is_expanded {
                    self.expanded_nodes.remove(&path);
                } else {
                    self.expanded_nodes.insert(path.clone());
                }
            }
        } else {
            // File display
            ui.horizontal(|ui| {
                ui.add_space(16.0); // Indentation
                if ui.selectable_label(false, format!("📄 {}", name)).clicked() {
                    println!("Selected file: {:?}", path);
                }
            });
        }
    }
}

impl TabInstance for FileExplorerTab {
    fn title(&self) -> WidgetText {
        if let Some(path) = &self.root_path {
            format!("📂 {}", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()).into()
        } else {
            "📁 Explorer".into()
        }
    }

    fn ui(&mut self, ui: &mut Ui, _control: &mut Vec<AppCommand>) {
        ui.vertical(|ui| {
            // Toolbar
            ui.horizontal(|ui| {
                if ui.button("Open Folder...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.root_path = Some(path);
                    }
                }
                if self.root_path.is_some() {
                    if ui.button("Refresh").clicked() {
                        self.expanded_nodes.retain(|p| p.exists());
                    }
                    if ui.button("Close").clicked() {
                        self.root_path = None;
                        self.expanded_nodes.clear();
                    }
                }
            });

            ui.separator();

            // Content
            if let Some(root) = self.root_path.clone() {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.render_tree(ui, root);
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("No directory selected.\nClick the button above to start exploring.");
                });
            }
        });
    }

    fn box_clone(&self) -> Box<dyn TabInstance> {
        Box::new(self.clone())
    }
}

// ----------------------------------------------------------------------------
// Plugin Implementation
// ----------------------------------------------------------------------------

pub struct FileManagerPlugin;

impl Plugin for FileManagerPlugin {
    fn name(&self) -> &str {
        "file_manager"
    }

    fn on_tab_menu(&mut self, ui: &mut Ui, control: &mut Vec<AppCommand>) {
        if ui.button("File Explorer").clicked() {
            control.push(AppCommand::OpenTab(Tab::new(Box::new(FileExplorerTab::new()))));
            ui.close_menu();
        }
    }
}

pub fn create() -> FileManagerPlugin {
    FileManagerPlugin
}