use std::path::{Path, PathBuf};

use crate::state::selection::{Selection, SelectionState};

// ── Asset categorisation ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetCategory {
    Mesh,
    Texture,
    Sound,
    Font,
    Other,
}

impl AssetCategory {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "glb" | "gltf" | "obj" => AssetCategory::Mesh,
            "png" | "jpg" | "jpeg" | "bmp" | "tga" => AssetCategory::Texture,
            "wav" | "ogg" | "mp3" | "flac" => AssetCategory::Sound,
            "ttf" | "otf" => AssetCategory::Font,
            _ => AssetCategory::Other,
        }
    }

    #[allow(dead_code)]
    fn label(self) -> &'static str {
        match self {
            AssetCategory::Mesh => "meshes",
            AssetCategory::Texture => "textures",
            AssetCategory::Sound => "sounds",
            AssetCategory::Font => "fonts",
            AssetCategory::Other => "other",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            AssetCategory::Mesh => "🔷",
            AssetCategory::Texture => "🖼",
            AssetCategory::Sound => "🔊",
            AssetCategory::Font => "🔤",
            AssetCategory::Other => "📄",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub path: PathBuf,
    pub category: AssetCategory,
    pub filename: String,
    /// Stem (filename without extension) — used as the asset ID.
    pub stem: String,
}

// ── Panel ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    All,
    Category(AssetCategory),
}

pub struct AssetBrowserPanel {
    assets: Vec<AssetEntry>,
    active_tab: Tab,
    filter: String,
    /// Path to the drag-and-drop payload being carried (if any).
    pub drag_payload: Option<PathBuf>,
}

impl AssetBrowserPanel {
    pub fn new() -> Self {
        Self {
            assets: Vec::new(),
            active_tab: Tab::All,
            filter: String::new(),
            drag_payload: None,
        }
    }

    /// Re-scan the `assets/` subdirectory of `project_root`.
    pub fn refresh(&mut self, project_root: &Path) {
        self.assets.clear();
        let assets_dir = project_root.join("assets");
        if !assets_dir.exists() {
            return;
        }
        scan_dir(&assets_dir, &mut self.assets);
        self.assets.sort_by(|a, b| a.filename.cmp(&b.filename));
    }

    /// Show the panel.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project_root: Option<&PathBuf>,
        selection: &mut SelectionState,
    ) {
        ui.horizontal(|ui| {
            ui.heading("Asset Browser");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⟳ Refresh").clicked() {
                    if let Some(root) = project_root {
                        self.refresh(root);
                    }
                }
                if ui.button("Import…").clicked() {
                    self.import_files(project_root);
                }
            });
        });

        ui.separator();

        // Category tabs
        ui.horizontal(|ui| {
            let tabs: &[(Tab, &str)] = &[
                (Tab::All, "all"),
                (Tab::Category(AssetCategory::Mesh), "meshes"),
                (Tab::Category(AssetCategory::Texture), "textures"),
                (Tab::Category(AssetCategory::Sound), "sounds"),
                (Tab::Category(AssetCategory::Font), "fonts"),
            ];
            for (tab, label) in tabs {
                let selected = self.active_tab == *tab;
                if ui.selectable_label(selected, *label).clicked() {
                    self.active_tab = *tab;
                }
            }
            ui.separator();
            ui.label("🔍");
            ui.text_edit_singleline(&mut self.filter);
        });

        ui.separator();

        if project_root.is_none() {
            ui.label("No project open.");
            return;
        }

        let filter_lower = self.filter.to_lowercase();
        let visible: Vec<&AssetEntry> = self
            .assets
            .iter()
            .filter(|a| match self.active_tab {
                Tab::All => true,
                Tab::Category(cat) => a.category == cat,
            })
            .filter(|a| {
                filter_lower.is_empty() || a.filename.to_lowercase().contains(&filter_lower)
            })
            .collect();

        if visible.is_empty() {
            ui.label("No assets found.");
            return;
        }

        let cell_size = 72.0;
        let spacing = 8.0;
        let avail_width = ui.available_width();
        let cols = ((avail_width + spacing) / (cell_size + spacing))
            .floor()
            .max(1.0) as usize;

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("asset_grid")
                .num_columns(cols)
                .spacing([spacing, spacing])
                .show(ui, |ui| {
                    let mut selected_path: Option<PathBuf> = None;
                    let mut drag_started: Option<PathBuf> = None;

                    for (i, entry) in visible.iter().enumerate() {
                        let is_selected = selection.current == Selection::Asset(entry.path.clone());

                        let (rect, resp) = ui.allocate_exact_size(
                            egui::Vec2::splat(cell_size),
                            egui::Sense::click_and_drag(),
                        );

                        // Draw cell background
                        let bg = if is_selected {
                            egui::Color32::from_rgb(60, 100, 160)
                        } else if resp.hovered() {
                            egui::Color32::from_gray(80)
                        } else {
                            egui::Color32::from_gray(50)
                        };
                        ui.painter().rect_filled(rect, 4.0, bg);

                        // Icon / type label
                        let icon_rect = egui::Rect::from_min_size(
                            rect.min + egui::Vec2::new(0.0, 4.0),
                            egui::Vec2::new(cell_size, cell_size * 0.55),
                        );
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            entry.category.icon(),
                            egui::FontId::proportional(28.0),
                            egui::Color32::WHITE,
                        );

                        // Filename label (truncated)
                        let label_rect = egui::Rect::from_min_size(
                            rect.min + egui::Vec2::new(2.0, cell_size * 0.60),
                            egui::Vec2::new(cell_size - 4.0, cell_size * 0.38),
                        );
                        let display_name = truncate_name(&entry.filename, 10);
                        ui.painter().text(
                            label_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            display_name,
                            egui::FontId::proportional(10.0),
                            egui::Color32::LIGHT_GRAY,
                        );

                        // Click → select
                        if resp.clicked() {
                            selected_path = Some(entry.path.clone());
                        }

                        // Drag → start payload
                        if resp.drag_started() {
                            drag_started = Some(entry.path.clone());
                        }

                        if (i + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }

                    if let Some(path) = selected_path {
                        selection.current = Selection::Asset(path);
                    }
                    if let Some(path) = drag_started {
                        self.drag_payload = Some(path);
                    }
                    // Clear payload when pointer is released
                    if ui.ctx().input(|i| i.pointer.any_released()) {
                        self.drag_payload = None;
                    }
                });
        });
    }

    fn import_files(&mut self, project_root: Option<&PathBuf>) {
        let Some(root) = project_root else { return };
        let result = rfd::FileDialog::new()
            .add_filter(
                "Assets",
                &[
                    "png", "jpg", "jpeg", "bmp", "tga", "glb", "gltf", "obj", "wav", "ogg", "mp3",
                    "flac", "ttf", "otf",
                ],
            )
            .pick_files();

        let Some(paths) = result else { return };

        for src_path in paths {
            let ext = src_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let cat = AssetCategory::from_extension(&ext);
            let subdir = match cat {
                AssetCategory::Mesh => "meshes",
                AssetCategory::Texture => "textures",
                AssetCategory::Sound => "sounds",
                AssetCategory::Font => "fonts",
                AssetCategory::Other => "misc",
            };

            let dest_dir = root.join("assets").join(subdir);
            let _ = std::fs::create_dir_all(&dest_dir);

            if let Some(fname) = src_path.file_name() {
                let dest = dest_dir.join(fname);
                let _ = std::fs::copy(&src_path, &dest);
            }
        }

        self.refresh(root);
    }
}

impl Default for AssetBrowserPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn scan_dir(dir: &Path, out: &mut Vec<AssetEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, out);
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let category = AssetCategory::from_extension(&ext);
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let stem = path
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(AssetEntry {
                path,
                category,
                filename,
                stem,
            });
        }
    }
}

fn truncate_name(name: &str, max_chars: usize) -> &str {
    if name.len() <= max_chars {
        name
    } else {
        // Return first max_chars chars (safe ASCII truncation)
        let end = name
            .char_indices()
            .nth(max_chars)
            .map(|(i, _)| i)
            .unwrap_or(name.len());
        &name[..end]
    }
}
