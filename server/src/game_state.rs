use crate::{
    api::error::ApiError,
    domain::{ComponentRegistry, ElementRegistry, WorldDirectory},
    game::ai::AiRuntime,
    state::AppState,
    transfer::{self, ModelBundle},
};
use axum::http::HeaderMap;
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

pub struct GameSession {
    pub user_id: i64,
}
pub async fn authenticate(state: &GameState, headers: &HeaderMap) -> Result<GameSession, ApiError> {
    Ok(GameSession {
        user_id: state.authenticate(headers).await?,
    })
}

impl axum::extract::FromRef<AppState> for GameState {
    fn from_ref(state: &AppState) -> Self {
        Self::embedded(state.clone())
    }
}

#[derive(Clone)]
pub enum GameIdentity {
    Embedded(Arc<AppState>),
    Signed {
        server_id: String,
        public_key: Vec<u8>,
    },
    Local {
        token: String,
    },
}

#[derive(Clone)]
pub struct GameState {
    pub identity: GameIdentity,
    pub ai_runtime: Arc<AiRuntime>,
    pub worlds: Arc<WorldDirectory>,
    pub elements: Arc<ElementRegistry>,
    pub components: Arc<ComponentRegistry>,
    pub terrains: Arc<crate::domain::TerrainRegistry>,
    pub organism_instance_storage: Arc<PathBuf>,
    pub organism_model_storage_lock: Arc<tokio::sync::Mutex<()>>,
}

impl GameState {
    pub fn embedded(state: AppState) -> Self {
        Self {
            ai_runtime: state.ai_runtime.clone(),
            worlds: state.worlds.clone(),
            elements: state.elements.clone(),
            components: state.components.clone(),
            terrains: state.terrains.clone(),
            organism_instance_storage: state.organism_instance_storage.clone(),
            organism_model_storage_lock: state.organism_model_storage_lock.clone(),
            identity: GameIdentity::Embedded(Arc::new(state)),
        }
    }

    pub async fn authenticate(&self, headers: &HeaderMap) -> Result<i64, ApiError> {
        if let GameIdentity::Embedded(state) = &self.identity {
            return super::api::auth::authenticate(state, headers)
                .await
                .map(|session| session.user_id);
        }
        let token = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthorized)?;
        match &self.identity {
            GameIdentity::Signed {
                server_id,
                public_key,
            } => {
                if token.len() > 8192 {
                    return Err(ApiError::Unauthorized);
                }
                transfer::verify::<()>(token, public_key, "game-entry", server_id)
                    .map(|claims| claims.user_id)
                    .map_err(|_| ApiError::Unauthorized)
            }
            GameIdentity::Local { token: expected } => {
                use sha2::{Digest, Sha256};
                // Compare fixed-size digests without leaking matching token prefixes.
                let a = Sha256::digest(token.as_bytes());
                let b = Sha256::digest(expected.as_bytes());
                if a.iter()
                    .zip(b.iter())
                    .fold(0u8, |difference, (a, b)| difference | (a ^ b))
                    == 0
                {
                    Ok(1)
                } else {
                    Err(ApiError::Unauthorized)
                }
            }
            GameIdentity::Embedded(_) => unreachable!(),
        }
    }

    pub async fn model(
        &self,
        model_id: Uuid,
        user_id: i64,
        signed: Option<String>,
        local: Option<ModelBundle>,
    ) -> Result<SpawnModel, ApiError> {
        match &self.identity {
            GameIdentity::Embedded(state) => {
                if signed.is_some() || local.is_some() {
                    return Err(ApiError::BadRequest(
                        "model bundles require a standalone game server".into(),
                    ));
                }
                let record = state
                    .organism_models
                    .find_owned(model_id, user_id)
                    .await
                    .map_err(|_| ApiError::Internal)?
                    .ok_or(ApiError::NotFound)?;
                Ok(SpawnModel {
                    id: record.id,
                    name: record.name,
                    composition: record.composition,
                    program: SpawnProgram::Directory(
                        state.organism_model_storage.join(record.id.to_string()),
                    ),
                })
            }
            GameIdentity::Signed {
                server_id,
                public_key,
            } => {
                if local.is_some() {
                    return Err(ApiError::BadRequest(
                        "unsigned model bundles are disabled".into(),
                    ));
                }
                let signed = signed.ok_or_else(|| {
                    ApiError::BadRequest("signed model_bundle is required".into())
                })?;
                let public_key = public_key.clone();
                let server_id = server_id.clone();
                let claims = tokio::task::spawn_blocking(move || {
                    transfer::verify::<ModelBundle>(
                        &signed,
                        &public_key,
                        "model-bundle",
                        &server_id,
                    )
                })
                .await
                .map_err(|_| ApiError::Internal)?
                .map_err(|_| ApiError::Forbidden)?;
                if claims.user_id != user_id || claims.data.id != model_id {
                    return Err(ApiError::Forbidden);
                }
                Ok(SpawnModel::from_bundle(claims.data))
            }
            GameIdentity::Local { .. } => {
                if signed.is_some() {
                    return Err(ApiError::BadRequest(
                        "local mode requires local_model".into(),
                    ));
                }
                let model =
                    local.ok_or_else(|| ApiError::BadRequest("local_model is required".into()))?;
                if model.id != model_id {
                    return Err(ApiError::BadRequest("model ID mismatch".into()));
                }
                Ok(SpawnModel::from_bundle(model))
            }
        }
    }
}

pub struct SpawnModel {
    pub id: Uuid,
    pub name: String,
    pub composition: serde_json::Value,
    pub program: SpawnProgram,
}

impl SpawnModel {
    fn from_bundle(bundle: ModelBundle) -> Self {
        Self {
            id: bundle.id,
            name: bundle.name.clone(),
            composition: bundle.composition.clone(),
            program: SpawnProgram::Bundle(bundle),
        }
    }
}

pub enum SpawnProgram {
    Directory(PathBuf),
    Bundle(ModelBundle),
}
impl SpawnProgram {
    pub fn write(&self, target: &std::path::Path) -> Result<(), ApiError> {
        match self {
            Self::Directory(source) => crate::domain::organism::program_storage::clone_program(
                source, target,
            )
            .map_err(|error| {
                tracing::error!(%error, "failed to clone instance program");
                if error.kind() == std::io::ErrorKind::InvalidInput {
                    ApiError::BadRequest(error.to_string())
                } else {
                    ApiError::Internal
                }
            }),
            Self::Bundle(bundle) => bundle
                .write_program(target)
                .map_err(|error| ApiError::BadRequest(error.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as BASE64};
    use ring::signature::KeyPair;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    async fn request(
        app: &Router,
        method: &str,
        path: &str,
        token: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 110 * 1024 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn downloaded_tutorial_runs_locally_with_saved_events_and_script() {
        use crate::domain::world::AiCommit;
        use crate::game::ai::AiContext;
        let root = std::env::temp_dir().join(format!("life-tutorial-import-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("instances")).unwrap();
        let terrains = Arc::new(crate::domain::create_default_terrain_registry().unwrap());
        let components = Arc::new(crate::domain::create_default_component_registry().unwrap());
        let worlds = Arc::new(WorldDirectory::new_with_storage(terrains.clone(), root.join("worlds")).unwrap());
        let state = GameState {
            identity: GameIdentity::Local { token: "local-only-test".into() }, worlds: worlds.clone(), terrains: terrains.clone(), components: components.clone(),
            elements: Arc::new(crate::domain::create_default_element_registry().unwrap()),
            ai_runtime: Arc::new(AiRuntime::new(crate::config::AiConfig { enabled: true, ..Default::default() }).unwrap()),
            organism_instance_storage: Arc::new(root.join("instances")),
            organism_model_storage_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        let app = crate::api::game_router(state.clone());
        let pack = serde_json::to_value(crate::tutorial::package("matter", &terrains, &components).unwrap()).unwrap();
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/import", "local-only-test", pack).await.0, StatusCode::CONFLICT);
        let mut pack = serde_json::to_value(crate::tutorial::package("welcome", &terrains, &components).unwrap()).unwrap();
        // A downloaded definition can differ from the engine's compiled catalog.
        pack["definition"]["id"] = json!("downloaded-first");
        pack["definition"]["map"]["level_id"] = json!("downloaded-map");
        pack["world"]["ground"]["seed"] = json!(999);
        let mut invalid = pack.clone(); invalid["version"] = json!(2);
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/import", "local-only-test", invalid).await.0, StatusCode::BAD_REQUEST);
        let mut invalid = pack.clone(); invalid["definition"]["map"]["size_px"] = json!(u64::MAX);
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/import", "local-only-test", invalid).await.0, StatusCode::BAD_REQUEST);
        assert!(worlds.list_all().is_empty());
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/import", "central-token", pack.clone()).await.0, StatusCode::UNAUTHORIZED);
        let (status, result) = request(&app, "POST", "/api/v1/worlds/tutorial/import", "local-only-test", pack.clone()).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        let world_id = result["data"]["id"].as_str().unwrap();
        let id = Uuid::parse_str(world_id).unwrap();
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/import", "local-only-test", pack.clone()).await.1["data"]["id"], world_id);
        let progress_path = format!("/api/v1/worlds/{world_id}/tutorial");
        let progress = request(&app, "GET", &progress_path, "local-only-test", Value::Null).await.1;
        assert_eq!(progress["data"]["id"], "downloaded-first");
        assert_eq!(progress["data"]["completed"], false);
        let model = &progress["data"]["models"][0];
        let body = model["composition"]["root_body"].as_str().unwrap();
        let (status, instance) = request(&app, "POST", &format!("/api/v1/worlds/{world_id}/organisms"), "local-only-test", json!({
            "model_id": model["id"], "local_model": model,
            "initial_contents": { (body): { "non_gas_contents_mg": {"water": 100.0} } }
        })).await;
        assert_eq!(status, StatusCode::CREATED, "{instance}");
        let organism_id = Uuid::parse_str(instance["data"]["id"].as_str().unwrap()).unwrap();
        let snapshot = worlds.read_world(id, 1).unwrap();
        assert_eq!(snapshot.ground_config().unwrap().seed, 999);
        let organism = &snapshot.organisms[&organism_id];
        let metadata = worlds.get(id).unwrap();
        let output = state.ai_runtime.run_scheduled(&root.join("instances").join(organism_id.to_string()), state.organism_model_storage_lock.clone(),
            AiContext::new(&metadata, snapshot.clone(), organism, state.elements.clone())).await.unwrap();
        assert!(output.report.ok, "{:?}", output.report);
        assert!(!output.report.commands.is_empty());
        assert!(worlds.commit_tick(id, &snapshot, vec![AiCommit { organism_id, expected_calls: 0, report: output.report, memory: output.memory }]).unwrap());
        let updated = worlds.read_world(id, 1).unwrap();
        assert!(!updated.matter.ejecta.is_empty());
        assert!(updated.tutorial.as_ref().unwrap().completed);
        let restored = WorldDirectory::new_with_storage(terrains.clone(), root.join("worlds")).unwrap();
        assert!(restored.read_world(id, 1).unwrap().tutorial.as_ref().unwrap().completed);
        let key = crate::transfer::tests::key();
        let signed_state = GameState { identity: GameIdentity::Signed { server_id: "remote".into(), public_key: key.public_key().as_ref().into() }, ..state.clone() };
        let token = transfer::sign(&key, "game-entry", "remote", 1, ()).unwrap();
        assert_eq!(request(&crate::api::game_router(signed_state), "POST", "/api/v1/worlds/tutorial/import", &token, pack).await.0, StatusCode::FORBIDDEN);
        state.ai_runtime.shutdown().await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn signed_game_runs_without_databases_and_imports_only_authorized_models() {
        let root = std::env::temp_dir().join(format!("life-game-split-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("instances")).unwrap();
        let key = crate::transfer::tests::key();
        let terrains = Arc::new(crate::domain::create_default_terrain_registry().unwrap());
        let components = Arc::new(crate::domain::create_default_component_registry().unwrap());
        let worlds = Arc::new(
            WorldDirectory::new_with_storage(terrains.clone(), root.join("worlds")).unwrap(),
        );
        let state = GameState {
            identity: GameIdentity::Signed {
                server_id: "game-1".into(),
                public_key: key.public_key().as_ref().into(),
            },
            worlds: worlds.clone(),
            terrains: terrains.clone(),
            components: components.clone(),
            elements: Arc::new(crate::domain::create_default_element_registry().unwrap()),
            ai_runtime: Arc::new(AiRuntime::new(crate::config::AiConfig::default()).unwrap()),
            organism_instance_storage: Arc::new(root.join("instances")),
            organism_model_storage_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        let logs = crate::observability::LogStore::open(root.join("logs")).unwrap();
        let app = crate::api::game_router(state.clone())
            .merge(crate::administration::game_routes(
                state.clone(),
                logs.clone(),
            ))
            .layer(axum::middleware::from_fn_with_state(
                logs.clone(),
                crate::observability::network,
            ));
        let token = transfer::sign(&key, "game-entry", "game-1", 7, ()).unwrap();
        let wrong_server = transfer::sign(&key, "game-entry", "game-2", 7, ()).unwrap();
        assert_eq!(
            request(&app, "GET", "/api/v1/worlds", &wrong_server, Value::Null)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(
                &app,
                "GET",
                "/api/v1/worlds",
                "central-login-token",
                Value::Null
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(&app, "POST", "/api/v1/auth/login", &token, json!({}))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        let (status, created) = request(
            &app,
            "POST",
            "/api/v1/worlds",
            &token,
            json!({"level_id": "split", "name": "split"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let world_id = created["data"]["id"].as_str().unwrap();
        let mut model =
            crate::domain::BiologicalOrganism::new(crate::domain::OrganismProgramId::new());
        let body = model.add_component("small_body_1", &components).unwrap();
        let bundle = ModelBundle {
            id: model.id,
            name: "imported".into(),
            composition: serde_json::to_value(&model).unwrap(),
            files: [
                (
                    "main.py".into(),
                    BASE64.encode(b"def main(game):\n    pass\n"),
                ),
                ("lib/asset.bin".into(), BASE64.encode([0, 255, 128])),
            ]
            .into(),
            directories: vec![],
        };
        let path = format!("/api/v1/worlds/{world_id}/organisms");
        for (audience, owner) in [("game-2", 7), ("game-1", 8)] {
            let signed = transfer::sign(&key, "model-bundle", audience, owner, &bundle).unwrap();
            assert_eq!(
                request(
                    &app,
                    "POST",
                    &path,
                    &token,
                    json!({"model_id": model.id, "model_bundle": signed})
                )
                .await
                .0,
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            request(
                &app,
                "POST",
                &path,
                &token,
                json!({"model_id": model.id, "local_model": bundle})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let signed = transfer::sign(&key, "model-bundle", "game-1", 7, &bundle).unwrap();
        let (status, instance) = request(&app, "POST", &path, &token, json!({
            "model_id": model.id, "model_bundle": signed,
            "initial_contents": { (body.to_string()): { "non_gas_contents_mg": {"water": 100.0} } }
        })).await;
        assert_eq!(status, StatusCode::CREATED, "{instance}");
        assert_eq!(instance["data"]["owner_user_id"], 7);
        let id = instance["data"]["id"].as_str().unwrap();
        assert_eq!(
            std::fs::read(root.join("instances").join(id).join("lib/asset.bin")).unwrap(),
            [0, 255, 128]
        );
        let files = format!("{path}/{id}/files?path=main.py");
        let (status, _) = request(&app, "GET", &files, &token, Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = request(
            &app,
            "PUT",
            &format!("{path}/{id}/files?path=helpers/test.py"),
            &token,
            json!("test"),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(
            request(
                &app,
                "DELETE",
                &format!("{path}/{id}/entries?path=main.py"),
                &token,
                Value::Null
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let other = transfer::sign(&key, "game-entry", "game-1", 8, ()).unwrap();
        assert_eq!(
            request(&app, "GET", &path, &other, Value::Null).await.0,
            StatusCode::FORBIDDEN
        );
        let restored = WorldDirectory::new_with_storage(terrains, root.join("worlds")).unwrap();
        assert_eq!(
            restored
                .read_world(Uuid::parse_str(world_id).unwrap(), 7)
                .unwrap()
                .organisms
                .len(),
            1
        );
        assert_eq!(
            request(
                &app,
                "POST",
                &format!("/api/v1/worlds/{world_id}/resume"),
                &token,
                Value::Null
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(
                &app,
                "POST",
                &format!("/api/v1/worlds/{world_id}/pause"),
                &token,
                Value::Null
            )
            .await
            .0,
            StatusCode::OK
        );
        let management_token = transfer::sign(&key, "game-admin", "game-1", 1, ()).unwrap();
        assert_eq!(
            request(&app, "GET", "/api/v1/management/logs", &token, Value::Null)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(
                &app,
                "GET",
                "/api/v1/worlds",
                &management_token,
                Value::Null
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, events) = request(
            &app,
            "GET",
            "/api/v1/management/logs?category=network&secret=excluded",
            &management_token,
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let encoded = events.to_string();
        assert!(!encoded.contains(&token));
        assert!(!encoded.contains("excluded"));
        assert!(!events["data"]["records"].as_array().unwrap().is_empty());
        assert_eq!(
            request(
                &app,
                "GET",
                "/api/v1/management/worlds",
                &management_token,
                Value::Null
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(
                &app,
                "GET",
                &format!("/api/v1/management/worlds/{world_id}"),
                &management_token,
                Value::Null
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(
                &app,
                "PATCH",
                &format!("/api/v1/management/worlds/{world_id}"),
                &management_token,
                json!({"status":"paused"})
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            request(
                &app,
                "DELETE",
                &format!("/api/v1/management/worlds/{world_id}"),
                &management_token,
                Value::Null
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
        assert!(worlds.get(Uuid::parse_str(world_id).unwrap()).is_none());
        assert!(!root.join("instances").join(id).exists());
        assert!(
            WorldDirectory::new_with_storage(state.terrains.clone(), root.join("worlds"))
                .unwrap()
                .list_all()
                .is_empty()
        );
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/matter", &token, Value::Null).await.0, StatusCode::CONFLICT);
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/missing", &token, Value::Null).await.0, StatusCode::NOT_FOUND);
        let (status, tutorial) = request(&app, "POST", "/api/v1/worlds/tutorial/welcome", &token, Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        let tutorial_id = tutorial["data"]["id"].as_str().unwrap();
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/welcome", &token, Value::Null).await.1["data"]["id"], tutorial_id);
        let state_path = format!("/api/v1/worlds/{tutorial_id}/tutorial");
        let progress = request(&app, "GET", &state_path, &token, Value::Null).await.1;
        assert_eq!(progress["data"]["completed"], false);
        assert_eq!(progress["data"]["panels"][0]["title"], "欢迎来到生命实验场");
        let another_user = transfer::sign(&key, "game-entry", "game-1", 8, ()).unwrap();
        assert_eq!(request(&app, "GET", &state_path, &another_user, Value::Null).await.0, StatusCode::FORBIDDEN);
        let signed = transfer::sign(&key, "model-bundle", "game-1", 7, &bundle).unwrap();
        assert_eq!(request(&app, "POST", &format!("/api/v1/worlds/{tutorial_id}/organisms"), &token,
            json!({"model_id": model.id, "model_bundle": signed})).await.0, StatusCode::CREATED);
        assert_eq!(request(&app, "GET", &state_path, &token, Value::Null).await.1["data"]["completed"], true);
        let restored = WorldDirectory::new_with_storage(state.terrains.clone(), root.join("worlds")).unwrap();
        assert!(restored.read_world(Uuid::parse_str(tutorial_id).unwrap(), 7).unwrap().tutorial.as_ref().unwrap().completed);
        assert_eq!(request(&app, "POST", "/api/v1/worlds/tutorial/matter", &token, Value::Null).await.0, StatusCode::OK);
        drop(app);
        drop(logs);
        std::fs::remove_dir_all(root).unwrap();
    }
}
