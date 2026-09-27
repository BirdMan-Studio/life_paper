use super::auth;
use crate::state::AppState;
use axum::{Router, routing::get};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .nest("/api/v1", auth::routes())
        .with_state(state)
}

async fn root() -> &'static str {
    "hello"
}
