use crate::config::RedisConfig;
use deadpool_redis::{
    Config as RedisPoolConfig, ConnectionAddr, ConnectionInfo, Pool as RedisPool,
    RedisConnectionInfo, Runtime,
};

pub fn create_pool(cfg: &RedisConfig) -> anyhow::Result<RedisPool> {
    let addr = ConnectionAddr::Tcp(cfg.host.clone(), cfg.port);

    let conn_info = RedisConnectionInfo {
        db: cfg.database,
        username: None,
        password: if cfg.password.is_empty() {
            None
        } else {
            Some(cfg.password.clone())
        },
        protocol: Default::default(),
    };

    let pool_cfg = RedisPoolConfig {
        url: None,
        pool: None,
        connection: Some(ConnectionInfo {
            addr,
            redis: conn_info,
        }),
    };

    let pool = pool_cfg.create_pool(Some(Runtime::Tokio1))?;
    Ok(pool)
}
