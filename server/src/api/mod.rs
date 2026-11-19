pub(crate) mod auth;
pub(crate) mod error;
mod organism_component;
pub(crate) mod organism_model;
mod player;
pub(crate) mod python_docs;
mod routes;
pub(crate) mod storage_quota;
mod transfer;
mod world;

pub(crate) use routes::game_router;
pub use routes::router;
