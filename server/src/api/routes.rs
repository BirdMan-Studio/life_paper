use super::auth;
use crate::state::AppState;
use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;

pub fn router(state: AppState, central_only: bool) -> Router {
    let body_guard = BodyGuard::new(crate::game_state::GameState::embedded(state.clone()));
    let state_for_admin = state.clone();
    let relay = crate::relay::routes(state.clone());
    let app = Router::new()
        .route("/", get(root))
        .route("/api/v1/python-api", get(super::python_docs::catalog))
        .nest("/api/v1", auth::routes())
        .nest("/api/v1", super::transfer::routes())
        .nest("/api/v1", crate::game_registry::routes())
        .nest("/api/v1", crate::tutorial::routes())
        .nest("/api/v1", super::organism_component::routes())
        .nest(
            "/api/v1",
            super::organism_model::routes().route_layer(axum::middleware::from_fn_with_state(
                body_guard.clone(),
                guard_body,
            )),
        )
        .nest("/api/v1", super::player::routes())
        .with_state(state.clone());
    let app = if central_only {
        app
    } else {
        app.merge(game_router_guarded(
            crate::game_state::GameState::embedded(state),
            body_guard,
        ))
    };
    app.merge(crate::administration::routes(state_for_admin))
        .merge(relay)
        .layer(CorsLayer::permissive())
}

pub fn game_router(state: crate::game_state::GameState) -> Router {
    game_router_guarded(state.clone(), BodyGuard::new(state))
}

fn game_router_guarded(state: crate::game_state::GameState, guard: BodyGuard) -> Router {
    Router::new()
        .nest(
            "/api/v1",
            super::world::routes().merge(super::organism_model::instance_routes()),
        )
        .layer(axum::extract::DefaultBodyLimit::max(
            crate::transfer::MAX_TRANSFER_REQUEST_BYTES,
        ))
        .route_layer(axum::middleware::from_fn_with_state(guard, guard_body))
        .with_state(state)
}

#[derive(Clone)]
struct BodyGuard {
    game: crate::game_state::GameState,
    slots: std::sync::Arc<tokio::sync::Semaphore>,
}

impl BodyGuard {
    fn new(game: crate::game_state::GameState) -> Self {
        Self {
            game,
            slots: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }
}

async fn guard_body(
    axum::extract::State(guard): axum::extract::State<BodyGuard>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, super::error::ApiError> {
    use axum::{body::HttpBody, http::StatusCode, response::IntoResponse};
    guard.game.authenticate(request.headers()).await?;
    // Unknown lengths (including chunked uploads) must reserve a slot too.
    let large_body = request
        .body()
        .size_hint()
        .upper()
        .is_none_or(|size| size > 2 * 1024 * 1024);
    let _permit = if !large_body {
        None
    } else {
        match guard.slots.try_acquire() {
            Ok(permit) => Some(permit),
            Err(_) => {
                return Ok((
                    StatusCode::SERVICE_UNAVAILABLE,
                    [("Retry-After", "1")],
                    axum::Json(serde_json::json!({
                        "error": {
                            "code": "REQUEST_BUSY",
                            "message": "Server is processing another large request; retry shortly"
                        }
                    })),
                )
                    .into_response());
            }
        }
    };
    Ok(next.run(request).await)
}

async fn root() -> &'static str {
    "hello"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn body_admission_authenticates_before_polling_and_releases_on_cancel() {
        use crate::game_state::{GameIdentity, GameState};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let terrains = Arc::new(crate::domain::create_default_terrain_registry().unwrap());
        let state = GameState {
            identity: GameIdentity::Local {
                token: "body-test".into(),
            },
            worlds: Arc::new(crate::domain::WorldDirectory::new(terrains.clone())),
            terrains,
            components: Arc::new(crate::domain::create_default_component_registry().unwrap()),
            elements: Arc::new(crate::domain::create_default_element_registry().unwrap()),
            ai_runtime: Arc::new(
                crate::game::ai::AiRuntime::new(crate::config::AiConfig {
                    enabled: false,
                    ..Default::default()
                })
                .unwrap(),
            ),
            organism_instance_storage: Arc::new(std::path::PathBuf::new()),
            organism_model_storage_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        let guard = BodyGuard::new(state.clone());
        let app = game_router_guarded(state, guard.clone());
        let polls = Arc::new(AtomicUsize::new(0));
        let request = |token: &str| {
            let polls = polls.clone();
            let body = Body::from_stream(futures_util::stream::poll_fn(move |_| {
                polls.fetch_add(1, Ordering::SeqCst);
                std::task::Poll::<Option<Result<axum::body::Bytes, std::io::Error>>>::Pending
            }));
            Request::builder()
                .method("POST")
                .uri("/api/v1/worlds")
                .header("Authorization", token)
                .header("Content-Type", "application/json")
                .header(
                    "Content-Length",
                    crate::transfer::MAX_TRANSFER_REQUEST_BYTES,
                )
                .body(body)
                .unwrap()
        };
        for token in ["", "Bearer incorrect"] {
            assert_eq!(
                app.clone().oneshot(request(token)).await.unwrap().status(),
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        let permit = guard.slots.acquire().await.unwrap();
        let response = app
            .clone()
            .oneshot(request("Bearer body-test"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()["Retry-After"], "1");
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        for (body, expected) in [
            (
                Body::from(vec![0u8; 2 * 1024 * 1024 + 1]),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (Body::from("invalid json"), StatusCode::BAD_REQUEST),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/worlds")
                        .header("Authorization", "Bearer body-test")
                        .header("Content-Type", "application/json")
                        .body(body)
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/worlds")
                        .header("Authorization", "Bearer body-test")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        drop(permit);
        let pending = tokio::spawn(app.clone().oneshot(request("Bearer body-test")));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while polls.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        assert_eq!(guard.slots.available_permits(), 1);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/worlds")
                    .header("Authorization", "Bearer body-test")
                    .header("Content-Type", "application/json")
                    .body(Body::from("invalid json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(guard.slots.available_permits(), 1);
    }

    #[tokio::test]
    async fn central_routes_exclude_worlds_while_combined_routes_keep_them() {
        let pg = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap();
        let terrains = Arc::new(crate::domain::create_default_terrain_registry().unwrap());
        let temporary =
            std::env::temp_dir().join(format!("life-admin-routes-{}", uuid::Uuid::new_v4()));
        let state = AppState {
            admin: Arc::new(
                crate::administration::AdminService::new(
                    pg.clone(),
                    crate::observability::LogStore::open(temporary.join("logs")).unwrap(),
                    temporary.clone(),
                    temporary.join("dist"),
                    Some("admin-test-secret-01234567890123456789".into()),
                )
                .unwrap(),
            ),
            central_only: true,
            signing_key: Arc::new(crate::transfer::tests::key()),
            ai_runtime: Arc::new(
                crate::game::ai::AiRuntime::new(crate::config::AiConfig::default()).unwrap(),
            ),
            users: crate::db::UserRepository::new(pg.clone()),
            component_unlocks: crate::db::ComponentUnlockRepository::new(pg.clone()),
            player_data: crate::db::PlayerDataRepository::new(pg.clone()),
            organism_models: crate::db::OrganismModelRepository::new(pg),
            organism_model_storage: Arc::new(std::path::PathBuf::new()),
            organism_instance_storage: Arc::new(std::path::PathBuf::new()),
            organism_model_storage_lock: Arc::new(tokio::sync::Mutex::new(())),
            redis_pool: crate::db::redis_pool::create_pool(&crate::config::RedisConfig {
                host: "127.0.0.1".into(),
                port: 1,
                password: "".into(),
                database: 0,
            })
            .unwrap(),
            elements: Arc::new(crate::domain::create_default_element_registry().unwrap()),
            components: Arc::new(crate::domain::create_default_component_registry().unwrap()),
            worlds: Arc::new(crate::domain::WorldDirectory::new(terrains.clone())),
            terrains,
        };
        crate::relay::tests::verify_http_relay(state.clone()).await;
        for central_only in [true, false] {
            let app = router(state.clone(), central_only);
            let unread = Body::from_stream(futures_util::stream::poll_fn(|_| {
                panic!("unauthenticated model upload polled the body");
                #[allow(unreachable_code)]
                std::task::Poll::<Option<Result<axum::body::Bytes, std::io::Error>>>::Pending
            }));
            assert_eq!(
                app.clone()
                    .oneshot(
                        Request::builder()
                            .method("PUT")
                            .uri(format!(
                                "/api/v1/organism-models/{}/files?path=weights.bin",
                                uuid::Uuid::new_v4()
                            ))
                            .body(unread)
                            .unwrap()
                    )
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/worlds")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if central_only {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::UNAUTHORIZED
                }
            );
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/auth/game-ticket")
                        .header("Content-Type", "application/json")
                        .body(Body::from(r#"{"game_server_id":"game-1"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/auth/game-public-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap())
                    .unwrap();
            assert_eq!(
                body["data"]["public_key"],
                crate::transfer::public_key(&state.signing_key)
            );
        }
        let app = router(state.clone(), true);
        for path in [
            "/api/v1/admin/session",
            "/api/v1/admin/logs",
            "/api/v1/admin/records/users",
            "/api/v1/admin/servers",
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        for (method, path, body) in [
            (
                "POST",
                "/api/v1/admin/servers",
                r#"{"id":"remote-1","name":"Remote","api_url":"http://127.0.0.1:8081/api/v1"}"#,
            ),
            ("GET", "/api/v1/admin/servers", ""),
            ("POST", "/api/v1/admin/servers/remote-1/ticket", ""),
            (
                "PUT",
                "/api/v1/admin/servers/remote-1",
                r#"{"id":"remote-1","name":"Renamed","api_url":"http://127.0.0.1:8082/api/v1"}"#,
            ),
            ("DELETE", "/api/v1/admin/servers/remote-1", ""),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header(
                            "Authorization",
                            "Bearer admin-test-secret-01234567890123456789",
                        )
                        .header("Content-Type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{method} {path}");
            let value: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap())
                    .unwrap();
            if method == "POST" && path.ends_with("/servers") {
                let key = value["data"]["registration_key"].as_str().unwrap();
                assert!(state.admin.connected_server("remote-1").is_err());
                let persisted =
                    std::fs::read_to_string(temporary.join("game-servers.json")).unwrap();
                assert!(!persisted.contains(key));
                for (credential, expected) in [
                    ("wrong-key", StatusCode::UNAUTHORIZED),
                    (key, StatusCode::OK),
                ] {
                    let response = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/game-servers/register").header("Authorization",format!("Bearer {credential}")).header("Content-Type","application/json").body(Body::from(r#"{"server_id":"remote-1","api_url":"http://127.0.0.1:8081/api/v1","worlds":[]}"#)).unwrap()).await.unwrap();
                    assert_eq!(response.status(), expected);
                }
                assert!(state.admin.connected_server("remote-1").is_ok());
            }
            if method == "GET" && path.ends_with("/servers") {
                assert_eq!(value["data"][0]["online"], true);
                assert!(value["data"][0].get("registration_key_hash").is_none());
            }
            if path.ends_with("ticket") {
                let token = value["data"]["token"].as_str().unwrap();
                let key = crate::transfer::decode_public_key(&crate::transfer::public_key(
                    &state.signing_key,
                ))
                .unwrap();
                assert!(
                    crate::transfer::verify::<()>(token, &key, "game-admin", "remote-1").is_ok()
                );
                assert!(
                    crate::transfer::verify::<()>(token, &key, "game-entry", "remote-1").is_err()
                );
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/api/v1/admin/servers/remote-1/registration-key")
                            .header(
                                "Authorization",
                                "Bearer admin-test-secret-01234567890123456789",
                            )
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert!(state.admin.connected_server("remote-1").is_err());
            }
        }
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(temporary.join("game-servers.json")).unwrap()
            )
            .unwrap(),
            serde_json::json!([])
        );
        drop(app);
        std::fs::create_dir_all(temporary.join("dist/admin")).unwrap();
        std::fs::create_dir_all(temporary.join("dist/assets")).unwrap();
        std::fs::write(
            temporary.join("dist/admin/index.html"),
            "<!doctype html><title>Admin</title>",
        )
        .unwrap();
        std::fs::write(temporary.join("dist/assets/test.js"), "export default 1").unwrap();
        let app = router(state.clone(), true);
        for (path, mime) in [
            ("/admin/", "text/html; charset=utf-8"),
            ("/assets/test.js", "text/javascript"),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["content-type"], mime);
        }
        drop(app);
        drop(state);
        std::fs::remove_dir_all(temporary).unwrap();
    }
}
