use crate::{
    api::{AccountProfile, ApiClient, ApiError, LoginData, RegisteredUser},
    config::Language,
    i18n::text,
    state::{Notice, Session},
    theme,
};
use eframe::egui;
use std::sync::mpsc::{self, Receiver, Sender};
use tokio::runtime::Runtime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    Login,
    Register,
}

enum AuthResult {
    Registered(Result<RegisteredUser, ApiError>),
    LoggedIn(Result<(LoginData, AccountProfile), ApiError>),
}

pub enum AuthPageEvent {
    OpenSettings,
    LoggedIn { session: Session, notice: Notice },
    Notice(Notice),
}

pub struct AuthPage {
    mode: AuthMode,
    username: String,
    password: String,
    email: String,
    pending: bool,
    sender: Sender<AuthResult>,
    receiver: Receiver<AuthResult>,
}

impl AuthPage {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            mode: AuthMode::Login,
            username: String::new(),
            password: String::new(),
            email: String::new(),
            pending: false,
            sender,
            receiver,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.pending
    }

    pub fn clear_password(&mut self) {
        self.password.clear();
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        language: Language,
        server_url: &str,
        client: &ApiClient,
        runtime: &Runtime,
        external_busy: bool,
    ) -> Option<AuthPageEvent> {
        let mut page_event = None;
        let is_busy = self.pending || external_busy;

        ui.vertical_centered(|ui| {
            ui.add_space(42.0);
            ui.label(
                egui::RichText::new("LIFE PAPER")
                    .size(30.0)
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);
            ui.weak(text(language, "auth.tagline"));
            ui.add_space(28.0);

            egui::Frame::default()
                .fill(theme::SURFACE)
                .corner_radius(12)
                .inner_margin(24.0)
                .show(ui, |ui| {
                    ui.set_width(390.0);
                    ui.columns(2, |columns| {
                        if columns[0]
                            .selectable_label(
                                self.mode == AuthMode::Login,
                                text(language, "auth.login"),
                            )
                            .clicked()
                        {
                            self.mode = AuthMode::Login;
                        }
                        if columns[1]
                            .selectable_label(
                                self.mode == AuthMode::Register,
                                text(language, "auth.register"),
                            )
                            .clicked()
                        {
                            self.mode = AuthMode::Register;
                        }
                    });

                    ui.add_space(20.0);
                    field_label(ui, text(language, "auth.username"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.username)
                            .desired_width(f32::INFINITY)
                            .hint_text(text(language, "auth.username_hint")),
                    );

                    if self.mode == AuthMode::Register {
                        ui.add_space(14.0);
                        field_label(ui, text(language, "auth.email"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.email)
                                .desired_width(f32::INFINITY)
                                .hint_text("name@example.com"),
                        );
                    }

                    ui.add_space(14.0);
                    field_label(ui, text(language, "auth.password"));
                    let password = ui.add(
                        egui::TextEdit::singleline(&mut self.password)
                            .password(true)
                            .desired_width(f32::INFINITY)
                            .hint_text(text(language, "auth.password_hint")),
                    );

                    ui.add_space(22.0);
                    let action_text = match self.mode {
                        AuthMode::Login => text(language, "auth.login"),
                        AuthMode::Register => text(language, "auth.create_account"),
                    };
                    let submitted = ui
                        .add_enabled_ui(!is_busy, |ui| {
                            ui.add_sized(
                                [ui.available_width(), 40.0],
                                egui::Button::new(action_text),
                            )
                            .clicked()
                        })
                        .inner
                        || (!is_busy
                            && password.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter)));

                    if submitted {
                        match self.mode {
                            AuthMode::Login => self.start_login(client, runtime),
                            AuthMode::Register => self.start_register(client, runtime),
                        }
                    }

                    if is_busy {
                        ui.add_space(10.0);
                        ui.horizontal_centered(|ui| {
                            ui.add(egui::Spinner::new());
                            ui.weak(text(language, "api.loading"));
                        });
                    }

                    ui.add_space(14.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(text(language, "auth.current_server"));
                        ui.monospace(server_url);
                    });
                });

            ui.add_space(16.0);
            if ui
                .add_enabled(!is_busy, egui::Button::new(text(language, "auth.settings")))
                .clicked()
            {
                page_event = Some(AuthPageEvent::OpenSettings);
            }
        });

        page_event
    }

    fn start_register(&mut self, client: &ApiClient, runtime: &Runtime) {
        if self.pending {
            return;
        }
        self.pending = true;
        let username = self.username.trim().to_string();
        let password = self.password.clone();
        let email = self.email.trim().to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        runtime.spawn(async move {
            let result = client.register(&username, &password, &email).await;
            let _ = sender.send(AuthResult::Registered(result));
        });
    }

    fn start_login(&mut self, client: &ApiClient, runtime: &Runtime) {
        if self.pending {
            return;
        }
        self.pending = true;
        let password = self.password.clone();
        let username = self.username.trim().to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        runtime.spawn(async move {
            let result = async {
                let login = client.login(&username, &password).await?;
                let profile = client.account_me(&login.token).await?;
                Ok((login, profile))
            }
            .await;
            let _ = sender.send(AuthResult::LoggedIn(result));
        });
    }

    pub fn poll(&mut self, language: Language) -> Option<AuthPageEvent> {
        let result = self.receiver.try_recv().ok()?;
        self.pending = false;
        Some(match result {
            AuthResult::Registered(Ok(user)) => {
                self.mode = AuthMode::Login;
                self.username = user.username.clone();
                self.password.clear();
                self.email.clear();
                AuthPageEvent::Notice(Notice::success(format!(
                    "{}: {} (#{})",
                    text(language, "api.register_success"),
                    user.username,
                    user.user_id
                )))
            }
            AuthResult::Registered(Err(error)) => {
                AuthPageEvent::Notice(Notice::from_api_error(language, error))
            }
            AuthResult::LoggedIn(Ok((login, profile))) => {
                self.password.clear();
                AuthPageEvent::LoggedIn {
                    session: Session {
                        id: profile.id,
                        token: login.token,
                        username: profile.username,
                        email: profile.email,
                        expires_at: login.expires_at,
                    },
                    notice: Notice::success(text(language, "api.login_success")),
                }
            }
            AuthResult::LoggedIn(Err(error)) => {
                AuthPageEvent::Notice(Notice::from_api_error(language, error))
            }
        })
    }
}

fn field_label(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).strong());
    ui.add_space(4.0);
}
