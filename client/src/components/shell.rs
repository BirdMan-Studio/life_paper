use crate::{config::Language, i18n::text, pages::Page, state::Notice, theme};
use eframe::egui;

pub enum ShellAction {
    Logout,
    ToggleNavigation,
}

pub fn top_bar(
    ui: &mut egui::Ui,
    language: Language,
    server_url: &str,
    username: Option<&str>,
    is_busy: bool,
    navigation_visible: bool,
) -> Option<ShellAction> {
    let mut action = None;
    ui.horizontal_centered(|ui| {
        ui.add_space(16.0);
        if username.is_some()
            && ui
                .button(if navigation_visible { "<" } else { ">" })
                .on_hover_text(if navigation_visible {
                    text(language, "navigation.collapse")
                } else {
                    text(language, "navigation.expand")
                })
                .clicked()
        {
            action = Some(ShellAction::ToggleNavigation);
        }
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
    let response = ui
        .add_sized(
            [ui.available_width(), 40.0],
            egui::Button::new(label).selected(selected),
        )
        .on_hover_text(label);
    if response.clicked() {
        *page = target;
    }
}

pub fn toast(ctx: &egui::Context, notice: &Notice, language: Language) -> bool {
    let mut dismissed = false;
    let accent = if notice.is_error {
        theme::DANGER
    } else {
        theme::SUCCESS
    };
    egui::Area::new(egui::Id::new("global_notice_toast"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 70.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(theme::SURFACE)
                .stroke(egui::Stroke::new(1.0, accent))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(accent, if notice.is_error { "!" } else { "OK" });
                        ui.label(&notice.message);
                        if ui
                            .button("x")
                            .on_hover_text(text(language, "common.close"))
                            .clicked()
                        {
                            dismissed = true;
                        }
                    });
                });
        });
    dismissed
}
