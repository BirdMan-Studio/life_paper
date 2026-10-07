use super::registry::TerrainRegistrationError;
use serde::{Deserialize, Serialize};
use std::{borrow::Borrow, fmt};

/// 地基在存档、地图和网络协议中的稳定标识。
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TerrainId(String);

impl TerrainId {
    pub fn new(value: impl Into<String>) -> Result<Self, TerrainRegistrationError> {
        let value = value.into();
        if !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            Ok(Self(value))
        } else {
            Err(TerrainRegistrationError::InvalidId(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for TerrainId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for TerrainId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 已注册地基的规则元数据。
///
/// 这里不实现地图生成和移动，只保存后续系统需要读取的规则。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerrainDefinition {
    pub id: TerrainId,
    pub name: String,
    pub slows_movement: bool,
    pub blocks_movement: bool,
    pub generates_oxygen: bool,
    /// 生成逻辑使用的配方标识。草地使用 `grass_matter`，其组成由后续生成系统解析。
    pub generated_matter_recipe: Option<String>,
}
