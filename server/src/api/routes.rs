use super::auth;
use crate::state::AppState;
use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;

pub fn router(state: AppState, central_only: bool) -> Router {
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
        .nest("/api/v1", super::organism_model::routes())
        .nest("/api/v1", super::player::routes())
        .with_state(state.clone());
    let app = if central_only {
        app
    } else {
        app.merge(game_router(crate::game_state::GameState::embedded(state)))
    };
    app.merge(crate::administration::routes(state_for_admin))
        .merge(relay)
        .layer(CorsLayer::permissive())
}

pub fn game_router(state: crate::game_state::GameState) -> Router {
    Router::new()
        .nest(
            "/api/v1",
            super::world::routes().merge(super::organism_model::instance_routes()),
        )
        .layer(axum::extract::DefaultBodyLimit::max(100 * 1024 * 1024))
        .with_state(state)
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
