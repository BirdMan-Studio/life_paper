use super::BiologicalOrganism;
use crate::domain::{
    MapPosition,
    element::{ElementId, ElementRegistry},
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InitialContainerContents {
    #[serde(default)]
    pub non_gas_contents_mg: BTreeMap<ElementId, f64>,
    #[serde(default)]
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
    pub fn initialize_containers(
        &mut self,
        contents: BTreeMap<Uuid, InitialContainerContents>,
        elements: &ElementRegistry,
    ) -> Result<(), String> {
        for (id, initial) in &contents {
            let container = &self
                .component_states
                .get(id)
                .ok_or_else(|| format!("unknown container component: {id}"))?
                .container;
            let mut mass = 0.0;
            for (element, amount) in &initial.non_gas_contents_mg {
                if !elements.contains(element.as_str()) || !amount.is_finite() || *amount < 0.0 {
                    return Err(format!("invalid initial substance or mass: {element}"));
                }
                mass += amount;
            }
            if !mass.is_finite() || mass > container.non_gas_capacity_mg as f64 {
                return Err(format!("initial non-gas contents exceed capacity: {id}"));
            }
            let mut volume = 0u64;
            for (element, amount) in &initial.gas_contents_ul {
                if !elements.contains(element.as_str()) {
                    return Err(format!("unknown initial substance: {element}"));
                }
                volume = volume
                    .checked_add(*amount)
                    .ok_or_else(|| format!("initial gas volume overflow: {id}"))?;
            }
            if volume > container.gas_capacity_ul {
                return Err(format!("initial gas contents exceed capacity: {id}"));
            }
        }
        for (id, initial) in contents {
            let container = &mut self
                .component_states
                .get_mut(&id)
                .expect("validated component")
                .container;
            container.non_gas_contents_mg = initial
                .non_gas_contents_mg
                .into_iter()
                .filter(|(_, amount)| *amount > 0.0)
                .collect();
            container.gas_contents_ul = initial
                .gas_contents_ul
                .into_iter()
                .filter(|(_, amount)| *amount > 0)
                .collect();
        }
        Ok(())
    }

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
    fn initializes_and_validates_container_contents_atomically() {
        let registry = create_default_component_registry().unwrap();
        let elements = crate::domain::create_default_element_registry().unwrap();
        let mut model = BiologicalOrganism::new(OrganismProgramId::new());
        let body = model.add_component("small_body_1", &registry).unwrap();
        let mut instance =
            OrganismInstance::from_model(model.id, 7, "test".into(), model, &registry).unwrap();
        let contents = |mass: f64, gas: u64| {
            BTreeMap::from([(
                body,
                InitialContainerContents {
                    non_gas_contents_mg: BTreeMap::from([(
                        ElementId::new("monosaccharide").unwrap(),
                        mass,
                    )]),
                    gas_contents_ul: BTreeMap::from([(ElementId::new("oxygen").unwrap(), gas)]),
                },
            )])
        };
        instance
            .initialize_containers(contents(1999.5, 20_000), &elements)
            .unwrap();
        let before = instance.clone();
        for mass in [2000.5, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                instance
                    .initialize_containers(contents(mass, 0), &elements)
                    .is_err()
            );
            assert_eq!(instance, before);
        }
        assert!(
            instance
                .initialize_containers(contents(0.0, 20_001), &elements)
                .is_err()
        );
        let mut invalid = contents(0.0, 0);
        invalid.get_mut(&body).unwrap().gas_contents_ul = BTreeMap::from([
            (ElementId::new("oxygen").unwrap(), u64::MAX),
            (ElementId::new("nitrogen").unwrap(), 1),
        ]);
        assert!(instance.initialize_containers(invalid, &elements).is_err());
        let mut invalid = contents(0.0, 0);
        invalid
            .get_mut(&body)
            .unwrap()
            .non_gas_contents_mg
            .insert(ElementId::new("unknown_material").unwrap(), 1.0);
        assert!(instance.initialize_containers(invalid, &elements).is_err());
        let mut invalid = contents(0.0, 0);
        invalid.insert(Uuid::new_v4(), InitialContainerContents::default());
        assert!(instance.initialize_containers(invalid, &elements).is_err());
        assert_eq!(instance, before);
        let restored: OrganismInstance =
            serde_json::from_value(serde_json::to_value(&instance).unwrap()).unwrap();
        assert_eq!(restored, instance);
        assert_eq!(
            instance.component_states[&body]
                .container
                .non_gas_contents_mg["monosaccharide"],
            1999.5
        );
        instance
            .initialize_containers(contents(0.0, 0), &elements)
            .unwrap();
        assert!(
            instance.component_states[&body]
                .container
                .non_gas_contents_mg
                .is_empty()
        );
        assert!(
            instance.component_states[&body]
                .container
                .gas_contents_ul
                .is_empty()
        );
    }

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
