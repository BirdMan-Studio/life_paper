//! 内置生物组件注册入口。
//!
//! Component catalog, including authoritative geometry and presentation data.
//! Function codes are registered but not executed here.

use super::{ComponentCategory, ComponentRegistrationError, ComponentRegistry, ComponentSlots};

pub fn create_default_component_registry() -> Result<ComponentRegistry, ComponentRegistrationError>
{
    let mut registry = ComponentRegistry::new();

    registry.register(
        "photosensor_organ_1",
        "感光器官I",
        ComponentCategory::ExternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;
    registry.register(
        "jet_organ_1",
        "喷射器官I",
        ComponentCategory::ExternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;
    registry.register(
        "absorption_organ_1",
        "吸收器官I",
        ComponentCategory::ExternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;
    registry.register(
        "skin_1",
        "皮肤I",
        ComponentCategory::ExternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;
    registry.register(
        "small_body_1",
        "小型躯体I",
        ComponentCategory::Body,
        ComponentSlots::new()
            .with(ComponentCategory::ExternalOrgan, 4)
            .with(ComponentCategory::InternalOrgan, 3),
        None,
    )?;
    registry.register(
        "reaction_structure_1",
        "反应结构I",
        ComponentCategory::InternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;
    registry.register(
        "sensor_group_1",
        "传感器组I",
        ComponentCategory::InternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;
    registry.register(
        "gas_chamber_1",
        "气室结构I",
        ComponentCategory::InternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;

    use super::{Assembly, ComponentModel, Geometry};
    let mut sensor = ComponentModel::for_category(ComponentCategory::ExternalOrgan);
    sensor.size.length_px = 10;
    registry.set_model("photosensor_organ_1", sensor)?;
    let mut absorption = ComponentModel::for_category(ComponentCategory::ExternalOrgan);
    absorption.appearance.range_radius_px = Some(5);
    registry.set_model("absorption_organ_1", absorption)?;
    let mut skin = ComponentModel::for_category(ComponentCategory::ExternalOrgan);
    skin.geometry = Geometry::HostOutline;
    skin.assembly = Assembly::Wrap;
    skin.appearance.stroke = "#7eab86".into();
    registry.set_model("skin_1", skin)?;
    let mut reaction = ComponentModel::for_category(ComponentCategory::InternalOrgan);
    reaction.geometry = Geometry::Rectangle;
    registry.set_model("reaction_structure_1", reaction)?;
    for (id, fill, stroke) in [
        ("sensor_group_1", "#253e62", "#7099d0"),
        ("gas_chamber_1", "#2b4b5d", "#74b7c9"),
    ] {
        let mut model = ComponentModel::for_category(ComponentCategory::InternalOrgan);
        model.appearance.fill = fill.into();
        model.appearance.stroke = stroke.into();
        registry.set_model(id, model)?;
    }
    use super::ComponentProperties;
    for (
        id,
        durability,
        damage,
        repair,
        mass_mg,
        energy,
        non_gas,
        gas,
        reaction_min,
        reaction_max,
        sensors,
    ) in [
        (
            "small_body_1",
            100,
            10,
            10,
            Some(40_000),
            1_000,
            2_000,
            20_000,
            -10,
            2,
            0,
        ),
        ("jet_organ_1", 50, 40, 10, Some(5_000), 0, 0, 0, 0, 0, 0),
        (
            "absorption_organ_1",
            50,
            40,
            10,
            Some(5_000),
            0,
            0,
            0,
            0,
            0,
            0,
        ),
        ("skin_1", 200, 100, 20, Some(2_000), 0, 0, 0, 0, 0, 0),
        (
            "photosensor_organ_1",
            10,
            40,
            10,
            Some(5_000),
            0,
            0,
            0,
            0,
            0,
            0,
        ),
        (
            "reaction_structure_1",
            10,
            2,
            1,
            None,
            0,
            20_000,
            0,
            -50,
            10,
            0,
        ),
        ("sensor_group_1", 20, 2, 1, None, 0, 0, 0, 0, 0, 2),
        ("gas_chamber_1", 10, 2, 1, None, 0, 0, 40_000, 0, 0, 0),
    ] {
        registry.set_properties(
            id,
            ComponentProperties {
                api: Some(super::builtin_component_api(id)),
                max_durability: durability,
                damage_weight: damage,
                repair_weight: repair,
                mass_mg,
                energy_capacity_mj: energy,
                non_gas_capacity_mg: non_gas,
                gas_capacity_ul: gas,
                reaction_min_mj_per_tick: reaction_min,
                reaction_max_mj_per_tick: reaction_max,
                sensor_slots: sensors,
            },
        )?;
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_distributes_complete_models() {
        let registry = create_default_component_registry().unwrap();
        for definition in registry.iter() {
            assert!(definition.model.valid_for(definition.category));
            let json = serde_json::to_value(definition).unwrap();
            assert!(json["model"]["geometry"].is_string());
            assert!(json["model"]["collision"].is_string());
            let restored: super::super::ComponentDefinition = serde_json::from_value(json).unwrap();
            assert_eq!(&restored, definition);
        }
        assert_eq!(
            registry
                .get("absorption_organ_1")
                .unwrap()
                .model
                .appearance
                .range_radius_px,
            Some(5)
        );
    }

    #[test]
    fn registers_builtin_components_and_connection_slots() {
        let registry = create_default_component_registry().unwrap();
        assert_eq!(registry.len(), 8);

        let photosensor = registry.get("photosensor_organ_1").unwrap();
        assert!(photosensor.unlock.default_unlocked);
        assert_eq!(photosensor.category, ComponentCategory::ExternalOrgan);
        assert_eq!(photosensor.slots.count(ComponentCategory::Body), 1);
        assert_eq!(photosensor.slots.total(), 1);

        let body = registry.get("small_body_1").unwrap();
        assert_eq!(body.category, ComponentCategory::Body);
        assert_eq!(body.slots.count(ComponentCategory::ExternalOrgan), 4);
        assert_eq!(body.slots.count(ComponentCategory::InternalOrgan), 3);
        assert_eq!(body.slots.total(), 7);

        let reaction = registry.get("reaction_structure_1").unwrap();
        assert_eq!(reaction.category, ComponentCategory::InternalOrgan);
        assert_eq!(reaction.slots.count(ComponentCategory::Body), 1);
        assert_eq!(reaction.function_code, None);

        for component_id in [
            "jet_organ_1",
            "absorption_organ_1",
            "skin_1",
            "sensor_group_1",
            "gas_chamber_1",
        ] {
            assert!(registry.get(component_id).is_some());
        }
    }
}
