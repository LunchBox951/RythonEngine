use egui::{Color32, RichText, ScrollArea, Ui};

// ── Log types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
}

// ── ConsolePanel ──────────────────────────────────────────────────────────────

/// Scrollable log panel displaying editor messages and game process output.
pub struct ConsolePanel {
    pub log: Vec<LogEntry>,
    pub filter_info: bool,
    pub filter_warn: bool,
    pub filter_error: bool,
    /// When true, automatically scrolls to the bottom when new entries arrive.
    auto_scroll: bool,
}

impl Default for ConsolePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsolePanel {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            filter_info: true,
            filter_warn: true,
            filter_error: true,
            auto_scroll: true,
        }
    }

    pub fn push(&mut self, level: LogLevel, message: impl Into<String>) {
        self.log.push(LogEntry { level, message: message.into() });
    }

    pub fn push_info(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Info, msg);
    }

    pub fn push_warn(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Warn, msg);
    }

    pub fn push_error(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Error, msg);
    }

    pub fn clear(&mut self) {
        self.log.clear();
    }

    pub fn show(&mut self, ui: &mut Ui) {
        // Toolbar
        ui.horizontal(|ui| {
            ui.heading("Console");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Clear").clicked() {
                    self.clear();
                }
                ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
                ui.separator();
                ui.checkbox(&mut self.filter_error, "Err");
                ui.checkbox(&mut self.filter_warn, "Warn");
                ui.checkbox(&mut self.filter_info, "Info");
            });
        });
        ui.separator();

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(self.auto_scroll)
            .show(ui, |ui| {
                for entry in &self.log {
                    let visible = match entry.level {
                        LogLevel::Info => self.filter_info,
                        LogLevel::Warn => self.filter_warn,
                        LogLevel::Error => self.filter_error,
                    };
                    if !visible {
                        continue;
                    }
                    let color = match entry.level {
                        LogLevel::Info => Color32::from_gray(220),
                        LogLevel::Warn => Color32::YELLOW,
                        LogLevel::Error => Color32::LIGHT_RED,
                    };
                    // selectable(true) enables text copy
                    ui.add(
                        egui::Label::new(RichText::new(&entry.message).color(color))
                            .selectable(true),
                    );
                }
            });
    }
}
