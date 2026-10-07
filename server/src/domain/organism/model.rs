use super::program::OrganismProgramId;
use crate::domain::organism_component::{ComponentCategory, ComponentId, ComponentRegistry};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComponentInstance {
    pub id: Uuid,
    pub component: ComponentId,
}

impl ComponentInstance {
    pub fn new(
        component: impl Into<String>,
        registry: &ComponentRegistry,
    ) -> Result<Self, OrganismError> {
        let component = component.into();
        let definition = registry
            .get(&component)
            .ok_or_else(|| OrganismError::UnknownComponent(component.clone()))?;
        Ok(Self {
            id: Uuid::new_v4(),
            component: definition.id.clone(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ComponentConnection {
    pub first: Uuid,
    pub second: Uuid,
}

impl ComponentConnection {
    pub fn new(first: Uuid, second: Uuid) -> Result<Self, OrganismError> {
        if first == second {
            return Err(OrganismError::SelfConnection(first));
        }
        let (first, second) = if first < second {
            (first, second)
        } else {
            (second, first)
        };
        Ok(Self { first, second })
    }
}

/// 由组件节点和连接组成的生物。未连接的槽位自然保持为空。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BiologicalOrganism {
    pub id: Uuid,
    pub program_id: OrganismProgramId,
    pub components: BTreeMap<Uuid, ComponentInstance>,
    pub connections: BTreeSet<ComponentConnection>,
}

impl BiologicalOrganism {
    pub fn new(program_id: OrganismProgramId) -> Self {
        Self {
            id: Uuid::new_v4(),
            program_id,
            components: BTreeMap::new(),
            connections: BTreeSet::new(),
        }
    }

    pub fn add_component(
        &mut self,
        component: impl Into<String>,
        registry: &ComponentRegistry,
    ) -> Result<Uuid, OrganismError> {
        let instance = ComponentInstance::new(component, registry)?;
        let id = instance.id;
        self.components.insert(id, instance);
        Ok(id)
    }

    pub fn connect(
        &mut self,
        first: Uuid,
        second: Uuid,
        registry: &ComponentRegistry,
    ) -> Result<(), OrganismError> {
        let first_instance = self
            .components
            .get(&first)
            .ok_or(OrganismError::UnknownComponentInstance(first))?;
        let second_instance = self
            .components
            .get(&second)
            .ok_or(OrganismError::UnknownComponentInstance(second))?;
        let first_definition = registry
            .get(first_instance.component.as_str())
            .ok_or_else(|| OrganismError::UnknownComponent(first_instance.component.to_string()))?;
        let second_definition = registry
            .get(second_instance.component.as_str())
            .ok_or_else(|| {
                OrganismError::UnknownComponent(second_instance.component.to_string())
            })?;
        let connection = ComponentConnection::new(first, second)?;
        if self.connections.contains(&connection) {
            return Err(OrganismError::DuplicateConnection(connection));
        }

        let first_count =
            self.connection_count_for_category(first, second_definition.category, registry)?;
        let second_count =
            self.connection_count_for_category(second, first_definition.category, registry)?;
        if first_count >= first_definition.slots.count(second_definition.category) {
            return Err(OrganismError::NoAvailableSlot {
                instance: first,
                accepts: second_definition.category,
            });
        }
        if second_count >= second_definition.slots.count(first_definition.category) {
            return Err(OrganismError::NoAvailableSlot {
                instance: second,
                accepts: first_definition.category,
            });
        }

        self.connections.insert(connection);
        Ok(())
    }

    pub fn connection_count(&self, instance: Uuid) -> u32 {
        self.connections
            .iter()
            .filter(|connection| connection.first == instance || connection.second == instance)
            .count() as u32
    }

    /// Counts only connections whose other endpoint belongs to `category`.
    /// Different slot categories must not consume one another's capacity.
    pub fn connection_count_for_category(
        &self,
        instance: Uuid,
        category: ComponentCategory,
        registry: &ComponentRegistry,
    ) -> Result<u32, OrganismError> {
        self.connections
            .iter()
            .filter_map(|connection| {
                let other = if connection.first == instance {
                    Some(connection.second)
                } else if connection.second == instance {
                    Some(connection.first)
                } else {
                    None
                }?;
                let other_instance = self
                    .components
                    .get(&other)
                    .ok_or(OrganismError::UnknownComponentInstance(other));
                Some(other_instance.and_then(|other_instance| {
                    let definition =
                        registry
                            .get(other_instance.component.as_str())
                            .ok_or_else(|| {
                                OrganismError::UnknownComponent(
                                    other_instance.component.to_string(),
                                )
                            })?;
                    Ok(u32::from(definition.category == category))
                }))
            })
            .try_fold(0_u32, |count, result| result.map(|matched| count + matched))
    }

    pub fn remove_component(&mut self, instance: Uuid) -> bool {
        if self.components.remove(&instance).is_none() {
            return false;
        }
        self.connections
            .retain(|connection| connection.first != instance && connection.second != instance);
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganismError {
    UnknownComponent(String),
    UnknownComponentInstance(Uuid),
    SelfConnection(Uuid),
    DuplicateConnection(ComponentConnection),
    NoAvailableSlot {
        instance: Uuid,
        accepts: ComponentCategory,
    },
}

impl fmt::Display for OrganismError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownComponent(id) => write!(formatter, "unknown component: {id}"),
            Self::UnknownComponentInstance(id) => {
                write!(formatter, "unknown component instance: {id}")
            }
            Self::SelfConnection(id) => {
                write!(formatter, "component cannot connect to itself: {id}")
            }
            Self::DuplicateConnection(connection) => write!(
                formatter,
                "component connection already exists: {}-{}",
                connection.first, connection.second
            ),
            Self::NoAvailableSlot { instance, accepts } => {
                write!(
                    formatter,
                    "component {instance} has no available slot for {accepts:?}"
                )
            }
        }
    }
}

impl std::error::Error for OrganismError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::create_default_component_registry;

    #[test]
    fn builds_connected_organism_with_empty_slots() {
        let registry = create_default_component_registry().unwrap();
        let mut organism = BiologicalOrganism::new(OrganismProgramId::new());
        let body = organism.add_component("small_body_1", &registry).unwrap();
        let sensor = organism
            .add_component("photosensor_organ_1", &registry)
            .unwrap();
        let reaction = organism
            .add_component("reaction_structure_1", &registry)
            .unwrap();

        organism.connect(body, sensor, &registry).unwrap();
        organism.connect(body, reaction, &registry).unwrap();
        assert_eq!(organism.connections.len(), 2);
        assert_eq!(organism.connection_count(body), 2);
        assert_eq!(organism.connection_count(sensor), 1);
    }

    #[test]
    fn rejects_connections_that_exceed_slots() {
        let registry = create_default_component_registry().unwrap();
        let mut organism = BiologicalOrganism::new(OrganismProgramId::new());
        let sensor = organism
            .add_component("photosensor_organ_1", &registry)
            .unwrap();
        let body = organism.add_component("small_body_1", &registry).unwrap();
        let other_body = organism.add_component("small_body_1", &registry).unwrap();

        organism.connect(sensor, body, &registry).unwrap();
        assert!(matches!(
            organism.connect(sensor, other_body, &registry),
            Err(OrganismError::NoAvailableSlot { .. })
        ));
    }

    #[test]
    fn counts_each_slot_category_independently() {
        let registry = create_default_component_registry().unwrap();
        let mut organism = BiologicalOrganism::new(OrganismProgramId::new());
        let body = organism.add_component("small_body_1", &registry).unwrap();
        let external = (0..4)
            .map(|_| {
                organism
                    .add_component("photosensor_organ_1", &registry)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let internal = (0..3)
            .map(|_| {
                organism
                    .add_component("reaction_structure_1", &registry)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        for instance in external {
            organism.connect(body, instance, &registry).unwrap();
        }
        for instance in internal {
            organism.connect(body, instance, &registry).unwrap();
        }

        assert_eq!(organism.connection_count(body), 7);
        assert_eq!(
            organism
                .connection_count_for_category(body, ComponentCategory::ExternalOrgan, &registry,)
                .unwrap(),
            4
        );
        assert_eq!(
            organism
                .connection_count_for_category(body, ComponentCategory::InternalOrgan, &registry,)
                .unwrap(),
            3
        );
    }
}
