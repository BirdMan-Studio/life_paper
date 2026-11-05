use super::BiologicalOrganism;
use crate::domain::{
    MapPosition,
    element::ElementId,
    organism_component::{ComponentProperties, ComponentRegistry},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ContainerState {
    pub non_gas_capacity_mg: u64,
    pub gas_capacity_ul: u64,
    pub non_gas_contents_mg: BTreeMap<ElementId, f64>,
    pub gas_contents_ul: BTreeMap<ElementId, u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ComponentState {
    pub properties: ComponentProperties,
    pub durability: u32,
    pub energy_mj: u64,
    pub container: ContainerState,
}

/// A model snapshot and independent mutable state for one organism in a world.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OrganismInstance {
    pub id: Uuid,
    pub model_id: Uuid,
    pub owner_user_id: i64,
    pub name: String,
    pub position: MapPosition,
    pub rotation_mdeg: i32,
    #[serde(default)]
    pub motion: Option<crate::game::dynamics::MotionState>,
    pub composition: BiologicalOrganism,
    pub component_states: BTreeMap<Uuid, ComponentState>,
    #[serde(default)]
    pub ai: super::AiState,
}

impl OrganismInstance {
    pub fn from_model(
        model_id: Uuid,
        owner_user_id: i64,
        name: String,
        mut composition: BiologicalOrganism,
        registry: &ComponentRegistry,
    ) -> Result<Self, String> {
        if composition.components.is_empty() || composition.components.len() > 256 {
            return Err("invalid component count".into());
        }
        crate::domain::organism_component::normalize_placements(&mut composition, registry)?;
        let root = composition
            .root_body
            .and_then(|id| composition.components.get(&id))
            .filter(|part| {
                registry
                    .get(part.component.as_str())
                    .is_some_and(|definition| {
                        definition.category == crate::domain::ComponentCategory::Body
                    })
            })
            .ok_or("model has no root body")?;
        let origin = root.placement.position.unwrap_or_default();
        for part in composition.components.values_mut() {
            if let Some(position) = &mut part.placement.position {
                position.x = position
                    .x
                    .checked_sub(origin.x)
                    .ok_or("model coordinate overflow")?;
                position.y = position
                    .y
                    .checked_sub(origin.y)
                    .ok_or("model coordinate overflow")?;
            }
        }
        let component_states = composition
            .components
            .iter()
            .map(|(id, part)| {
                let properties = registry
                    .get(part.component.as_str())
                    .expect("normalized component")
                    .properties
                    .clone();
                (
                    *id,
                    ComponentState {
                        durability: properties.max_durability,
                        energy_mj: properties.energy_capacity_mj,
                        container: ContainerState {
                            non_gas_capacity_mg: properties.non_gas_capacity_mg,
                            gas_capacity_ul: properties.gas_capacity_ul,
                            ..ContainerState::default()
                        },
                        properties,
                    },
                )
            })
            .collect();
        let id = Uuid::new_v4();
        composition.id = id;
        composition.program_id = super::OrganismProgramId::from_uuid(id);
        Ok(Self {
            id,
            model_id,
            owner_user_id,
            name,
            position: MapPosition::default(),
            rotation_mdeg: 0,
            motion: None,
            composition,
            component_states,
            ai: Default::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrganismProgramId, create_default_component_registry};

    #[test]
    fn spawns_independent_full_durability_empty_containers() {
        let registry = create_default_component_registry().unwrap();
        let mut model = BiologicalOrganism::new(OrganismProgramId::new());
        let body = model.add_component("small_body_1", &registry).unwrap();
        let reaction = model
            .add_component("reaction_structure_1", &registry)
            .unwrap();
        let gas = model.add_component("gas_chamber_1", &registry).unwrap();
        model.connect(body, reaction, &registry).unwrap();
        model.connect(body, gas, &registry).unwrap();
        model.components.get_mut(&body).unwrap().placement.position =
            Some(MapPosition { x: 100, y: -40 });
        let before = model.clone();
        let mut first =
            OrganismInstance::from_model(model.id, 7, "test".into(), model.clone(), &registry)
                .unwrap();
        let second =
            OrganismInstance::from_model(model.id, 7, "test".into(), model.clone(), &registry)
                .unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.composition.id, first.id);
        assert_eq!(first.composition.program_id.clone().as_uuid(), first.id);
        assert_ne!(first.composition.program_id, second.composition.program_id);
        assert_ne!(first.composition.program_id, model.program_id);
        assert_eq!(first.position, MapPosition::default());
        assert_eq!(first.rotation_mdeg, 0);
        assert_eq!(
            first.composition.components[&body].placement.position,
            Some(MapPosition::default())
        );
        assert_eq!(first.component_states[&body].durability, 100);
        assert_eq!(
            first.component_states[&body].container.non_gas_capacity_mg,
            2_000
        );
        assert_eq!(
            first.component_states[&body].container.gas_capacity_ul,
            20_000
        );
        assert_eq!(
            first.component_states[&reaction]
                .container
                .non_gas_capacity_mg,
            20_000
        );
        assert_eq!(
            first.component_states[&gas].container.gas_capacity_ul,
            40_000
        );
        assert!(first.component_states.values().all(|state| state.energy_mj
            == state.properties.energy_capacity_mj
            && state.container.non_gas_contents_mg.is_empty()
            && state.container.gas_contents_ul.is_empty()));
        first.component_states.get_mut(&body).unwrap().durability = 1;
        assert_eq!(second.component_states[&body].durability, 100);
        assert_eq!(model, before);
        let restored: OrganismInstance =
            serde_json::from_value(serde_json::to_value(&first).unwrap()).unwrap();
        assert_eq!(restored, first);
    }
}
