use super::registry::ComponentRegistrationError;
use serde::{Deserialize, Serialize};
use std::{borrow::Borrow, collections::BTreeMap, fmt};

/// 生物组件的稳定注册 ID。
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ComponentId(String);

impl ComponentId {
    pub fn new(value: impl Into<String>) -> Result<Self, ComponentRegistrationError> {
        let value = value.into();
        if !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            Ok(Self(value))
        } else {
            Err(ComponentRegistrationError::InvalidId(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ComponentId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 目前的三种组件类别。连接规则使用这个类别，而不是具体组件 ID。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentCategory {
    ExternalOrgan,
    Body,
    InternalOrgan,
}

/// 组件可接收的外接组件槽位数量。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComponentSlots {
    counts: BTreeMap<ComponentCategory, u32>,
}

impl ComponentSlots {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, category: ComponentCategory, count: u32) -> Self {
        if count == 0 {
            self.counts.remove(&category);
        } else {
            self.counts.insert(category, count);
        }
        self
    }

    pub fn count(&self, category: ComponentCategory) -> u32 {
        self.counts.get(&category).copied().unwrap_or(0)
    }

    pub fn total(&self) -> u32 {
        self.counts.values().sum()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ComponentCategory, u32)> + '_ {
        self.counts
            .iter()
            .map(|(category, count)| (*category, *count))
    }
}

/// 可注册的生物组件定义。
///
/// `function_code` 仅保存未来功能实现的稳定代码，不在注册阶段执行。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ComponentDefinition {
    pub id: ComponentId,
    pub name: String,
    pub category: ComponentCategory,
    pub slots: ComponentSlots,
    pub function_code: Option<String>,
}
