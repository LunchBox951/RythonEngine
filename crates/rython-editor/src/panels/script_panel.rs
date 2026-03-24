use std::path::{Path, PathBuf};

use crate::project::format::{ProjectConfig, ScriptAssociation};
use crate::project::scaffold::{generate_script, write_script, ScaffoldTemplate};
use crate::state::selection::SelectionState;

// ── Types ─────────────────────────────────────────────────────────────────────

struct ScriptEntry {
    filename: String,
    path: PathBuf,
}

// ── Panel ─────────────────────────────────────────────────────────────────────

pub struct ScriptPanel {
    scripts: Vec<ScriptEntry>,
    selected_idx: Option<usize>,

    // "New Script" dialog state
    dialog_open: bool,
    dialog_name: String,
    dialog_template: ScaffoldTemplate,
    dialog_error: Option<String>,
}

impl ScriptPanel {
    pub fn new() -> Self {
        Self {
            scripts: Vec::new(),
            selected_idx: None,
            dialog_open: false,
            dialog_name: String::new(),
            dialog_template: ScaffoldTemplate::BasicModule,
            dialog_error: None,
        }
    }

    /// Re-scan `<root>/scripts/` for `.py` files.
    pub fn refresh(&mut self, root: &Path) {
        self.scripts.clear();
        let scripts_dir = root.join("scripts");
        let Ok(entries) = std::fs::read_dir(&scripts_dir) else {
            return;
        };
        let mut found: Vec<ScriptEntry> = entries
            .filter_map(|e| {
                let entry = e.ok()?;
                let path = entry.path();
                if path.extension()?.to_str()? == "py" {
                    let filename = path.file_name()?.to_string_lossy().into_owned();
                    Some(ScriptEntry { filename, path })
                } else {
                    None
                }
            })
            .collect();
        found.sort_by(|a, b| a.filename.cmp(&b.filename));
        self.scripts = found;
    }

    /// Draw the panel.
    ///
    /// `project_dirty` is set to `true` whenever associations are modified.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project_root: Option<&PathBuf>,
        config: &mut ProjectConfig,
        selection: &SelectionState,
        project_dirty: &mut bool,
    ) {
        ui.heading("Scripts");
        ui.separator();

        // ── Toolbar ───────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            if ui.button("+ New Script").clicked() {
                self.dialog_open = true;
                self.dialog_name.clear();
                self.dialog_template = ScaffoldTemplate::BasicModule;
                self.dialog_error = None;
            }
            if ui.button("+ Script Class").clicked() {
                self.dialog_open = true;
                self.dialog_name.clear();
                self.dialog_template = ScaffoldTemplate::ScriptClass;
                self.dialog_error = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⟳").on_hover_text("Refresh").clicked() {
                    if let Some(root) = project_root {
                        self.refresh(root);
                    }
                }
            });
        });

        ui.separator();

        if project_root.is_none() {
            ui.label("No project open.");
            return;
        }
        let root = project_root.unwrap();

        // ── Script list ───────────────────────────────────────────────────────
        if self.scripts.is_empty() {
            ui.label("No scripts found. Use '+ New Script' to create one.");
        } else {
            egui::ScrollArea::vertical()
                .id_salt("script_list")
                .max_height(300.0)
                .show(ui, |ui| {
                    for (i, entry) in self.scripts.iter().enumerate() {
                        let is_selected = self.selected_idx == Some(i);

                        // Annotation: show attached entity tag if any
                        let annotation = config
                            .script_associations
                            .iter()
                            .find(|a| a.script == entry.filename)
                            .map(|a| format!("  (attached: {})", a.entity_tag))
                            .unwrap_or_default();

                        let label = format!("{}{}", entry.filename, annotation);
                        if ui.selectable_label(is_selected, &label).clicked() {
                            self.selected_idx = Some(i);
                        }
                    }
                });
        }

        ui.separator();

        // ── Actions for selected script ───────────────────────────────────────
        let selected_script = self.selected_idx.and_then(|i| self.scripts.get(i));

        ui.horizontal(|ui| {
            let can_open = selected_script.is_some();
            if ui
                .add_enabled(can_open, egui::Button::new("Open in IDE"))
                .clicked()
            {
                if let Some(entry) = selected_script {
                    open_in_ide(&entry.path);
                }
            }

            // "Attach" — associate selected script with selected entity
            let selected_entity_tag = selection.selected_entity().map(|_| {
                // Use entity ID as the tag string since we can't access Scene here
                "selected_entity".to_string()
            });

            let can_attach = can_open && selected_entity_tag.is_some();
            if ui
                .add_enabled(can_attach, egui::Button::new("Attach to Entity"))
                .on_hover_text("Associate this script with the currently selected entity")
                .clicked()
            {
                if let (Some(entry), Some(entity_tag)) = (selected_script, selected_entity_tag) {
                    let class_name = stem_to_class(&entry.filename);
                    let script_name = entry.filename.clone();
                    // Remove any existing association for this script, then add new one
                    config.script_associations.retain(|a| a.script != script_name);
                    config.script_associations.push(ScriptAssociation {
                        entity_tag,
                        script: script_name,
                        class: class_name,
                    });
                    *project_dirty = true;
                }
            }

            // Remove association
            if let Some(entry) = selected_script {
                let has_assoc = config
                    .script_associations
                    .iter()
                    .any(|a| a.script == entry.filename);
                if has_assoc
                    && ui
                        .add_enabled(true, egui::Button::new("Remove Association"))
                        .clicked()
                {
                    let name = entry.filename.clone();
                    config.script_associations.retain(|a| a.script != name);
                    *project_dirty = true;
                }
            }
        });

        // ── New-script dialog (modal window) ──────────────────────────────────
        if self.dialog_open {
            let mut should_close = false;
            let mut should_create = false;

            egui::Window::new("New Script")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    egui::Grid::new("new_script_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Name:");
                            let resp = ui.text_edit_singleline(&mut self.dialog_name);
                            if resp.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                should_create = true;
                            }
                            ui.end_row();

                            ui.label("Template:");
                            egui::ComboBox::from_id_salt("template_combo")
                                .selected_text(self.dialog_template.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.dialog_template,
                                        ScaffoldTemplate::BasicModule,
                                        "Basic Module",
                                    );
                                    ui.selectable_value(
                                        &mut self.dialog_template,
                                        ScaffoldTemplate::ScriptClass,
                                        "Script Class",
                                    );
                                    ui.selectable_value(
                                        &mut self.dialog_template,
                                        ScaffoldTemplate::PerFrameUpdate,
                                        "Per-Frame Update",
                                    );
                                });
                            ui.end_row();
                        });

                    if let Some(err) = &self.dialog_error {
                        ui.colored_label(egui::Color32::RED, err);
                    }

                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            should_create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                });

            if should_create {
                let name = self.dialog_name.trim().to_string();
                if name.is_empty() {
                    self.dialog_error = Some("Name cannot be empty.".to_string());
                } else {
                    let filename = ensure_py_extension(&name);
                    let content =
                        generate_script(self.dialog_template, &stem(&filename), &config.name);
                    match write_script(root, &filename, &content) {
                        Ok(()) => {
                            self.refresh(root);
                            should_close = true;
                        }
                        Err(e) => {
                            self.dialog_error = Some(format!("Write error: {e}"));
                        }
                    }
                }
            }

            if should_close {
                self.dialog_open = false;
                self.dialog_error = None;
            }
        }
    }
}

impl Default for ScriptPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn open_in_ide(path: &Path) {
    let editor = std::env::var("EDITOR").unwrap_or_default();
    if !editor.is_empty() {
        let _ = std::process::Command::new(&editor).arg(path).spawn();
        return;
    }
    // Try VS Code
    if std::process::Command::new("code")
        .arg("--version")
        .output()
        .is_ok()
    {
        let _ = std::process::Command::new("code").arg(path).spawn();
        return;
    }
    // Fallback: xdg-open / open
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(not(target_os = "macos"))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// Append `.py` if the name doesn't already end with it.
fn ensure_py_extension(name: &str) -> String {
    if name.ends_with(".py") {
        name.to_string()
    } else {
        format!("{name}.py")
    }
}

/// Strip `.py` extension to get the bare stem.
fn stem(filename: &str) -> String {
    filename.strip_suffix(".py").unwrap_or(filename).to_string()
}

/// Derive a likely class name from a `.py` filename (PascalCase of stem).
fn stem_to_class(filename: &str) -> String {
    let s = stem(filename);
    s.split(|c: char| c == '_' || c == '-' || c == ' ')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}
