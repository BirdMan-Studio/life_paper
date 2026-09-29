use crate::{
    api::{ApiClient, ApiError},
    config::{ClientConfig, Language},
    i18n::text,
    state::{Notice, Session},
    theme,
};
use eframe::egui;
use std::sync::mpsc::{self, Receiver, Sender};
use tokio::runtime::Runtime;

pub enum SettingsPageEvent {
    Back,
    Save(ClientConfig),
    AccountDeleted(Notice),
    SessionExpired(Notice),
    Notice(Notice),
}

pub struct SettingsPage {
    server_url: String,
    language: Language,
    delete_confirmed: bool,
    delete_pending: bool,
    sender: Sender<Result<(), ApiError>>,
    receiver: Receiver<Result<(), ApiError>>,
}

impl SettingsPage {
    pub fn new(config: &ClientConfig) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            server_url: config.server_url.clone(),
            language: config.language,
            delete_confirmed: false,
            delete_pending: false,
            sender,
            receiver,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.delete_pending
    }

    pub fn sync(&mut self, config: &ClientConfig) {
        self.server_url.clone_from(&config.server_url);
        self.language = config.language;
        self.delete_confirmed = false;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        session: Option<&Session>,
        client: &ApiClient,
        runtime: &Runtime,
        external_busy: bool,
    ) -> Option<SettingsPageEvent> {
        let language = self.language;
        let is_busy = self.delete_pending || external_busy;
        let mut page_event = None;

        if ui
            .add_enabled(!is_busy, egui::Button::new(text(language, "settings.back")))
            .clicked()
        {
            page_event = Some(SettingsPageEvent::Back);
        }
        ui.add_space(18.0);
        ui.heading(text(language, "settings.title"));
        ui.add_space(6.0);
        ui.label(text(language, "settings.description"));
        ui.add_space(24.0);

        egui::Frame::default()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(10)
            .inner_margin(20.0)
            .show(ui, |ui| {
                ui.set_max_width(680.0);
                ui.strong(text(language, "settings.server_url"));
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.server_url)
                        .desired_width(f32::INFINITY)
                        .hint_text("http://127.0.0.1:8080/api/v1"),
                );
                ui.small(text(language, "settings.server_url_hint"));

                ui.add_space(20.0);
                ui.strong(text(language, "settings.language"));
                ui.add_space(6.0);
                egui::ComboBox::from_id_salt("language_selector")
                    .selected_text(self.language.display_name())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for candidate in Language::ALL {
                            ui.selectable_value(
                                &mut self.language,
                                candidate,
                                candidate.display_name(),
                            );
                        }
                    });

                ui.add_space(24.0);
                if ui
                    .add_enabled(
                        !is_busy,
                        egui::Button::new(text(language, "settings.save"))
                            .min_size(egui::vec2(128.0, 36.0)),
                    )
                    .clicked()
                {
                    let server_url = self.server_url.trim().trim_end_matches('/');
                    if server_url.is_empty()
                        || !(server_url.starts_with("http://")
                            || server_url.starts_with("https://"))
                    {
                        page_event = Some(SettingsPageEvent::Notice(Notice::error(text(
                            language,
                            "settings.invalid_server_url",
                        ))));
                    } else {
                        page_event = Some(SettingsPageEvent::Save(ClientConfig {
                            server_url: server_url.to_string(),
                            language: self.language,
                        }));
                    }
                }
            });

        if let Some(session) = session {
            ui.add_space(24.0);
            egui::Frame::default()
                .fill(theme::DANGER.gamma_multiply(0.08))
                .stroke(egui::Stroke::new(1.0, theme::DANGER.gamma_multiply(0.45)))
                .corner_radius(10)
                .inner_margin(20.0)
                .show(ui, |ui| {
                    ui.set_max_width(680.0);
                    ui.colored_label(
                        theme::DANGER,
                        egui::RichText::new(text(language, "settings.account_title")).strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(text(language, "settings.account_description"));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.weak(text(language, "settings.account_id"));
                        ui.monospace(session.id.to_string());
                    });
                    ui.horizontal(|ui| {
                        ui.weak(text(language, "settings.account_username"));
                        ui.monospace(&session.username);
                    });
                    ui.horizontal(|ui| {
                        ui.weak(text(language, "settings.account_email"));
                        ui.monospace(&session.email);
                    });
                    ui.add_space(12.0);
                    ui.checkbox(
                        &mut self.delete_confirmed,
                        text(language, "settings.delete_confirm"),
                    );
                    ui.add_space(8.0);
                    if ui
                        .add_enabled(
                            self.delete_confirmed && !is_busy,
                            egui::Button::new(text(language, "settings.delete_account")),
                        )
                        .clicked()
                    {
                        self.start_delete(&session.token, client, runtime);
                    }
                });
        }

        page_event
    }

    fn start_delete(&mut self, token: &str, client: &ApiClient, runtime: &Runtime) {
        if self.delete_pending {
            return;
        }
        self.delete_pending = true;
        let token = token.to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        runtime.spawn(async move {
            let _ = sender.send(client.delete_account(&token).await);
        });
    }

    pub fn poll(&mut self) -> Option<SettingsPageEvent> {
        let result = self.receiver.try_recv().ok()?;
        self.delete_pending = false;
        self.delete_confirmed = false;
        let language = self.language;
        Some(match result {
            Ok(()) => SettingsPageEvent::AccountDeleted(Notice::success(text(
                language,
                "api.account_deleted",
            ))),
            Err(error) if error.is_unauthorized() => SettingsPageEvent::SessionExpired(
                Notice::error(text(language, "api.session_expired")),
            ),
            Err(error) => SettingsPageEvent::Notice(Notice::from_api_error(language, error)),
        })
    }
}
