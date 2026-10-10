use crate::{
    api::ApiClient,
    components::shell::{self, ShellAction},
    config::{ClientConfig, ConfigStore},
    i18n::text,
    pages::{
        Page,
        auth::{AuthPage, AuthPageEvent},
        creatures::{CreaturesPage, CreaturesPageEvent},
        models,
        settings::{SettingsPage, SettingsPageEvent},
        world,
    },
    state::{Notice, SessionEvent, SessionState, StoredSession, TokenStore},
    theme,
};
use eframe::egui;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};

pub struct LifePaperApp {
    config: ClientConfig,
    config_store: ConfigStore,
    api_client: ApiClient,
    runtime: Runtime,
    page: Page,
    auth_page: AuthPage,
    settings_page: SettingsPage,
    creatures_page: CreaturesPage,
    session: SessionState,
    notice: Option<Notice>,
    navigation_collapsed: bool,
}

impl LifePaperApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        theme::install(&creation_context.egui_ctx);

        let config_store = ConfigStore::new();
        let (config, mut notice) = match config_store.load() {
            Ok(config) => (config, None),
            Err(error) => {
                let config = ClientConfig::default();
                let notice = Notice::error(format!(
                    "{}: {}",
                    text(config.language, error.message_key),
                    error.detail
                ));
                (config, Some(notice))
            }
        };

        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create client async runtime");

        let api_client = ApiClient::new(config.server_url.clone());
        let mut session = SessionState::new();
        match TokenStore::load(&config.server_url) {
            Ok(Some(stored)) => session.start_restore(stored, &api_client, &runtime),
            Ok(None) => {}
            Err(error) => {
                notice = Some(Notice::error(format!(
                    "{}: {}",
                    text(config.language, "credential.load_failed"),
                    error
                )));
            }
        }

        Self {
            api_client,
            auth_page: AuthPage::new(),
            settings_page: SettingsPage::new(&config),
            creatures_page: CreaturesPage::new(),
            session,
            config,
            config_store,
            runtime,
            page: Page::Auth,
            notice,
            navigation_collapsed: false,
        }
    }

    fn is_busy(&self) -> bool {
        self.auth_page.is_busy()
            || self.settings_page.is_busy()
            || self.creatures_page.is_busy()
            || self.session.is_busy()
    }

    fn clear_session(&mut self) {
        self.session.clear();
        self.auth_page.clear_password();
        self.settings_page.sync(&self.config);
        self.creatures_page.reset();
        self.page = Page::Auth;
    }

    fn clear_saved_session(&mut self, notice: Notice) {
        let delete_result = TokenStore::delete(&self.config.server_url);
        self.clear_session();
        self.notice = Some(match delete_result {
            Ok(()) => notice,
            Err(error) => Notice::error(format!(
                "{}: {}",
                text(self.config.language, "credential.delete_failed"),
                error
            )),
        });
    }

    fn poll_session(&mut self) {
        let Some(event) = self.session.poll() else {
            return;
        };
        let language = self.config.language;

        match event {
            SessionEvent::Restored(session) => {
                self.session.set(session);
                self.page = Page::World;
                self.notice = Some(Notice::success(text(language, "api.session_restored")));
            }
            SessionEvent::LoggedOut => {
                self.clear_saved_session(Notice::success(text(language, "api.logout_success")));
            }
            SessionEvent::Unauthorized => {
                self.clear_saved_session(Notice::error(text(language, "api.session_expired")));
            }
            SessionEvent::Error(error) => {
                self.notice = Some(Notice::from_api_error(language, error));
            }
        }
    }

    fn handle_auth_event(&mut self, event: AuthPageEvent) {
        match event {
            AuthPageEvent::OpenSettings => {
                self.settings_page.sync(&self.config);
                self.page = Page::Settings;
                self.notice = None;
            }
            AuthPageEvent::LoggedIn { session, notice } => {
                let save_result = TokenStore::save(
                    &self.config.server_url,
                    &StoredSession {
                        token: session.token.clone(),
                        expires_at: session.expires_at.clone(),
                    },
                );
                self.session.set(session);
                self.page = Page::World;
                self.notice = Some(match save_result {
                    Ok(()) => notice,
                    Err(error) => Notice::error(format!(
                        "{}: {}",
                        text(self.config.language, "credential.save_failed"),
                        error
                    )),
                });
            }
            AuthPageEvent::Notice(notice) => self.notice = Some(notice),
        }
    }

    fn handle_settings_event(&mut self, event: SettingsPageEvent) {
        match event {
            SettingsPageEvent::Back => {
                self.settings_page.sync(&self.config);
                self.page = if self.session.is_authenticated() {
                    Page::World
                } else {
                    Page::Auth
                };
            }
            SettingsPageEvent::Save(config) => self.save_config(config),
            SettingsPageEvent::AccountDeleted(notice)
            | SettingsPageEvent::SessionExpired(notice) => {
                self.clear_saved_session(notice);
            }
            SettingsPageEvent::Notice(notice) => self.notice = Some(notice),
        }
    }

    fn handle_creatures_event(&mut self, event: CreaturesPageEvent) {
        match event {
            CreaturesPageEvent::SessionExpired => self.clear_saved_session(Notice::error(text(
                self.config.language,
                "api.session_expired",
            ))),
            CreaturesPageEvent::Notice(notice) => self.notice = Some(notice),
        }
    }

    fn save_config(&mut self, config: ClientConfig) {
        let language = config.language;
        let server_changed = self.config.server_url != config.server_url;
        let previous_server_url = self.config.server_url.clone();

        match self.config_store.save(&config) {
            Ok(()) => {
                self.config = config;
                self.api_client.set_base_url(self.config.server_url.clone());
                self.settings_page.sync(&self.config);

                if server_changed {
                    let delete_result = TokenStore::delete(&previous_server_url);
                    self.clear_session();
                    self.notice = Some(match delete_result {
                        Ok(()) => Notice::success(text(language, "settings.server_changed_logout")),
                        Err(error) => Notice::error(format!(
                            "{}: {}",
                            text(language, "credential.delete_failed"),
                            error
                        )),
                    });
                } else {
                    self.notice = Some(Notice::success(text(language, "settings.saved")));
                }
            }
            Err(error) => {
                self.notice = Some(Notice::error(format!(
                    "{}: {}",
                    text(language, error.message_key),
                    error.detail
                )));
            }
        }
    }
}

impl eframe::App for LifePaperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.notice.as_ref().is_some_and(Notice::is_expired) {
            self.notice = None;
        }
        self.poll_session();

        if let Some(event) = self.auth_page.poll(self.config.language) {
            self.handle_auth_event(event);
        }
        if let Some(event) = self.settings_page.poll() {
            self.handle_settings_event(event);
        }
        if let Some(event) = self.creatures_page.poll(self.config.language) {
            self.handle_creatures_event(event);
        }

        let language = self.config.language;
        let is_busy = self.is_busy();
        let username = self
            .session
            .current()
            .map(|session| session.username.clone());
        let token = self.session.current().map(|session| session.token.clone());

        let mut shell_action = None;
        egui::TopBottomPanel::top("top_bar")
            .exact_height(54.0)
            .frame(egui::Frame::default().fill(theme::SURFACE))
            .show(ctx, |ui| {
                shell_action = shell::top_bar(
                    ui,
                    language,
                    &self.config.server_url,
                    username.as_deref(),
                    is_busy,
                    !self.navigation_collapsed,
                );
            });
        match shell_action {
            Some(ShellAction::Logout) => {
                self.session.start_logout(&self.api_client, &self.runtime);
                self.notice = None;
            }
            Some(ShellAction::ToggleNavigation) => {
                self.navigation_collapsed = !self.navigation_collapsed;
            }
            None => {}
        }

        if self.session.is_authenticated()
            && matches!(
                self.page,
                Page::World | Page::Creatures | Page::Models | Page::Settings
            )
            && !self.navigation_collapsed
        {
            egui::SidePanel::left("navigation")
                .exact_width(176.0)
                .resizable(false)
                .frame(egui::Frame::default().fill(theme::SIDEBAR))
                .show(ctx, |ui| shell::navigation(ui, language, &mut self.page));
        }

        let mut auth_event = None;
        let mut settings_event = None;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(theme::BACKGROUND)
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| match self.page {
                Page::Auth => {
                    auth_event = self.auth_page.show(
                        ui,
                        language,
                        &self.config.server_url,
                        &self.api_client,
                        &self.runtime,
                        self.session.is_busy(),
                    );
                }
                Page::World => world::show(ui, language),
                Page::Creatures => {
                    if let Some(token) = token.as_deref() {
                        self.creatures_page.show(
                            ui,
                            language,
                            token,
                            &self.api_client,
                            &self.runtime,
                        );
                    }
                }
                Page::Models => models::show(ui, language),
                Page::Settings => {
                    settings_event = self.settings_page.show(
                        ui,
                        self.session.current(),
                        &self.api_client,
                        &self.runtime,
                        self.session.is_busy(),
                    );
                }
            });

        if let Some(event) = auth_event {
            self.handle_auth_event(event);
        }
        if let Some(event) = settings_event {
            self.handle_settings_event(event);
        }

        if let Some(notice) = &self.notice {
            if shell::toast(ctx, notice, language) {
                self.notice = None;
            } else {
                ctx.request_repaint_after(notice.remaining());
            }
        }

        if self.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}
