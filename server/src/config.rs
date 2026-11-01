use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Repository-root and server-directory launches resolve the same config.
pub fn locate_config(working_dir: &Path, executable_dir: &Path) -> PathBuf {
    [
        working_dir.join("config.toml"),
        working_dir.join("server/config.toml"),
        executable_dir.join("config.toml"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .unwrap_or_else(|| working_dir.join("config.toml"))
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub ai: AiConfig,
    pub server: ServerConfig,
    pub postgres: PostgresConfig,
    pub redis: RedisConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    pub python: String,
    pub timeout_ms: u64,
    pub max_queries: usize,
    pub workers: usize,
    pub cache_entries: usize,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            python: "python".into(),
            timeout_ms: 2000,
            max_queries: 256,
            workers: 2,
            cache_entries: 8,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct PostgresConfig {
    pub safe_mode: bool,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub max_connections: u32,
}

#[derive(Debug, Deserialize)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
    pub database: i64,
}

const DEFAULT_CONFIG: &str = r#"[server]
host = "0.0.0.0"
port = 8080

# Python worker executes trusted player code with this server account's OS permissions.
[ai]
enabled = false
python = "python"
timeout_ms = 2000
max_queries = 256
workers = 2
cache_entries = 8

# ===== PostgreSQL 配置 =====
[postgres]
# safe_mode = false: 初始化模式，用高权限用户建库、跑迁移，并创建 <database_name>_controller 用户
# safe_mode = true:  运行模式，直接用 user 字段连接
#
# 【安全建议】
# 如果你是第一次运行本服务且不会配置数据库，可以把 safe_mode 保持为 false，
# 并配置一个高权限账号（例如 postgres）。
# 第一次运行后，程序会自动：
#   1. 创建数据库（如果不存在）
#   2. 执行迁移，建好完整的游戏数据表
#   3. 创建名为 <database_name>_controller 的用户（例如 life_paper_controller），
#      密码与本次配置的 password 相同，
#      并且只授予该数据库的 DML 权限（SELECT / INSERT / UPDATE / DELETE）。
#
# 建议在首次运行成功后，把下面的 user 改为 <database_name>_controller，并修改该用户的密码
# 并把 safe_mode 改为 true，这样服务运行时只使用最小权限账号，更安全。
safe_mode = false

host = "localhost"
port = 5432
user = "change_me"
password = "change_me"
database = "life_paper"
max_connections = 20

# ===== Redis 配置 =====
[redis]
host = "localhost"
port = 6379
password = ""
database = 0
"#;

/// 所有必需字段的清单，格式为 (段名, 字段名)
const REQUIRED_FIELDS: &[(&str, &str)] = &[
    ("server", "host"),
    ("server", "port"),
    ("postgres", "safe_mode"),
    ("postgres", "host"),
    ("postgres", "port"),
    ("postgres", "user"),
    ("postgres", "password"),
    ("postgres", "database"),
    ("postgres", "max_connections"),
    ("redis", "host"),
    ("redis", "port"),
    ("redis", "password"),
    ("redis", "database"),
];

impl Config {
    pub fn load_or_create(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let display_path = path.display();
        // 文件不存在：生成模板，提示用户修改后重启，然后退出
        if !std::path::Path::new(path).exists() {
            std::fs::write(path, DEFAULT_CONFIG)?;
            tracing::warn!(
                target: "config",
                "config file not found, generated default template at {display_path}"
            );
            tracing::warn!(
                target: "config",
                "please edit {display_path} and restart the server"
            );
            std::process::exit(0);
        }

        let content = std::fs::read_to_string(path)?;

        // 先检查所有必需字段是否都存在，缺失就一次性列出并退出
        Self::validate(&content)?;

        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// 检查 config.toml 中所有必需字段是否都已填入。
    /// 缺失时返回错误，错误信息里列出所有缺失的字段。
    fn validate(content: &str) -> anyhow::Result<()> {
        let value: toml::Value =
            toml::from_str(content).map_err(|e| anyhow::anyhow!("config.toml 语法错误: {e}"))?;

        let mut missing = Vec::new();

        for (section, field) in REQUIRED_FIELDS {
            let present = value.get(section).and_then(|t| t.get(field)).is_some();
            if !present {
                missing.push(format!("{section}.{field}"));
            }
        }

        if !missing.is_empty() {
            anyhow::bail!(
                "config.toml 缺少以下字段，请补齐后重新启动:\n  - {}",
                missing.join("\n  - ")
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_config_is_optional_and_partially_configurable() {
        let config: AiConfig = toml::from_str("enabled = true").unwrap();
        assert!(config.enabled);
        assert_eq!(config.python, "python");
        assert_eq!(config.timeout_ms, 2000);
        let value: toml::Value = toml::from_str(DEFAULT_CONFIG).unwrap();
        let mut value = value;
        value.as_table_mut().unwrap().remove("ai");
        let legacy: Config = value.try_into().unwrap();
        assert!(!legacy.ai.enabled);
    }

    #[test]
    fn repository_and_server_launches_share_config_and_data_directory() {
        let root = std::env::temp_dir().join(format!("life-paper-paths-{}", uuid::Uuid::new_v4()));
        let server = root.join("server");
        let executable = root.join("target/debug");
        std::fs::create_dir_all(&server).unwrap();
        std::fs::create_dir_all(&executable).unwrap();
        std::fs::write(server.join("config.toml"), DEFAULT_CONFIG).unwrap();
        let from_root = std::fs::canonicalize(locate_config(&root, &executable)).unwrap();
        let from_server = std::fs::canonicalize(locate_config(&server, &executable)).unwrap();
        assert_eq!(from_root, from_server);
        assert_eq!(
            from_root.parent().unwrap().join("data/worlds"),
            std::fs::canonicalize(&server).unwrap().join("data/worlds")
        );
        std::fs::write(executable.join("config.toml"), DEFAULT_CONFIG).unwrap();
        assert_eq!(
            locate_config(&root.join("other"), &executable),
            executable.join("config.toml")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
