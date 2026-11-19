use crate::{api::error::ApiError, state::AppState};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Path, Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use futures_util::StreamExt;
use ring::signature::KeyPair;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const MAX_REQUEST: usize = crate::transfer::MAX_TRANSFER_REQUEST_BYTES;
const MAX_RESPONSE: usize = crate::transfer::MAX_TRANSFER_REQUEST_BYTES;
const MAX_CONCURRENT: usize = 8;

pub struct RelayService {
    client: reqwest::Client,
    slots: Arc<Semaphore>,
}
impl RelayService {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .pool_max_idle_per_host(4)
                .referer(false)
                .build()?,
            slots: Arc::new(Semaphore::new(MAX_CONCURRENT)),
        })
    }
}
pub fn relay_path(server_id: &str) -> String {
    format!("/api/v1/relay/{server_id}")
}
pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/relay/{server_id}/{*path}", any(forward))
        .with_state(state)
}
fn purpose(path: &str) -> Result<&'static str, ApiError> {
    if path
        .split('/')
        .any(|s| s.is_empty() || s == "." || s == "..")
        || !path
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"-_/".contains(&v))
    {
        return Err(ApiError::BadRequest("invalid relay path".into()));
    }
    if path == "worlds"
        || path.starts_with("worlds/")
        || matches!(path, "game-server" | "python-api")
    {
        return Ok("game-entry");
    }
    if path == "management/logs"
        || path == "management/worlds"
        || path.starts_with("management/worlds/")
    {
        return Ok("game-admin");
    }
    Err(ApiError::Forbidden)
}
fn credential(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| v.len() <= 8192)
        .ok_or(ApiError::Unauthorized)
}
async fn forward(
    State(state): State<AppState>,
    Path((id, path)): Path<(String, String)>,
    request: Request,
) -> Result<Response, ApiError> {
    let started = Instant::now();
    let expected_purpose = purpose(&path)?;
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return Ok(StatusCode::METHOD_NOT_ALLOWED.into_response());
    }
    let token = credential(request.headers())?.to_owned();
    crate::transfer::verify::<()>(
        &token,
        state.signing_key.public_key().as_ref(),
        expected_purpose,
        &id,
    )
    .map_err(|_| ApiError::Unauthorized)?;
    let server = state.admin.connected_server(&id)?;
    let Ok(permit) = state.admin.relay.slots.clone().try_acquire_owned() else {
        return Ok((StatusCode::SERVICE_UNAVAILABLE, "relay is busy").into_response());
    };
    let (parts, body) = request.into_parts();
    if parts
        .headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|v| v > MAX_REQUEST)
    {
        return Ok(StatusCode::PAYLOAD_TOO_LARGE.into_response());
    }
    let body =
        match tokio::time::timeout(Duration::from_secs(30), to_bytes(body, MAX_REQUEST)).await {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(_)) => return Ok(StatusCode::PAYLOAD_TOO_LARGE.into_response()),
            Err(_) => return Ok(StatusCode::REQUEST_TIMEOUT.into_response()),
        };
    // Only the administrator-approved origin and API path can be forwarded.
    let mut url = reqwest::Url::parse(&format!("{}/{path}", server.api_url))
        .map_err(|_| ApiError::Internal)?;
    url.set_query(parts.uri.query());
    let mut upstream = state
        .admin
        .relay
        .client
        .request(parts.method.clone(), url)
        .bearer_auth(&token)
        .body(body);
    if let Some(content_type) = parts.headers.get("content-type") {
        upstream = upstream.header("content-type", content_type);
    }
    let response = match upstream.send().await {
        Ok(response) => response,
        Err(error) => {
            let status = if error.is_timeout() {
                StatusCode::GATEWAY_TIMEOUT
            } else {
                StatusCode::BAD_GATEWAY
            };
            state.admin.logs.push("WARN", "relay", "http-relay", serde_json::json!({"server_id":id,"method":parts.method.as_str(),"path":path,"status":status.as_u16(),"duration_ms":started.elapsed().as_millis()}));
            return Ok((status, "game server is unavailable through relay").into_response());
        }
    };
    if response.status().is_redirection() {
        return Ok((StatusCode::BAD_GATEWAY, "relay refuses upstream redirects").into_response());
    }
    if response
        .content_length()
        .is_some_and(|v| v > MAX_RESPONSE as u64)
    {
        return Ok((StatusCode::BAD_GATEWAY, "upstream response too large").into_response());
    }
    let status = response.status();
    let mut headers = HeaderMap::new();
    for name in ["content-type", "x-request-id"] {
        if let Some(value) = response.headers().get(name) {
            headers.insert(
                if name == "x-request-id" {
                    "x-upstream-request-id"
                } else {
                    "content-type"
                },
                value.clone(),
            );
        }
    }
    headers.insert("cache-control", "no-store".parse().unwrap());
    headers.insert("x-life-paper-transport", "relay".parse().unwrap());
    state.admin.logs.push("INFO", "relay", "http-relay", serde_json::json!({"server_id":id,"method":parts.method.as_str(),"path":path,"status":status.as_u16(),"duration_ms":started.elapsed().as_millis()}));
    let logs = state.admin.logs.clone();
    let stream = response.bytes_stream().scan((0usize, permit, false), move |(received, _permit, finished), chunk| {
        if *finished { return std::future::ready(None); }
        let result = match chunk {
            Ok(chunk) if received.saturating_add(chunk.len()) <= MAX_RESPONSE => { *received += chunk.len(); Ok(chunk) },
            _ => {
                *finished = true;
                logs.push("WARN", "relay", "http-relay", serde_json::json!({"server_id":id,"path":path,"message":"upstream response stream failed or exceeded size limit"}));
                Err(std::io::Error::other("relay response interrupted"))
            },
        };
        std::future::ready(Some(result))
    });
    Ok((status, headers, Body::from_stream(stream)).into_response())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    pub(crate) async fn verify_http_relay(state: AppState) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let upstream = Router::new().fallback(any(|request: Request| async move {
            assert!(request.headers().get("cookie").is_none());
            assert!(request.headers().get("x-admin-secret").is_none());
            assert!(
                request.headers()["authorization"]
                    .to_str()
                    .unwrap()
                    .starts_with("Bearer ")
            );
            if request.uri().path().ends_with("redirect") {
                return (StatusCode::FOUND, [("location", "http://127.0.0.1:1")], "")
                    .into_response();
            }
            let uri = request.uri().to_string();
            let body = to_bytes(request.into_body(), MAX_REQUEST).await.unwrap();
            (
                StatusCode::CREATED,
                [("content-type", "application/octet-stream")],
                format!("{uri}:{}", String::from_utf8_lossy(&body)),
            )
                .into_response()
        }));
        let task = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        state
            .admin
            .servers
            .lock()
            .unwrap()
            .push(crate::administration::GameServer {
                id: "relay-test".into(),
                name: "Relay".into(),
                api_url: format!("http://{address}/api/v1"),
                registration_key_hash: Some("configured".into()),
                last_seen: Some(chrono::Utc::now().timestamp()),
                worlds: vec![],
            });
        let app = routes(state.clone());
        let entry =
            crate::transfer::sign(&state.signing_key, "game-entry", "relay-test", 1, ()).unwrap();
        let admin =
            crate::transfer::sign(&state.signing_key, "game-admin", "relay-test", 1, ()).unwrap();
        let wrong =
            crate::transfer::sign(&state.signing_key, "game-entry", "another", 1, ()).unwrap();
        for (path, token, expected) in [
            ("worlds?since=12", entry.as_str(), StatusCode::CREATED),
            ("management/logs", admin.as_str(), StatusCode::CREATED),
            ("management/logs", entry.as_str(), StatusCode::UNAUTHORIZED),
            ("worlds", admin.as_str(), StatusCode::UNAUTHORIZED),
            ("worlds", wrong.as_str(), StatusCode::UNAUTHORIZED),
            ("worlds", "account-secret", StatusCode::UNAUTHORIZED),
            ("auth/login", entry.as_str(), StatusCode::FORBIDDEN),
            (
                "worlds/%2e%2e/admin",
                entry.as_str(),
                StatusCode::BAD_REQUEST,
            ),
            ("worlds/redirect", entry.as_str(), StatusCode::BAD_GATEWAY),
        ] {
            let response = app
                .clone()
                .oneshot(
                    HttpRequest::builder()
                        .method("PUT")
                        .uri(format!("/api/v1/relay/relay-test/{path}"))
                        .header("authorization", format!("Bearer {token}"))
                        .header("cookie", "central-secret")
                        .header("x-admin-secret", "secret")
                        .header("content-type", "application/octet-stream")
                        .body(Body::from("uploaded-code"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{path}");
            if expected == StatusCode::CREATED {
                assert_eq!(
                    response.headers()["content-type"],
                    "application/octet-stream"
                );
                assert_eq!(response.headers()["x-life-paper-transport"], "relay");
                let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
                assert_eq!(
                    String::from_utf8_lossy(&bytes),
                    format!("/api/v1/{path}:uploaded-code")
                );
            }
        }
        let response = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method("PUT")
                    .uri("/api/v1/relay/relay-test/worlds")
                    .header("authorization", format!("Bearer {entry}"))
                    .header("content-length", MAX_REQUEST + 1)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        state.admin.servers.lock().unwrap()[0].last_seen = None;
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/relay/relay-test/worlds")
                    .header("authorization", format!("Bearer {entry}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        state.admin.servers.lock().unwrap().clear();
        task.abort();
        let _ = task.await;
    }
    #[test]
    fn relay_namespaces_cannot_escape_or_mix_ticket_purposes() {
        assert_eq!(purpose("worlds/abc/organisms").unwrap(), "game-entry");
        assert_eq!(purpose("management/worlds/abc").unwrap(), "game-admin");
        for path in [
            "auth/login",
            "admin/session",
            "relay/other/worlds",
            "worlds/../admin",
            "worlds/%2e%2e/admin",
            "worlds\\admin",
            "worlds//organisms",
        ] {
            assert!(purpose(path).is_err(), "{path}");
        }
    }
}
