use crate::{api::ApiError, config::Language, i18n::text};
use std::time::{Duration, Instant};

pub struct Notice {
    pub message: String,
    pub is_error: bool,
    expires_at: Instant,
}

impl Notice {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: false,
            expires_at: Instant::now() + Duration::from_secs(4),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: true,
            expires_at: Instant::now() + Duration::from_secs(6),
        }
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    pub fn remaining(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }

    pub fn from_api_error(language: Language, error: ApiError) -> Self {
        let message = if let Some(message) = error.server_message() {
            message.to_string()
        } else {
            let key = match error {
                ApiError::Transport(_) => "api.error.network",
                ApiError::InvalidResponse(_) => "api.error.invalid_response",
                ApiError::Server { .. } => "api.error.server",
            };
            format!("{}: {}", text(language, key), error.detail())
        };

        Self::error(message)
    }
}
