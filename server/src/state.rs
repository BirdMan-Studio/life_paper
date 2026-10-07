use crate::{
    db::UserRepository,
    domain::{ComponentRegistry, ElementRegistry, TerrainRegistry, WorldDirectory},
};
use deadpool_redis::Pool as RedisPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub users: UserRepository,
    pub redis_pool: RedisPool,
    pub elements: Arc<ElementRegistry>,
    pub components: Arc<ComponentRegistry>,
    pub terrains: Arc<TerrainRegistry>,
    pub worlds: Arc<WorldDirectory>,
}
