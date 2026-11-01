use super::definition::{TerrainDefinition, TerrainId, TerrainVisual};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TerrainRegistry {
    definitions: BTreeMap<TerrainId, TerrainDefinition>,
}

impl TerrainRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        slows_movement: bool,
        blocks_movement: bool,
        generates_oxygen: bool,
        generated_matter_recipe: Option<&str>,
    ) -> Result<(), TerrainRegistrationError> {
        let id = TerrainId::new(id)?;
        if self.definitions.contains_key(&id) {
            return Err(TerrainRegistrationError::DuplicateId(id));
        }

        self.definitions.insert(
            id.clone(),
            TerrainDefinition {
                id,
                name: name.into(),
                slows_movement,
                blocks_movement,
                generates_oxygen,
                generated_matter_recipe: generated_matter_recipe.map(str::to_owned),
                visual: TerrainVisual::default(),
            },
        );
        Ok(())
    }

    pub fn set_visual(&mut self, id: &str, visual: TerrainVisual) -> bool {
        let Some(definition) = self.definitions.get_mut(id) else {
            return false;
        };
        definition.visual = visual;
        true
    }

    pub fn get(&self, id: &str) -> Option<&TerrainDefinition> {
        self.definitions.get(id)
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TerrainDefinition> {
        self.definitions.values()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerrainRegistrationError {
    InvalidId(String),
    DuplicateId(TerrainId),
}

impl fmt::Display for TerrainRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(formatter, "invalid terrain id: {id}"),
            Self::DuplicateId(id) => write!(formatter, "terrain id already registered: {id}"),
        }
    }
}

impl std::error::Error for TerrainRegistrationError {}
