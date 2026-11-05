use super::definition::{ElementCategory, ElementDefinition, ElementId, ElementPhysicalProperties};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

/// 游戏可用物质的注册表。
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ElementRegistry {
    definitions: BTreeMap<ElementId, ElementDefinition>,
}

impl ElementRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一种物质及其全部物理属性。
    ///
    /// ID 只能使用小写 ASCII 字母、数字和下划线，重复 ID 会被拒绝。
    pub fn register(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        symbol: impl Into<String>,
        category: ElementCategory,
        physical: ElementPhysicalProperties,
    ) -> Result<(), ElementRegistrationError> {
        let id = ElementId::new(id)?;
        if self.definitions.contains_key(&id) {
            return Err(ElementRegistrationError::DuplicateId(id));
        }

        let definition = ElementDefinition {
            id: id.clone(),
            name: name.into(),
            symbol: symbol.into(),
            category,
            physical,
            initial_processing_speed: super::definition::default_processing_speed(),
            chemistry: None,
        };
        self.definitions.insert(id, definition);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&ElementDefinition> {
        self.definitions.get(id)
    }

    pub fn set_initial_processing_speed(
        &mut self,
        id: &str,
        speed: f64,
    ) -> Result<(), ElementRegistrationError> {
        if !speed.is_finite() || !(0.0..=1.0).contains(&speed) {
            return Err(ElementRegistrationError::InvalidProcessingSpeed(id.into()));
        }
        let definition = self
            .definitions
            .get_mut(id)
            .ok_or_else(|| ElementRegistrationError::UnknownElement(id.into()))?;
        definition.initial_processing_speed = speed;
        Ok(())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.definitions.contains_key(id)
    }

    pub fn set_chemistry(
        &mut self,
        id: &str,
        chemistry: super::chemistry::ChemicalProperties,
    ) -> Result<(), ElementRegistrationError> {
        if !chemistry.is_valid() {
            return Err(ElementRegistrationError::InvalidChemistry(id.into()));
        }
        self.definitions
            .get_mut(id)
            .ok_or_else(|| ElementRegistrationError::UnknownElement(id.into()))?
            .chemistry = Some(chemistry);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ElementDefinition> {
        self.definitions.values()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ElementRegistrationError {
    InvalidId(String),
    DuplicateId(ElementId),
    UnknownElement(String),
    InvalidProcessingSpeed(String),
    InvalidChemistry(String),
}

impl fmt::Display for ElementRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(formatter, "invalid element id: {id}"),
            Self::DuplicateId(id) => write!(formatter, "element id already registered: {id}"),
            Self::UnknownElement(id) => write!(formatter, "unknown element: {id}"),
            Self::InvalidProcessingSpeed(id) => {
                write!(formatter, "invalid processing speed for element: {id}")
            }
            Self::InvalidChemistry(id) => write!(formatter, "invalid chemistry for element: {id}"),
        }
    }
}

impl std::error::Error for ElementRegistrationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::element::PhaseArealDensity;

    #[test]
    fn rejects_duplicate_and_invalid_ids() {
        let mut registry = ElementRegistry::new();
        let physical = ElementPhysicalProperties::new(
            PhaseArealDensity::new(Some(1.0), None, None),
            1.0,
            None,
        );

        registry
            .register(
                "test_element",
                "测试",
                "T",
                ElementCategory::Basic,
                physical,
            )
            .unwrap();
        assert!(matches!(
            registry.register(
                "test_element",
                "重复",
                "T2",
                ElementCategory::Basic,
                physical
            ),
            Err(ElementRegistrationError::DuplicateId(_))
        ));
        assert!(matches!(
            registry.register("Invalid ID", "错误", "X", ElementCategory::Basic, physical),
            Err(ElementRegistrationError::InvalidId(_))
        ));
    }
}
