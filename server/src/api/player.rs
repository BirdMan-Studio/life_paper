use super::{auth::authenticate, error::ApiError};
use crate::state::AppState;
use axum::{Json, Router, extract::State, http::HeaderMap, routing::get};
use serde::Serialize;

pub fn routes() -> Router<AppState> {
    Router::new().route("/player/data", get(get_player_data))
}

#[derive(Serialize)]
struct DataResponse<T> {
    data: T,
}

#[derive(Serialize)]
struct PlayerDataResponse {
    energy_coins: i64,
    gold_coins: i64,
    bio_soup: Vec<ElementBalance>,
}

#[derive(Serialize)]
struct ElementBalance {
    element_id: String,
    name: String,
    symbol: String,
    amount: i64,
}

async fn get_player_data(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataResponse<PlayerDataResponse>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let data = state.player_data.get(session.user_id).await.map_err(|error| {
        tracing::error!(target: "api", %error, user_id = session.user_id, "failed to query player data");
        ApiError::Internal
    })?;

    let bio_soup = state
        .elements
        .iter()
        .filter(|element| matches!(element.category, crate::domain::ElementCategory::Basic))
        .map(|element| ElementBalance {
            element_id: element.id.to_string(),
            name: element.name.clone(),
            symbol: element.symbol.clone(),
            amount: data.bio_soup.get(element.id.as_str()).copied().unwrap_or(0),
        })
        .collect();

    Ok(Json(DataResponse {
        data: PlayerDataResponse {
            energy_coins: data.energy_coins,
            gold_coins: data.gold_coins,
            bio_soup,
        },
    }))
}
