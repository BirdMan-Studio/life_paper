mod api;
mod config;
mod db;
mod domain;
mod game;
mod state;

use config::Config;
use state::AppState;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::ChronoLocal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // --- 1. 日志初始化 ---
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let timer = ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_timer(timer)
        .init();

    tracing::info!(target: "main", "preparing to start server...");

    // --- 2. 加载配置 ---
    let working_dir = std::env::current_dir()?;
    let executable = std::env::current_exe()?;
    let config_path = match std::env::var_os("LIFE_PAPER_CONFIG") {
        Some(path) => working_dir.join(path),
        None => config::locate_config(&working_dir, executable.parent().unwrap_or(&working_dir)),
    };
    let config = Config::load_or_create(&config_path)?;
    let config_path = std::fs::canonicalize(config_path)?;
    let data_dir = config_path
        .parent()
        .expect("absolute configuration path")
        .join("data");
    tracing::info!(target: "main", config = %config_path.display(), data = %data_dir.display(), "config loaded");

    // --- 3. 初始化数据库（PostgreSQL + Redis） ---
    let (pg_pool, redis_pool) = match db::init(&config).await {
        Ok(pools) => pools,
        Err(e) => {
            tracing::error!(target: "main", error = %e, "database init failed, aborting");
            std::process::exit(1);
        }
    };

    // --- 4. 构建共享状态与 API 路由 ---
    let elements = Arc::new(domain::create_default_element_registry()?);
    let components = Arc::new(domain::create_default_component_registry()?);
    let terrains = Arc::new(domain::create_default_terrain_registry()?);
    let worlds = Arc::new(domain::WorldDirectory::new_with_storage(
        Arc::clone(&terrains),
        data_dir.join("worlds"),
    )?);
    let game_worlds = Arc::clone(&worlds);
    let organism_model_storage = data_dir.join("organism_models");
    std::fs::create_dir_all(&organism_model_storage)?;
    let organism_instance_storage = data_dir.join("organism_instances");
    std::fs::create_dir_all(&organism_instance_storage)?;

    let ai_runtime = Arc::new(game::ai::AiRuntime::new(config.ai.clone())?);
    ai_runtime.warm_up().await?;
    let state = AppState {
        ai_runtime: ai_runtime.clone(),
        users: db::UserRepository::new(pg_pool.clone()),
        component_unlocks: db::ComponentUnlockRepository::new(pg_pool.clone()),
        player_data: db::PlayerDataRepository::new(pg_pool.clone()),
        organism_models: db::OrganismModelRepository::new(pg_pool),
        organism_model_storage: Arc::new(organism_model_storage),
        organism_instance_storage: Arc::new(organism_instance_storage),
        organism_model_storage_lock: Arc::new(tokio::sync::Mutex::new(())),
        redis_pool,
        elements,
        components,
        terrains,
        worlds,
    };

    let game_elements = state.elements.clone();
    let game_storage = state.organism_instance_storage.clone();
    let game_model_lock = state.organism_model_storage_lock.clone();
    let app = api::router(state);

    // --- 5. 绑定端口 ---
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(target: "main", "server started on {}:{}", config.server.host, config.server.port);

    // --- 6. 启动游戏循环与 API 服务 ---
    let game_task = tokio::spawn(game::run(
        game_worlds,
        ai_runtime.clone(),
        game_elements,
        game_storage,
        game_model_lock,
    ));

    let server_result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    // 游戏循环目前没有自己的退出信号，API 服务停止后直接结束该任务。
    game_task.abort();
    let _ = game_task.await;
    ai_runtime.shutdown().await;

    server_result?;

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
