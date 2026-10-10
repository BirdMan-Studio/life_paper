use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    InvalidCredentials,
    Unauthorized,
    Forbidden,
    NotFound,
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
    request_id: String,
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
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "没有权限执行此操作".to_string(),
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "RESOURCE_NOT_FOUND",
                "请求的资源不存在".to_string(),
            ),
            Self::Conflict(message) => (StatusCode::CONFLICT, "RESOURCE_CONFLICT", message),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "服务器内部错误".to_string(),
            ),
        };

        let request_id = Uuid::new_v4().simple().to_string();
        tracing::warn!(target: "api", %request_id, %status, code, %message, "request failed");

        (
            status,
            Json(ErrorResponse {
                error: ErrorBody {
                    code,
                    message,
                    request_id,
                },
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use serde_json::Value;

    #[tokio::test]
    async fn forbidden_error_has_status_code_and_request_id() {
        let response = ApiError::Forbidden.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["error"]["code"], "FORBIDDEN");
        assert!(
            body["error"]["request_id"]
                .as_str()
                .is_some_and(|request_id| !request_id.is_empty())
        );
    }

    #[tokio::test]
    async fn not_found_error_has_status_code_and_request_id() {
        let response = ApiError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["error"]["code"], "RESOURCE_NOT_FOUND");
        assert!(
            body["error"]["request_id"]
                .as_str()
                .is_some_and(|request_id| !request_id.is_empty())
        );
    }
}
