use super::auth;
use crate::state::AppState;
use axum::{Router, routing::get};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .nest("/api/v1", auth::routes())
        .nest("/api/v1", super::organism_component::routes())
        .nest("/api/v1", super::organism_model::routes())
        .nest("/api/v1", super::player::routes())
        .nest("/api/v1", super::world::routes())
        .with_state(state)
}

async fn root() -> &'static str {
    "hello"
}
