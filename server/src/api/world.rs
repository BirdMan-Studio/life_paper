use super::error::ApiError;
use crate::domain::{WorldDirectoryError, WorldInstance, WorldKind, WorldStatus};
use crate::game_state::{GameState as AppState, authenticate};
use axum::{
    Json, Router,
    extract::State,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/worlds", get(list_worlds).post(create_player_world))
        .route("/worlds/{world_id}/ground", get(get_ground))
        .route("/worlds/{world_id}/tutorial", get(get_tutorial_state))
        .route("/worlds/{world_id}/matter", get(get_matter))
        .route("/worlds/{world_id}/matter/history", get(get_matter_history))
        .route("/worlds/{world_id}/organisms/{organism_id}/ai", get(get_ai))
        .route(
            "/worlds/{world_id}/organisms",
            get(list_organisms).post(create_organism),
        )
        .route("/worlds/tutorial", post(get_or_create_tutorial))
        .route("/worlds/tutorial/import", post(import_tutorial))
        .route("/worlds/tutorial/{tutorial_id}", post(get_or_create_named_tutorial))
        .route("/worlds/{world_id}/pause", post(pause_world))
        .route("/worlds/{world_id}/resume", post(resume_world))
}

#[derive(Serialize)]
struct DataResponse<T> {
    data: T,
}

async fn import_tutorial(
    State(state): State<AppState>, headers: HeaderMap,
    Json(mut package): Json<crate::tutorial::TutorialPackage>,
) -> Result<Json<DataResponse<WorldSummary>>, ApiError> {
    let user_id = authenticate(&state, &headers).await?.user_id;
    if !matches!(state.identity, crate::game_state::GameIdentity::Local { .. }) { return Err(ApiError::Forbidden); }
    if package.version != 1 { return Err(ApiError::BadRequest("unsupported tutorial package version".into())); }
    crate::tutorial::validate_definition(&package.definition).map_err(|message| ApiError::BadRequest(message.into()))?;
    if !package.world.organisms.is_empty() { return Err(ApiError::BadRequest("tutorial package must not contain player instances".into())); }
    let world = tokio::task::spawn_blocking(move || {
        let level_id = package.definition["map"]["level_id"].as_str().ok_or(ApiError::Internal)?.to_owned();
        let name = package.definition["title"].as_str().ok_or(ApiError::Internal)?.to_owned();
        let existing = state.worlds.list_for_user(user_id);
        if existing.iter().any(|world| world.is_tutorial() && world.level_id == level_id) {
            return state.worlds.import_tutorial_world(user_id, &level_id, &name, package.world).map_err(map_world_error);
        }
        for prerequisite in package.definition["prerequisites"].as_array().into_iter().flatten() {
            if !existing.iter().any(|world| state.worlds.read_world(world.id, user_id).ok().is_some_and(|world|
                world.tutorial.as_ref().is_some_and(|progress| progress.definition["id"] == *prerequisite && progress.completed))) {
                return Err(ApiError::Conflict("请先通关前置教程".into()));
            }
        }
        package.world.tutorial = Some(crate::tutorial::TutorialRuntime::new(package.definition));
        state.worlds.import_tutorial_world(user_id, &level_id, &name, package.world).map_err(map_world_error)
    }).await.map_err(|_| ApiError::Internal)??;
    Ok(Json(DataResponse { data: WorldSummary::from(world.as_ref()) }))
}

async fn get_tutorial_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<Uuid>,
) -> Result<Json<DataResponse<serde_json::Value>>, ApiError> {
    let user_id = authenticate(&state, &headers).await?.user_id;
    let data = tokio::task::spawn_blocking(move || {
        let world = state.worlds.read_world(world_id, user_id).map_err(map_world_error)?;
        let tutorial = world.tutorial.as_ref().ok_or(ApiError::NotFound)?;
        Ok::<_, ApiError>(tutorial.view())
    }).await.map_err(|_| ApiError::Internal)??;
    Ok(Json(DataResponse { data }))
}

#[derive(Deserialize)]
struct CreateWorldRequest {
    level_id: String,
    name: Option<String>,
    #[serde(default)]
    ai_full_visibility: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MatterQuery {
    x: i32,
    y: i32,
    radius_px: u32,
    since: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MatterHistoryQuery {
    #[serde(default)]
    after: u64,
    #[serde(default = "default_history_limit")]
    limit: u32,
}

fn default_history_limit() -> u32 {
    100
}

async fn get_matter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<Uuid>,
    Query(query): Query<MatterQuery>,
) -> Result<Json<DataResponse<crate::domain::world::MatterSync>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let data = tokio::task::spawn_blocking(move || {
        let world = state
            .worlds
            .read_world(world_id, session.user_id)
            .map_err(map_world_error)?;
        world
            .matter
            .sync(
                crate::domain::MapPosition {
                    x: query.x,
                    y: query.y,
                },
                query.radius_px,
                query.since,
            )
            .map_err(|message| ApiError::BadRequest(message.into()))
    })
    .await
    .map_err(|_| ApiError::Internal)??;
    Ok(Json(DataResponse { data }))
}

async fn get_matter_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<Uuid>,
    Query(query): Query<MatterHistoryQuery>,
) -> Result<Json<DataResponse<crate::domain::world::MatterHistory>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let data = tokio::task::spawn_blocking(move || {
        let world = state
            .worlds
            .read_world(world_id, session.user_id)
            .map_err(map_world_error)?;
        world
            .matter
            .history(query.after, query.limit)
            .map_err(|message| ApiError::BadRequest(message.into()))
    })
    .await
    .map_err(|_| ApiError::Internal)??;
    Ok(Json(DataResponse { data }))
}

async fn owned_organism(
    state: &AppState,
    world_id: Uuid,
    organism_id: Uuid,
    user_id: i64,
) -> Result<
    (
        std::sync::Arc<crate::domain::World>,
        crate::domain::organism::OrganismInstance,
    ),
    ApiError,
> {
    let worlds = state.worlds.clone();
    tokio::task::spawn_blocking(move || {
        let world = worlds
            .read_world(world_id, user_id)
            .map_err(map_world_error)?;
        let organism = world
            .organisms
            .get(&organism_id)
            .filter(|value| value.owner_user_id == user_id)
            .cloned()
            .ok_or(ApiError::NotFound)?;
        Ok((world, organism))
    })
    .await
    .map_err(|_| ApiError::Internal)?
}

async fn get_ai(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((world_id, organism_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DataResponse<serde_json::Value>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let (_, organism) = owned_organism(&state, world_id, organism_id, session.user_id).await?;
    Ok(Json(DataResponse {
        data: serde_json::json!({
            "enabled": state.ai_runtime.config.enabled, "entry_file": "main.py", "entry_function": "main(game)",
            "timeout_ms": state.ai_runtime.config.timeout_ms, "max_queries": state.ai_runtime.config.max_queries,
            "workers": state.ai_runtime.config.workers, "cache_entries": state.ai_runtime.config.cache_entries,
            "state": organism.ai,
        }),
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateOrganismRequest {
    model_id: Uuid,
    model_bundle: Option<String>,
    local_model: Option<crate::transfer::ModelBundle>,
    #[serde(default)]
    initial_contents:
        std::collections::BTreeMap<Uuid, crate::domain::organism::InitialContainerContents>,
}

async fn list_organisms(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<Uuid>,
) -> Result<Json<DataResponse<Vec<crate::domain::organism::OrganismInstance>>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let worlds = state.worlds.clone();
    let data =
        tokio::task::spawn_blocking(move || worlds.list_organisms(world_id, session.user_id))
            .await
            .map_err(|_| ApiError::Internal)?
            .map_err(map_world_error)?;
    Ok(Json(DataResponse { data }))
}

async fn create_organism(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<Uuid>,
    Json(request): Json<CreateOrganismRequest>,
) -> Result<
    (
        StatusCode,
        Json<DataResponse<crate::domain::organism::OrganismInstance>>,
    ),
    ApiError,
> {
    let session = authenticate(&state, &headers).await?;
    let world = state.worlds.get(world_id).ok_or(ApiError::NotFound)?;
    if world.owner_user_id != session.user_id {
        return Err(ApiError::Forbidden);
    }
    let storage_guard = state.organism_model_storage_lock.clone().lock_owned().await;
    let record = state
        .model(
            request.model_id,
            session.user_id,
            request.model_bundle,
            request.local_model,
        )
        .await?;
    let composition = serde_json::from_value(record.composition).map_err(|error| {
        tracing::error!(target: "api", %error, "failed to decode organism model");
        ApiError::Internal
    })?;
    let mut organism = crate::domain::organism::OrganismInstance::from_model(
        record.id,
        session.user_id,
        record.name,
        composition,
        &state.components,
    )
    .map_err(ApiError::BadRequest)?;
    organism
        .initialize_containers(request.initial_contents, &state.elements)
        .map_err(ApiError::BadRequest)?;
    let worlds = state.worlds.clone();
    let program = record.program;
    let quota_roots = super::storage_quota::user_roots(&state, session.user_id).await?;
    let target = state
        .organism_instance_storage
        .join(organism.id.to_string());
    let result = tokio::task::spawn_blocking(move || {
        let _guard = storage_guard;
        super::storage_quota::check_copy(&quota_roots, &program)?;
        worlds
            .place_organism(world_id, &mut organism)
            .map_err(map_world_error)?;
        program.write(&target)?;
        let result = organism.clone();
        if let Err(error) = worlds.add_organism(world_id, organism) {
            let _ = std::fs::remove_dir_all(&target);
            return Err(map_world_error(error));
        }
        Ok::<_, ApiError>(result)
    })
    .await
    .map_err(|_| ApiError::Internal)??;
    Ok((StatusCode::CREATED, Json(DataResponse { data: result })))
}

#[derive(Serialize)]
struct WorldSummary {
    id: Uuid,
    kind: WorldKind,
    name: String,
    owner_user_id: i64,
    level_id: String,
    status: WorldStatus,
    tutorial_completed: bool,
}

impl From<&WorldInstance> for WorldSummary {
    fn from(world: &WorldInstance) -> Self {
        Self {
            id: world.id,
            kind: world.kind,
            name: world.name.clone(),
            owner_user_id: world.owner_user_id,
            level_id: world.level_id.clone(),
            status: world.status(),
            tutorial_completed: world.world().and_then(|runtime| runtime.tutorial.as_ref().map(|state| state.completed)).unwrap_or(false),
        }
    }
}

async fn pause_world(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<Uuid>,
) -> Result<Json<DataResponse<WorldSummary>>, ApiError> {
    set_world_status(state, headers, world_id, WorldStatus::Paused).await
}

async fn resume_world(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<Uuid>,
) -> Result<Json<DataResponse<WorldSummary>>, ApiError> {
    set_world_status(state, headers, world_id, WorldStatus::Active).await
}

async fn set_world_status(
    state: AppState,
    headers: HeaderMap,
    world_id: Uuid,
    status: WorldStatus,
) -> Result<Json<DataResponse<WorldSummary>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let world = state.worlds.get(world_id).ok_or(ApiError::NotFound)?;
    if world.owner_user_id != session.user_id {
        return Err(ApiError::Forbidden);
    }
    let world = state
        .worlds
        .set_status(world_id, status)
        .ok_or(ApiError::NotFound)?;

    Ok(Json(DataResponse {
        data: WorldSummary::from(world.as_ref()),
    }))
}

async fn list_worlds(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataResponse<Vec<WorldSummary>>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let worlds = tokio::task::spawn_blocking(move || {
        state.worlds.list_for_user(session.user_id).iter().map(|world| {
            let mut summary = WorldSummary::from(world.as_ref());
            if world.is_tutorial() {
                summary.tutorial_completed = state.worlds.read_world(world.id, session.user_id)
                    .map_err(map_world_error)?.tutorial.as_ref().is_some_and(|state| state.completed);
            }
            Ok(summary)
        }).collect::<Result<Vec<_>, ApiError>>()
    }).await.map_err(|_| ApiError::Internal)??;

    Ok(Json(DataResponse { data: worlds }))
}

#[derive(Serialize)]
struct GroundResponse {
    generator: Option<crate::domain::GroundGenerationConfig>,
    perlin_permutations: Vec<Vec<u8>>,
    default_terrain: crate::domain::TerrainId,
    terrain_layers: Vec<crate::domain::world::TerrainLayer>,
    terrains: Vec<crate::domain::TerrainDefinition>,
}

async fn get_ground(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(world_id): Path<Uuid>,
) -> Result<Json<DataResponse<GroundResponse>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let worlds = state.worlds.clone();
    let runtime = tokio::task::spawn_blocking(move || worlds.read_world(world_id, session.user_id))
        .await
        .map_err(|_| ApiError::Internal)?
        .map_err(map_world_error)?;
    let generator = runtime.ground_config().cloned();
    let perlin_permutations = generator
        .as_ref()
        .map(|config| config.perlin_permutations())
        .unwrap_or_default();
    Ok(Json(DataResponse {
        data: GroundResponse {
            generator,
            perlin_permutations,
            default_terrain: runtime.default_terrain().clone(),
            terrain_layers: runtime.terrain_layers().to_vec(),
            terrains: state.terrains.iter().cloned().collect(),
        },
    }))
}

async fn get_or_create_tutorial(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataResponse<WorldSummary>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let world = state
        .worlds
        .get_or_create_tutorial(session.user_id)
        .map_err(map_world_error)?;

    Ok(Json(DataResponse {
        data: WorldSummary::from(world.as_ref()),
    }))
}

async fn get_or_create_named_tutorial(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tutorial_id): Path<String>,
) -> Result<Json<DataResponse<WorldSummary>>, ApiError> {
    let user_id = authenticate(&state, &headers).await?.user_id;
    let tutorial = crate::tutorial::find(&tutorial_id).ok_or(ApiError::NotFound)?;
    let world = tokio::task::spawn_blocking(move || {
        let level_id = tutorial["map"]["level_id"].as_str().ok_or(ApiError::Internal)?;
        let existing = state.worlds.list_for_user(user_id);
        if !existing.iter().any(|world| world.is_tutorial() && world.level_id == level_id) {
            for prerequisite in tutorial["prerequisites"].as_array().into_iter().flatten() {
                let id = prerequisite.as_str().ok_or(ApiError::Internal)?;
                let completed = existing.iter().filter(|world| world.is_tutorial()).any(|world| {
                    state.worlds.read_world(world.id, user_id).ok().is_some_and(|world|
                        world.tutorial.as_ref().is_some_and(|state| state.definition["id"] == id && state.completed))
                });
                if !completed { return Err(ApiError::Conflict("请先通关前置教程".into())); }
            }
        }
        let name = tutorial["title"].as_str().unwrap_or(level_id);
        state.worlds.create_tutorial_world(user_id, &tutorial_id, level_id, name).map_err(map_world_error)
    }).await.map_err(|_| ApiError::Internal)??;
    Ok(Json(DataResponse { data: WorldSummary::from(world.as_ref()) }))
}

async fn create_player_world(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateWorldRequest>,
) -> Result<Json<DataResponse<WorldSummary>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let world = state
        .worlds
        .create_player_world_with_ai_visibility(
            session.user_id,
            request.level_id,
            request.name.unwrap_or_default(),
            request.ai_full_visibility,
        )
        .map_err(map_world_error)?;

    Ok(Json(DataResponse {
        data: WorldSummary::from(world.as_ref()),
    }))
}

pub(super) fn map_world_error(error: WorldDirectoryError) -> ApiError {
    match error {
        WorldDirectoryError::WorldNotFound => ApiError::NotFound,
        WorldDirectoryError::Forbidden => ApiError::Forbidden,
        WorldDirectoryError::InvalidLevelId => ApiError::BadRequest("关卡 ID 不能为空".to_string()),
        WorldDirectoryError::LevelIdTooLong => ApiError::BadRequest(format!(
            "关卡 ID 不能超过 {} 个字符",
            crate::domain::world::MAX_LEVEL_ID_LENGTH
        )),
        WorldDirectoryError::InvalidWorldName => {
            ApiError::BadRequest("世界名称不能为空".to_string())
        }
        WorldDirectoryError::WorldNameTooLong => ApiError::BadRequest(format!(
            "世界名称不能超过 {} 个字符",
            crate::domain::world::MAX_WORLD_NAME_LENGTH
        )),
        WorldDirectoryError::WorldLimitReached => {
            ApiError::BadRequest("世界数量已达到上限".to_string())
        }
        error @ WorldDirectoryError::Generation(_) => {
            tracing::error!(target: "api", %error, "failed to create user world");
            ApiError::Internal
        }
        error @ (WorldDirectoryError::Storage(_)
        | WorldDirectoryError::Serialization(_)
        | WorldDirectoryError::SnapshotUnavailable) => {
            tracing::error!(target: "api", %error, "failed to access user world snapshot");
            ApiError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_request_accepts_optional_initial_contents() {
        let id = Uuid::new_v4();
        let request: CreateOrganismRequest =
            serde_json::from_value(serde_json::json!({ "model_id": id })).unwrap();
        assert_eq!(request.model_id, id);
        assert!(request.initial_contents.is_empty());
        let request: CreateOrganismRequest = serde_json::from_value(serde_json::json!({
            "model_id": id,
            "initial_contents": { (id.to_string()): { "non_gas_contents_mg": { "monosaccharide": 100.5 }, "gas_contents_ul": { "oxygen": 1000 } } }
        })).unwrap();
        assert_eq!(
            request.initial_contents[&id]
                .gas_contents_ul
                .values()
                .next(),
            Some(&1000)
        );
        for key in ["position", "rotation_mdeg", "component_states"] {
            let mut value = serde_json::json!({ "model_id": id });
            value[key] = serde_json::json!(0);
            assert!(serde_json::from_value::<CreateOrganismRequest>(value).is_err());
        }
    }
}
