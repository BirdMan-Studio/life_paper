use crate::db::UserRepository;
use deadpool_redis::Pool as RedisPool;

#[derive(Clone)]
pub struct AppState {
    pub users: UserRepository,
    pub redis_pool: RedisPool,
}
