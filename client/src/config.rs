use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8080/api/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "zh_CN", alias = "simplified_chinese")]
    ZhCn,
    #[serde(rename = "en_US", alias = "english")]
    EnUs,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::ZhCn, Self::EnUs];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::EnUs => "English",
        }
    }
}

#[derive(Debug)]
pub struct ConfigError {
    pub message_key: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    pub server_url: String,
    pub language: Language,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER_URL.to_string(),
            language: Language::default(),
        }
    }
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new() -> Self {
        let path = ProjectDirs::from("dev", "LifePaper", "LifePaper")
            .map(|dirs| dirs.config_dir().join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("client-config.toml"));

        Self { path }
    }

    pub fn load(&self) -> Result<ClientConfig, ConfigError> {
        if !self.path.exists() {
            return Ok(ClientConfig::default());
        }

        let content = fs::read_to_string(&self.path).map_err(|error| ConfigError {
            message_key: "config.error.read",
            detail: error.to_string(),
        })?;
        toml::from_str(&content).map_err(|error| ConfigError {
            message_key: "config.error.parse",
            detail: error.to_string(),
        })
    }

    pub fn save(&self, config: &ClientConfig) -> Result<(), ConfigError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| ConfigError {
                message_key: "config.error.create_directory",
                detail: error.to_string(),
            })?;
        }

        let content = toml::to_string_pretty(config).map_err(|error| ConfigError {
            message_key: "config.error.serialize",
            detail: error.to_string(),
        })?;
        fs::write(&self.path, content).map_err(|error| ConfigError {
            message_key: "config.error.save",
            detail: error.to_string(),
        })
    }
}
