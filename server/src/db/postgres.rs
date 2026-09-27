use crate::config::PostgresConfig;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Executor, PgPool};
use std::time::Duration;

/// 统一入口：根据 safe_mode 决定走初始化流程还是直接连接
pub async fn connect(cfg: &PostgresConfig) -> anyhow::Result<PgPool> {
    if cfg.safe_mode {
        tracing::info!(target: "db", "safe_mode=true (normal mode)");
        normal_connect(cfg, &cfg.user, &cfg.password).await
    } else {
        tracing::warn!(
            target: "db",
            "safe_mode=false (init mode), assuming high-privilege user"
        );
        init_and_connect(cfg).await
    }
}

/// 直接用指定账号连接目标数据库
async fn normal_connect(
    cfg: &PostgresConfig,
    user: &str,
    password: &str,
) -> anyhow::Result<PgPool> {
    let opts = PgConnectOptions::new()
        .host(&cfg.host)
        .port(cfg.port)
        .username(user)
        .password(password)
        .database(&cfg.database);

    let pool = PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(opts)
        .await?;

    Ok(pool)
}

/// 初始化模式：
///   1. 用高权限账号建库
///   2. 创建 [database_name]_controller 用户
///   3. 跑迁移
///   4. 授予 controller 用户 DML 权限
///   5. 切换到 controller 用户返回连接池
async fn init_and_connect(cfg: &PostgresConfig) -> anyhow::Result<PgPool> {
    let controller_user = format!("{}_controller", cfg.database);
    // 按需求：controller 用户的密码与配置里的 password 相同
    let controller_password = cfg.password.clone();

    // --- 1. 连维护库 postgres，检查 / 创建目标数据库 ---
    let admin_opts = PgConnectOptions::new()
        .host(&cfg.host)
        .port(cfg.port)
        .username(&cfg.user)
        .password(&cfg.password)
        .database("postgres");

    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(admin_opts)
        .await?;

    let db_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&cfg.database)
            .fetch_one(&admin_pool)
            .await?;

    if !db_exists {
        tracing::info!(target: "db", "database does not exist, creating...");
        // CREATE DATABASE 不支持参数绑定，用双引号包标识符
        let sql = format!(r#"CREATE DATABASE "{}""#, cfg.database);
        admin_pool.execute(sql.as_str()).await?;
    } else {
        tracing::info!(target: "db", "database already exists");
    }

    // --- 2. 创建 controller 用户（如不存在） ---
    let user_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(&controller_user)
            .fetch_one(&admin_pool)
            .await?;

    if !user_exists {
        tracing::info!(target: "db", "creating controller user...");
        // 密码里的单引号需要转义
        let escaped_pwd = controller_password.replace('\'', "''");
        let sql = format!(
            r#"CREATE USER "{}" WITH PASSWORD '{}'"#,
            controller_user, escaped_pwd
        );
        admin_pool.execute(sql.as_str()).await?;
    } else {
        tracing::info!(target: "db", "controller user already exists");
    }

    admin_pool.close().await;

    // --- 3. 用高权限账号连目标库，跑迁移 ---
    let target_opts = PgConnectOptions::new()
        .host(&cfg.host)
        .port(cfg.port)
        .username(&cfg.user)
        .password(&cfg.password)
        .database(&cfg.database);

    let target_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(target_opts)
        .await?;

    tracing::info!(target: "db", "running migrations...");
    sqlx::migrate!("./migrations").run(&target_pool).await?;
    tracing::info!(target: "db", "migrations complete");

    // --- 4. 授予 controller 用户 DML 权限 ---
    // 已有对象用 GRANT ... ON ALL ...；
    // 未来的对象用 ALTER DEFAULT PRIVILEGES（以高权限账号为 owner 角色）
    let grants = [
        format!(
            r#"GRANT CONNECT ON DATABASE "{}" TO "{}""#,
            cfg.database, controller_user
        ),
        format!(r#"GRANT USAGE ON SCHEMA public TO "{}""#, controller_user),
        format!(
            r#"GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO "{}""#,
            controller_user
        ),
        format!(
            r#"GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO "{}""#,
            controller_user
        ),
        format!(
            r#"ALTER DEFAULT PRIVILEGES FOR ROLE "{}" IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO "{}""#,
            cfg.user, controller_user
        ),
        format!(
            r#"ALTER DEFAULT PRIVILEGES FOR ROLE "{}" IN SCHEMA public GRANT USAGE, SELECT ON SEQUENCES TO "{}""#,
            cfg.user, controller_user
        ),
    ];

    for g in &grants {
        target_pool.execute(g.as_str()).await?;
    }

    target_pool.close().await;

    // --- 5. 切换到 controller 用户建立运行时连接池 ---
    tracing::info!(
        target: "db",
        "switched to controller user for runtime operations"
    );
    normal_connect(cfg, &controller_user, &controller_password).await
}
