use std::path::PathBuf;

use egui::Ui;

/// Landing screen shown when no project is open.
///
/// The caller drives all actions via the output bool/Option parameters — this
/// panel is purely presentational.
pub struct WelcomePanel;

impl Default for WelcomePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl WelcomePanel {
    pub fn new() -> Self {
        Self
    }

    /// Draw the welcome screen.
    ///
    /// - `recent_projects` — list of recent project root paths (most recent first)
    /// - `on_new_project` — set to `true` if the user clicked "New Project"
    /// - `on_open_project` — set to `true` if the user clicked "Open Project"
    /// - `on_open_recent` — set to the path the user clicked in the recent list
    pub fn show(
        ui: &mut Ui,
        recent_projects: &[PathBuf],
        on_new_project: &mut bool,
        on_open_project: &mut bool,
        on_open_recent: &mut Option<PathBuf>,
    ) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);

            ui.heading("Rython Editor");
            ui.add_space(8.0);
            ui.label("Open or create a project to get started.");
            ui.add_space(24.0);

            if ui.button("  New Project…  ").clicked() {
                *on_new_project = true;
            }
            ui.add_space(6.0);
            if ui.button("  Open Project…  ").clicked() {
                *on_open_project = true;
            }

            if !recent_projects.is_empty() {
                ui.add_space(32.0);
                ui.separator();
                ui.add_space(12.0);
                ui.strong("Recent Projects");
                ui.add_space(8.0);

                for path in recent_projects {
                    let display = path.to_string_lossy();
                    if ui.link(display.as_ref()).clicked() {
                        *on_open_recent = Some(path.clone());
                    }
                }
            }

            ui.add_space(40.0);
        });
    }
}
