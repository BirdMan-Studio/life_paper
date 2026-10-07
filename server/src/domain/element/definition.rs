use super::registry::ElementRegistrationError;
use serde::{Deserialize, Serialize};
use std::{borrow::Borrow, fmt};

/// 物质在存档、库存和网络协议中的稳定标识。
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ElementId(String);

impl ElementId {
    pub fn new(value: impl Into<String>) -> Result<Self, ElementRegistrationError> {
        let value = value.into();
        if is_valid_element_id(&value) {
            Ok(Self(value))
        } else {
            Err(ElementRegistrationError::InvalidId(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ElementId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn is_valid_element_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Solid,
    Liquid,
    Gas,
}

/// 三种物态对应的二维面密度，单位为 `g/cm²`。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PhaseArealDensity {
    pub solid: Option<f64>,
    pub liquid: Option<f64>,
    pub gas: Option<f64>,
}

impl PhaseArealDensity {
    pub const fn new(solid: Option<f64>, liquid: Option<f64>, gas: Option<f64>) -> Self {
        Self { solid, liquid, gas }
    }

    pub const fn get(self, phase: Phase) -> Option<f64> {
        match phase {
            Phase::Solid => self.solid,
            Phase::Liquid => self.liquid,
            Phase::Gas => self.gas,
        }
    }
}

/// 物质的二维物理属性。
///
/// 面密度是二维世界使用的游戏参数，不表示现实三维物质的体积密度。
/// 温度单位为摄氏度，相变温度采用常压下的近似值。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ElementPhysicalProperties {
    pub areal_density: PhaseArealDensity,
    pub melting_point_celsius: f64,
    pub boiling_point_celsius: Option<f64>,
}

impl ElementPhysicalProperties {
    pub const fn new(
        areal_density: PhaseArealDensity,
        melting_point_celsius: f64,
        boiling_point_celsius: Option<f64>,
    ) -> Self {
        Self {
            areal_density,
            melting_point_celsius,
            boiling_point_celsius,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementCategory {
    Basic,
    Compound,
}

/// 一种已注册物质的完整定义。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ElementDefinition {
    pub id: ElementId,
    pub name: String,
    pub symbol: String,
    pub category: ElementCategory,
    pub physical: ElementPhysicalProperties,
}

impl ElementDefinition {
    pub const fn areal_density(&self, phase: Phase) -> Option<f64> {
        self.physical.areal_density.get(phase)
    }
}
