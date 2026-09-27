use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    InvalidCredentials,
    Unauthorized,
    Conflict(String),
    Internal,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", message),
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "INVALID_CREDENTIALS",
                "用户名或密码错误".to_string(),
            ),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "未登录或登录状态已失效".to_string(),
            ),
            Self::Conflict(message) => (StatusCode::CONFLICT, "RESOURCE_CONFLICT", message),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "服务器内部错误".to_string(),
            ),
        };

        (
            status,
            Json(ErrorResponse {
                error: ErrorBody { code, message },
            }),
        )
            .into_response()
    }
}
