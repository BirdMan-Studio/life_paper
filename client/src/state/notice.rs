use crate::{api::ApiError, config::Language, i18n::text};

pub struct Notice {
    pub message: String,
    pub is_error: bool,
}

impl Notice {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: false,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: true,
        }
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
