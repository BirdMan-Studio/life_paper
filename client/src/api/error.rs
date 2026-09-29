use serde::Deserialize;

#[derive(Debug)]
pub enum ApiError {
    Server {
        status: u16,
        code: String,
        message: String,
    },
    Transport(String),
    InvalidResponse(String),
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
}

impl ApiError {
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Server { status: 401, .. })
    }

    pub fn server_message(&self) -> Option<&str> {
        match self {
            Self::Server { message, .. } => Some(message),
            Self::Transport(_) | Self::InvalidResponse(_) => None,
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Self::Server { code, .. } => code,
            Self::Transport(detail) | Self::InvalidResponse(detail) => detail,
        }
    }

    pub(crate) async fn from_response(response: reqwest::Response) -> Self {
        let status = response.status().as_u16();
        match response.bytes().await {
            Ok(body) => match serde_json::from_slice::<ErrorResponse>(&body) {
                Ok(error) => Self::Server {
                    status,
                    code: error.error.code,
                    message: error.error.message,
                },
                Err(error) => Self::InvalidResponse(error.to_string()),
            },
            Err(error) => Self::Transport(error.to_string()),
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error.to_string())
    }
}
