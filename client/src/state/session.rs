use crate::{
    api::{AccountProfile, ApiClient, ApiError},
    state::StoredSession,
};
use std::sync::mpsc::{self, Receiver, Sender};
use tokio::runtime::Runtime;

pub struct Session {
    pub id: i64,
    pub token: String,
    pub username: String,
    pub email: String,
    pub expires_at: String,
}

enum SessionResult {
    Restored {
        stored: StoredSession,
        result: Result<AccountProfile, ApiError>,
    },
    LoggedOut(Result<(), ApiError>),
}

pub enum SessionEvent {
    Restored(Session),
    LoggedOut,
    Unauthorized,
    Error(ApiError),
}

pub struct SessionState {
    current: Option<Session>,
    pending: bool,
    sender: Sender<SessionResult>,
    receiver: Receiver<SessionResult>,
}

impl SessionState {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            current: None,
            pending: false,
            sender,
            receiver,
        }
    }

    pub fn current(&self) -> Option<&Session> {
        self.current.as_ref()
    }

    pub fn is_authenticated(&self) -> bool {
        self.current.is_some()
    }

    pub fn is_busy(&self) -> bool {
        self.pending
    }

    pub fn set(&mut self, session: Session) {
        self.current = Some(session);
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.pending = false;
    }

    pub fn start_restore(&mut self, stored: StoredSession, client: &ApiClient, runtime: &Runtime) {
        if self.pending || self.current.is_some() {
            return;
        }

        self.pending = true;
        let token = stored.token.clone();
        let client = client.clone();
        let sender = self.sender.clone();
        runtime.spawn(async move {
            let result = client.account_me(&token).await;
            let _ = sender.send(SessionResult::Restored { stored, result });
        });
    }

    pub fn start_logout(&mut self, client: &ApiClient, runtime: &Runtime) {
        let Some(session) = &self.current else {
            return;
        };
        if self.pending {
            return;
        }

        self.pending = true;
        let token = session.token.clone();
        let client = client.clone();
        let sender = self.sender.clone();
        runtime.spawn(async move {
            let _ = sender.send(SessionResult::LoggedOut(client.logout(&token).await));
        });
    }

    pub fn poll(&mut self) -> Option<SessionEvent> {
        let result = self.receiver.try_recv().ok()?;
        self.pending = false;

        Some(match result {
            SessionResult::Restored {
                stored,
                result: Ok(profile),
            } => SessionEvent::Restored(Session {
                id: profile.id,
                token: stored.token,
                username: profile.username,
                email: profile.email,
                expires_at: stored.expires_at,
            }),
            SessionResult::Restored {
                result: Err(error), ..
            }
            | SessionResult::LoggedOut(Err(error))
                if error.is_unauthorized() =>
            {
                SessionEvent::Unauthorized
            }
            SessionResult::Restored {
                result: Err(error), ..
            }
            | SessionResult::LoggedOut(Err(error)) => SessionEvent::Error(error),
            SessionResult::LoggedOut(Ok(())) => SessionEvent::LoggedOut,
        })
    }
}
