use super::{auth::authenticate, error::ApiError};
use crate::db::UnlockResult;
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Serialize;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/organism-components", get(list_all))
        .route("/organism-components/unlocked", get(list_unlocked))
        .route("/organism-components/locked", get(list_locked))
        .route(
            "/organism-components/{component_id}/unlock",
            post(unlock_component),
        )
}

#[derive(Serialize)]
struct DataResponse<T> {
    data: T,
}

async fn list_all(
    State(state): State<AppState>,
) -> Result<Json<DataResponse<Vec<crate::domain::ComponentDefinition>>>, ApiError> {
    let components = state.components.iter().cloned().collect();
    Ok(Json(DataResponse { data: components }))
}

async fn list_unlocked(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataResponse<Vec<crate::domain::ComponentDefinition>>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let unlocked = state
        .component_unlocks
        .list_for_user(session.user_id)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, user_id = session.user_id, "failed to query unlocked components");
            ApiError::Internal
        })?;
    let components = state
        .components
        .iter()
        .filter(|component| {
            component.unlock.default_unlocked || unlocked.contains(component.id.as_str())
        })
        .cloned()
        .collect();

    Ok(Json(DataResponse { data: components }))
}

async fn list_locked(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataResponse<Vec<crate::domain::ComponentDefinition>>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let unlocked = state
        .component_unlocks
        .list_for_user(session.user_id)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, user_id = session.user_id, "failed to query unlocked components");
            ApiError::Internal
        })?;
    let components = state
        .components
        .iter()
        .filter(|component| {
            !component.unlock.default_unlocked && !unlocked.contains(component.id.as_str())
        })
        .cloned()
        .collect();

    Ok(Json(DataResponse { data: components }))
}

#[derive(Serialize)]
struct UnlockResponse {
    component: crate::domain::ComponentDefinition,
    energy_coins: i64,
    gold_coins: i64,
}

async fn unlock_component(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(component_id): Path<String>,
) -> Result<Json<DataResponse<UnlockResponse>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let component = state
        .components
        .get(&component_id)
        .ok_or(ApiError::NotFound)?;

    if component.unlock.default_unlocked {
        let data = state
            .player_data
            .get(session.user_id)
            .await
            .map_err(|error| {
                tracing::error!(target: "api", %error, "failed to query player wallet");
                ApiError::Internal
            })?;
        return Ok(Json(DataResponse {
            data: UnlockResponse {
                component: component.clone(),
                energy_coins: data.energy_coins,
                gold_coins: data.gold_coins,
            },
        }));
    }

    let result = state
        .component_unlocks
        .unlock_with_methods(session.user_id, &component_id, &component.unlock.methods)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, user_id = session.user_id, %component_id, "failed to unlock component");
            ApiError::Internal
        })?;

    match result {
        UnlockResult::InsufficientFunds => {
            return Err(ApiError::Conflict("能量币或金币余额不足".to_string()));
        }
        UnlockResult::Unlocked | UnlockResult::AlreadyUnlocked => {}
    }

    let data = state
        .player_data
        .get(session.user_id)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, "failed to query player wallet after unlock");
            ApiError::Internal
        })?;
    Ok(Json(DataResponse {
        data: UnlockResponse {
            component: component.clone(),
            energy_coins: data.energy_coins,
            gold_coins: data.gold_coins,
        },
    }))
}
