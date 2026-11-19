use super::{auth::authenticate, error::ApiError};
use crate::{
    state::AppState,
    transfer::{self, ModelBundle},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/game-public-key", get(public_key))
        .route("/auth/game-ticket", post(ticket))
        .route("/organism-models/{model_id}/export", post(export_model))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Audience {
    game_server_id: String,
}

impl Audience {
    fn validate(&self) -> Result<(), ApiError> {
        if self.game_server_id.is_empty()
            || self.game_server_id.len() > 128
            || !self
                .game_server_id
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || b"-_".contains(&value))
        {
            return Err(ApiError::BadRequest("invalid game_server_id".into()));
        }
        Ok(())
    }
}

async fn public_key(State(state): State<AppState>) -> Json<Value> {
    Json(
        json!({"data": {"algorithm": "Ed25519", "public_key": transfer::public_key(&state.signing_key)}}),
    )
}

async fn ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<Audience>,
) -> Result<Json<Value>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    request.validate()?;
    let server = state.admin.connected_server(&request.game_server_id)?;
    let token = transfer::sign(
        &state.signing_key,
        "game-entry",
        &request.game_server_id,
        session.user_id,
        (),
    )
    .map_err(|_| ApiError::Internal)?;
    Ok(Json(
        json!({"data": {"token": token, "expires_in": 900, "game_server_id": request.game_server_id, "api_url":server.api_url, "direct_url":server.api_url,"relay_url":crate::relay::relay_path(&server.id)}}),
    ))
}

async fn export_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Json(request): Json<Audience>,
) -> Result<Json<Value>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    request.validate()?;
    state.admin.connected_server(&request.game_server_id)?;
    let guard = state.organism_model_storage_lock.clone().lock_owned().await;
    let model = state
        .organism_models
        .find_owned(model_id, session.user_id)
        .await
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::NotFound)?;
    let root = state.organism_model_storage.join(model.id.to_string());
    let data = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        let bundle =
            ModelBundle::read(model.id, model.name, model.composition, &root).map_err(|error| {
                tracing::error!(%error, "failed to export model");
                ApiError::Internal
            })?;
        transfer::sign(
            &state.signing_key,
            "model-bundle",
            &request.game_server_id,
            session.user_id,
            bundle,
        )
        .map_err(|_| ApiError::Internal)
    })
    .await
    .map_err(|_| ApiError::Internal)??;
    Ok(Json(
        json!({"data": {"model_bundle": data, "expires_in": 900}}),
    ))
}
