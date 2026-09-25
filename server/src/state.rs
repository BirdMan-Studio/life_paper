use deadpool_redis::Pool as RedisPool;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pg_pool: PgPool,
    pub redis_pool: RedisPool,
}