mod config;
mod db;
mod state;

use config::Config;
use state::AppState;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // --- 1. 日志初始化 ---
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let timer = ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_timer(timer)
        .init();

    tracing::info!(target: "main", "preparing to start server...");

    // --- 2. 加载配置 ---
    let config = Config::load_or_create("config.toml")?;
    tracing::info!(target: "main", "config loaded");

    // --- 3. 初始化数据库（PostgreSQL + Redis） ---
    let (pg_pool, redis_pool) = match db::init(&config).await {
        Ok(pools) => pools,
        Err(e) => {
            tracing::error!(target: "main", error = %e, "database init failed, aborting");
            std::process::exit(1);
        }
    };

    // --- 4. 构建共享状态与路由 ---
    let state = AppState { pg_pool, redis_pool };

    let app: axum::Router = axum::Router::new()
        .route("/", axum::routing::get(|| async { "hello" }))
        .with_state(state);

    // --- 5. 绑定端口 ---
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(target: "main", "listening on {}:{}", config.server.host, config.server.port);

    // --- 6. 启动服务 + 优雅关闭 ---
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!(target: "main", "server stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!(target: "main", "shutdown signal received, stopping...");
}