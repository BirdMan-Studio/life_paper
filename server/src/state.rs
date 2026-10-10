use crate::{
    db::{
        ComponentUnlockRepository, OrganismModelRepository, PlayerDataRepository, UserRepository,
    },
    domain::{ComponentRegistry, ElementRegistry, TerrainRegistry, WorldDirectory},
};
use deadpool_redis::Pool as RedisPool;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub users: UserRepository,
    pub component_unlocks: ComponentUnlockRepository,
    pub player_data: PlayerDataRepository,
    pub organism_models: OrganismModelRepository,
    pub organism_model_storage: Arc<PathBuf>,
    pub organism_model_storage_lock: Arc<tokio::sync::Mutex<()>>,
    pub redis_pool: RedisPool,
    pub elements: Arc<ElementRegistry>,
    pub components: Arc<ComponentRegistry>,
    pub terrains: Arc<TerrainRegistry>,
    pub worlds: Arc<WorldDirectory>,
}
