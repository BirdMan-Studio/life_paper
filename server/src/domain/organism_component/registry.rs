use super::definition::{
    ComponentCategory, ComponentDefinition, ComponentId, ComponentSlots, ComponentUnlockRule,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ComponentRegistry {
    definitions: BTreeMap<ComponentId, ComponentDefinition>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        category: ComponentCategory,
        slots: ComponentSlots,
        function_code: Option<&str>,
    ) -> Result<(), ComponentRegistrationError> {
        self.register_with_unlock(
            id,
            name,
            category,
            slots,
            function_code,
            ComponentUnlockRule::default(),
        )
    }

    pub fn register_with_unlock(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        category: ComponentCategory,
        slots: ComponentSlots,
        function_code: Option<&str>,
        unlock: ComponentUnlockRule,
    ) -> Result<(), ComponentRegistrationError> {
        let id = ComponentId::new(id)?;
        if self.definitions.contains_key(&id) {
            return Err(ComponentRegistrationError::DuplicateId(id));
        }

        self.definitions.insert(
            id.clone(),
            ComponentDefinition {
                model: super::ComponentModel::for_category(category),
                id,
                name: name.into(),
                category,
                slots,
                function_code: function_code.map(str::to_owned),
                unlock,
            },
        );
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&ComponentDefinition> {
        self.definitions.get(id)
    }

    pub fn set_model(
        &mut self,
        id: &str,
        model: super::ComponentModel,
    ) -> Result<(), ComponentRegistrationError> {
        let definition = self
            .definitions
            .get_mut(id)
            .ok_or_else(|| ComponentRegistrationError::InvalidModel(id.into()))?;
        if !model.valid_for(definition.category) {
            return Err(ComponentRegistrationError::InvalidModel(id.into()));
        }
        definition.model = model;
        Ok(())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.definitions.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ComponentDefinition> {
        self.definitions.values()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentRegistrationError {
    InvalidModel(String),
    InvalidId(String),
    DuplicateId(ComponentId),
}

impl fmt::Display for ComponentRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModel(id) => write!(formatter, "invalid component model: {id}"),
            Self::InvalidId(id) => write!(formatter, "invalid component id: {id}"),
            Self::DuplicateId(id) => write!(formatter, "component id already registered: {id}"),
        }
    }
}

impl std::error::Error for ComponentRegistrationError {}
