use crate::{config::Language, i18n::text};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, language: Language) {
    ui.heading(text(language, "navigation.models"));
    ui.add_space(8.0);
    ui.label(text(language, "models.description"));
    ui.add_space(24.0);
    ui.weak(text(language, "common.coming_soon"));
}
