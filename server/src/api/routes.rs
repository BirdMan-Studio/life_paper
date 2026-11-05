use super::auth;
use crate::state::AppState;
use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/api/v1/python-api", get(super::python_docs::catalog))
        .nest("/api/v1", auth::routes())
        .nest("/api/v1", super::organism_component::routes())
        .nest("/api/v1", super::organism_model::routes())
        .nest("/api/v1", super::player::routes())
        .nest("/api/v1", super::world::routes())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn root() -> &'static str {
    "hello"
}
