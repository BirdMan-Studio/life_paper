pub mod postgres;
pub mod redis_pool;
pub mod user;

pub use user::UserRepository;

use crate::config::Config;

pub async fn init(cfg: &Config) -> anyhow::Result<(sqlx::PgPool, deadpool_redis::Pool)> {
    tracing::info!(target: "db", "connecting to PostgreSQL...");
    let pg_pool = postgres::connect(&cfg.postgres).await?;
    tracing::info!(target: "db", "PostgreSQL connected");

    tracing::info!(target: "db", "connecting to Redis...");
    let redis_pool = redis_pool::create_pool(&cfg.redis)?;

    // 这里 redis:: 就明确指外部 crate 了
    let mut conn = redis_pool.get().await?;
    redis::cmd("PING").query_async::<String>(&mut conn).await?;
    tracing::info!(target: "db", "Redis connected");

    Ok((pg_pool, redis_pool))
}
