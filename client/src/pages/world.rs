use crate::{components::map, config::Language, i18n::text, theme};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, language: Language) {
    ui.heading(text(language, "world.title"));
    ui.label(text(language, "world.description"));
    ui.add_space(18.0);

    ui.columns(3, |columns| {
        metric(
            &mut columns[0],
            text(language, "world.creatures"),
            "--",
            theme::ACCENT,
        );
        metric(
            &mut columns[1],
            text(language, "world.current_tick"),
            "--",
            theme::WARNING,
        );
        metric(
            &mut columns[2],
            text(language, "world.server"),
            text(language, "world.connected"),
            theme::SUCCESS,
        );
    });
    ui.add_space(18.0);

    egui::Frame::default()
        .fill(theme::SURFACE)
        .corner_radius(10)
        .inner_margin(12.0)
        .show(ui, |ui| map::show(ui, language));
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str, color: egui::Color32) {
    egui::Frame::default()
        .fill(theme::SURFACE)
        .corner_radius(10)
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.weak(label);
            ui.add_space(6.0);
            ui.colored_label(color, egui::RichText::new(value).size(22.0).strong());
        });
}
