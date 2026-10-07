use super::{auth::authenticate, error::ApiError};
use crate::{
    domain::{WorldDirectoryError, WorldInstance, WorldKind, WorldStatus},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::Path,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/worlds", get(list_worlds).post(create_player_world))
        .route("/worlds/tutorial", post(get_or_create_tutorial))
        .route("/worlds/{world_id}/pause", post(pause_world))
        .route("/worlds/{world_id}/resume", post(resume_world))
}

#[derive(Serialize)]
struct DataResponse<T> {
    data: T,
}

#[derive(Deserialize)]
struct CreateWorldRequest {
    level_id: String,
    name: Option<String>,
}

#[derive(Serialize)]
struct WorldSummary {
    id: Uuid,
    kind: WorldKind,
    name: String,
    owner_user_id: i64,
    level_id: String,
    status: WorldStatus,
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
    let world = state.worlds.get(world_id).ok_or(ApiError::Unauthorized)?;
    if world.owner_user_id != session.user_id {
        return Err(ApiError::Unauthorized);
    }
    let world = state
        .worlds
        .set_status(world_id, status)
        .ok_or(ApiError::Unauthorized)?;

    Ok(Json(DataResponse {
        data: WorldSummary::from(world.as_ref()),
    }))
}

async fn list_worlds(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataResponse<Vec<WorldSummary>>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let worlds = state
        .worlds
        .list_for_user(session.user_id)
        .iter()
        .map(|world| WorldSummary::from(world.as_ref()))
        .collect();

    Ok(Json(DataResponse { data: worlds }))
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

async fn create_player_world(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateWorldRequest>,
) -> Result<Json<DataResponse<WorldSummary>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let world = state
        .worlds
        .create_player_world(
            session.user_id,
            request.level_id,
            request.name.unwrap_or_default(),
        )
        .map_err(map_world_error)?;

    Ok(Json(DataResponse {
        data: WorldSummary::from(world.as_ref()),
    }))
}

fn map_world_error(error: WorldDirectoryError) -> ApiError {
    match error {
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
