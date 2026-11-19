use crate::{api, config, db, domain, game, state::AppState};
use config::Config;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::prelude::*;

fn init_logging(logs: Arc<crate::observability::LogStore>) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string())),
        )
        .with(crate::observability::LogLayer(logs))
        .init();
}

pub async fn run(central_only: bool) -> anyhow::Result<()> {
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
    let logs = crate::observability::LogStore::open(data_dir.join("logs"))?;
    init_logging(logs.clone());
    let configured_admin_token = std::env::var_os("LIFE_PAPER_ADMIN_TOKEN")
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("LIFE_PAPER_ADMIN_TOKEN must be valid Unicode"))
        })
        .transpose()?
        .filter(|value| !value.is_empty());
    let uses_local_admin_token = configured_admin_token.is_none();
    let admin_token =
        crate::administration::load_admin_token(&data_dir.join("admin"), configured_admin_token)?;
    if uses_local_admin_token {
        tracing::info!(path = %data_dir.join("admin/admin-token.txt").display(), "admin credential loaded from local file");
    } else {
        tracing::info!("admin credential loaded from environment");
    }
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
    let worlds = Arc::new(if central_only {
        domain::WorldDirectory::new(Arc::clone(&terrains))
    } else {
        domain::WorldDirectory::new_with_storage(Arc::clone(&terrains), data_dir.join("worlds"))?
    });
    let game_worlds = Arc::clone(&worlds);
    let organism_model_storage = data_dir.join("organism_models");
    std::fs::create_dir_all(&organism_model_storage)?;
    let organism_instance_storage = data_dir.join("organism_instances");
    if !central_only {
        std::fs::create_dir_all(&organism_instance_storage)?;
    }

    let ai_runtime = Arc::new(game::ai::AiRuntime::new(if central_only {
        config::AiConfig::default()
    } else {
        config.ai.clone()
    })?);
    if !central_only {
        ai_runtime.warm_up().await?;
    }
    let state = AppState {
        admin: Arc::new(crate::administration::AdminService::new(
            pg_pool.clone(),
            logs.clone(),
            data_dir.join("admin"),
            std::env::var_os("LIFE_PAPER_ADMIN_DIST")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    let deployed = config_path.parent().unwrap().join("admin-dist");
                    if deployed.is_dir() {
                        deployed
                    } else {
                        let repository = config_path
                            .parent()
                            .and_then(|p| p.parent())
                            .map(|p| p.join("client/dist"));
                        repository
                            .filter(|p| p.is_dir())
                            .unwrap_or_else(|| working_dir.join("client/dist"))
                    }
                }),
            Some(admin_token),
        )?),
        ai_runtime: ai_runtime.clone(),
        signing_key: Arc::new(crate::transfer::load_signing_key(
            &data_dir.join("signing-key.pk8"),
        )?),
        central_only,
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
    let app = api::router(state, central_only).layer(axum::middleware::from_fn_with_state(
        logs,
        crate::observability::network,
    ));

    // --- 5. 绑定端口 ---
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(target: "main", "server started on {}:{}", config.server.host, config.server.port);

    // --- 6. 启动游戏循环与 API 服务 ---
    let game_task = (!central_only).then(|| {
        tokio::spawn(game::run(
            game_worlds,
            ai_runtime.clone(),
            game_elements,
            game_storage,
            game_model_lock,
        ))
    });

    let server_result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    // 游戏循环目前没有自己的退出信号，API 服务停止后直接结束该任务。
    if let Some(game_task) = game_task {
        game_task.abort();
        let _ = game_task.await;
    }
    ai_runtime.shutdown().await;

    server_result?;

    tracing::info!(target: "main", "server stopped");
    Ok(())
}

pub(crate) async fn shutdown_signal() {
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GameConfig {
    server_id: String,
    host: std::net::IpAddr,
    port: u16,
    data_dir: std::path::PathBuf,
    #[serde(default)]
    ai: config::AiConfig,
    auth: GameAuthConfig,
    #[serde(default)]
    registration: Option<crate::game_registry::RegistrationConfig>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum GameAuthConfig {
    Central { public_key: String },
    Local { token: String },
}

impl GameConfig {
    fn identity(&self) -> anyhow::Result<crate::game_state::GameIdentity> {
        anyhow::ensure!(
            !self.server_id.is_empty()
                && self.server_id.len() <= 128
                && self
                    .server_id
                    .bytes()
                    .all(|value| value.is_ascii_alphanumeric() || b"-_".contains(&value)),
            "invalid server_id"
        );
        Ok(match &self.auth {
            GameAuthConfig::Central { public_key } => {
                self.registration.as_ref().ok_or_else(|| anyhow::anyhow!("central mode requires [registration] with a centrally issued verification key"))?.validate()?;
                crate::game_state::GameIdentity::Signed {
                    server_id: self.server_id.clone(),
                    public_key: crate::transfer::decode_public_key(public_key)?,
                }
            }
            GameAuthConfig::Local { token } => {
                anyhow::ensure!(
                    self.registration.is_none(),
                    "local mode cannot register with a central service"
                );
                anyhow::ensure!(
                    self.host.is_loopback(),
                    "local mode must bind a loopback address"
                );
                anyhow::ensure!(
                    token.len() >= 32
                        && token.len() <= 256
                        && token.is_ascii()
                        && !token
                            .bytes()
                            .any(|value| value.is_ascii_whitespace() || value.is_ascii_control()),
                    "local token must contain 32..256 visible ASCII characters"
                );
                crate::game_state::GameIdentity::Local {
                    token: token.clone(),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_game_configuration_needs_no_database_and_cannot_bind_publicly() {
        let mut config: GameConfig =
            toml::from_str(include_str!("../game-local.example.toml")).unwrap();
        assert!(config.identity().is_ok());
        config.host = "0.0.0.0".parse().unwrap();
        assert!(config.identity().is_err());
        config.host = "::1".parse().unwrap();
        assert!(config.identity().is_ok());
        config.auth = GameAuthConfig::Local {
            token: "short".into(),
        };
        assert!(config.identity().is_err());
        assert!(toml::from_str::<GameConfig>("server_id = 'local'").is_err());
    }
}

/// Standalone simulation with authenticated central registration and no database connection.
pub async fn run_game() -> anyhow::Result<()> {
    let path = std::env::var_os("LIFE_PAPER_GAME_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("game-server.toml"));
    let path = std::fs::canonicalize(&path).map_err(|error| {
        anyhow::anyhow!(
            "cannot open {}: {error}; see server/game-server.example.toml",
            path.display()
        )
    })?;
    let config: GameConfig = toml::from_str(&std::fs::read_to_string(&path)?)?;
    let identity = config.identity()?;
    let data = path.parent().unwrap().join(&config.data_dir);
    let logs = crate::observability::LogStore::open(data.join("logs"))?;
    init_logging(logs.clone());
    let terrains = Arc::new(domain::create_default_terrain_registry()?);
    let worlds = Arc::new(domain::WorldDirectory::new_with_storage(
        terrains.clone(),
        data.join("worlds"),
    )?);
    let listener = tokio::net::TcpListener::bind((config.host, config.port)).await?;
    let registration = match (&config.registration, &identity) {
        (
            Some(registration),
            crate::game_state::GameIdentity::Signed {
                server_id,
                public_key,
            },
        ) => {
            let client = crate::game_registry::registration_client()?;
            crate::game_registry::announce(&client, registration, server_id, public_key, &worlds)
                .await?;
            tracing::info!(server_id = %server_id, "game server registration verified");
            Some((
                client,
                registration.clone(),
                server_id.clone(),
                public_key.clone(),
            ))
        }
        _ => None,
    };
    let storage = data.join("organism_instances");
    std::fs::create_dir_all(&storage)?;
    let ai = Arc::new(game::ai::AiRuntime::new(config.ai)?);
    ai.warm_up().await?;
    let state = crate::game_state::GameState {
        identity,
        ai_runtime: ai.clone(),
        worlds: worlds.clone(),
        terrains,
        elements: Arc::new(domain::create_default_element_registry()?),
        components: Arc::new(domain::create_default_component_registry()?),
        organism_instance_storage: Arc::new(storage),
        organism_model_storage_lock: Arc::new(tokio::sync::Mutex::new(())),
    };
    let app = api::game_router(state.clone())
        .merge(crate::administration::game_routes(state.clone(), logs.clone()))
        .route("/api/v1/python-api", axum::routing::get(api::python_docs::catalog).with_state(state.clone()))
        .route("/api/v1/game-server", axum::routing::get({
            let id = config.server_id.clone();
            let mode = if matches!(state.identity, crate::game_state::GameIdentity::Local { .. }) { "local" } else { "central" };
            move || async move { axum::Json(serde_json::json!({"data": {"server_id": id, "auth_mode": mode, "protocol_version": 1}})) }
        }))
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(axum::middleware::from_fn_with_state(logs, crate::observability::network));
    tracing::info!(address = %listener.local_addr()?, server_id = %config.server_id, data = %data.display(), "standalone game server started");
    let task = tokio::spawn(game::run(
        worlds.clone(),
        ai.clone(),
        state.elements,
        state.organism_instance_storage,
        state.organism_model_storage_lock,
    ));
    let registration_task = registration.map(|(client, registration, server_id, public_key)| {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                if let Err(error) = crate::game_registry::announce(
                    &client,
                    &registration,
                    &server_id,
                    &public_key,
                    &worlds,
                )
                .await
                {
                    tracing::warn!(%error, "game server registration heartbeat failed");
                }
            }
        })
    });
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    task.abort();
    let _ = task.await;
    if let Some(task) = registration_task {
        task.abort();
        let _ = task.await;
    }
    ai.shutdown().await;
    result?;
    Ok(())
}
