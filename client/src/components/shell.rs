use crate::{config::Language, i18n::text, pages::Page, theme};
use eframe::egui;

pub enum ShellAction {
    Logout,
}

pub fn top_bar(
    ui: &mut egui::Ui,
    language: Language,
    server_url: &str,
    username: Option<&str>,
    is_busy: bool,
) -> Option<ShellAction> {
    let mut action = None;
    ui.horizontal_centered(|ui| {
        ui.add_space(16.0);
        ui.heading("LIFE PAPER");
        ui.separator();
        ui.weak(text(language, "app.subtitle"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(16.0);
            if username.is_some()
                && ui
                    .add_enabled(!is_busy, egui::Button::new(text(language, "shell.logout")))
                    .clicked()
            {
                action = Some(ShellAction::Logout);
            }
            if let Some(username) = username {
                ui.strong(username);
                ui.separator();
            }
            ui.colored_label(theme::SUCCESS, text(language, "app.local_config"));
            ui.weak(server_url);
        });
    });
    action
}

pub fn navigation(ui: &mut egui::Ui, language: Language, page: &mut Page) {
    ui.add_space(18.0);
    ui.label(
        egui::RichText::new(text(language, "navigation.title"))
            .weak()
            .small(),
    );
    ui.add_space(10.0);

    navigation_button(ui, page, Page::World, text(language, "navigation.world"));
    navigation_button(
        ui,
        page,
        Page::Creatures,
        text(language, "navigation.creatures"),
    );
    navigation_button(ui, page, Page::Models, text(language, "navigation.models"));

    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(16.0);
        navigation_button(
            ui,
            page,
            Page::Settings,
            text(language, "navigation.settings"),
        );
    });
}

fn navigation_button(ui: &mut egui::Ui, page: &mut Page, target: Page, label: &str) {
    let selected = *page == target;
    if ui
        .add_sized(
            [ui.available_width(), 40.0],
            egui::Button::new(label).selected(selected),
        )
        .clicked()
    {
        *page = target;
    }
}
