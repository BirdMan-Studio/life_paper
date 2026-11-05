//! 内置物质注册入口。
//!
//! 新增或调整内置物质时，通常只需要修改这个文件。物质的结构和注册 API
//! 分别位于 `definition.rs` 与 `registry.rs`。

use super::{
    ElementCategory, ElementPhysicalProperties, ElementRegistrationError, ElementRegistry,
    PhaseArealDensity,
};

/// 创建包含全部内置物质的注册表。
///
/// 可以在返回的注册表上继续调用 `register`，全部注册完成后再放入共享状态。
pub fn create_default_element_registry() -> Result<ElementRegistry, ElementRegistrationError> {
    let mut registry = ElementRegistry::new();

    registry.register(
        "carbon",
        "碳",
        "C",
        ElementCategory::Basic,
        ElementPhysicalProperties::new(
            PhaseArealDensity::new(Some(2.267), None, None),
            3550.0,
            None,
        ),
    )?;

    registry.register(
        "hydrogen",
        "氢",
        "H",
        ElementCategory::Basic,
        ElementPhysicalProperties::new(
            PhaseArealDensity::new(Some(0.086), Some(0.0708), Some(0.0000899)),
            -259.16,
            Some(-252.87),
        ),
    )?;

    registry.register(
        "oxygen",
        "氧",
        "O",
        ElementCategory::Basic,
        ElementPhysicalProperties::new(
            PhaseArealDensity::new(Some(1.426), Some(1.141), Some(0.001429)),
            -218.79,
            Some(-182.95),
        ),
    )?;

    registry.register(
        "carbon_dioxide",
        "二氧化碳",
        "CO2",
        ElementCategory::Compound,
        ElementPhysicalProperties {
            // Atmospheric sublimation is not represented by melting/boiling fields.
            areal_density: PhaseArealDensity::new(Some(1.56), None, Some(0.001977)),
            melting_point_celsius: None,
            boiling_point_celsius: None,
        },
    )?;

    registry.register(
        "lipid",
        "通用脂类",
        "C18H36O2",
        ElementCategory::Compound,
        ElementPhysicalProperties {
            areal_density: PhaseArealDensity::new(Some(0.95), Some(0.92), None),
            melting_point_celsius: None,
            boiling_point_celsius: None,
        },
    )?;
    registry.set_initial_processing_speed("lipid", 0.25)?;

    registry.register(
        "water",
        "水",
        "H2O",
        ElementCategory::Compound,
        ElementPhysicalProperties::new(
            PhaseArealDensity::new(Some(0.917), Some(0.997), Some(0.000804)),
            0.0,
            Some(100.0),
        ),
    )?;

    registry.register(
        "monosaccharide",
        "单糖",
        "CH2O",
        ElementCategory::Compound,
        ElementPhysicalProperties::new(
            PhaseArealDensity::new(Some(1.54), Some(1.50), None),
            146.0,
            None,
        ),
    )?;

    for (id, name, symbol, density, speed) in [
        ("disaccharide", "二糖", "C12H22O11", 1.59, 0.8),
        ("polysaccharide", "多糖", "(C6H10O5)n", 1.50, 0.5),
        ("cellulose", "纤维素", "(C6H10O5)n", 1.50, 0.1),
    ] {
        registry.register(
            id,
            name,
            symbol,
            ElementCategory::Compound,
            ElementPhysicalProperties {
                areal_density: PhaseArealDensity::new(Some(density), None, None),
                melting_point_celsius: None,
                boiling_point_celsius: None,
            },
        )?;
        registry.set_initial_processing_speed(id, speed)?;
    }

    registry.register(
        "nitrogen",
        "氮气",
        "N2",
        ElementCategory::Basic,
        ElementPhysicalProperties::new(
            PhaseArealDensity::new(Some(1.026), Some(0.808), Some(0.001251)),
            -210.0,
            Some(-195.79),
        ),
    )?;

    registry.register(
        "ammonia",
        "氨气",
        "NH3",
        ElementCategory::Compound,
        ElementPhysicalProperties::new(
            PhaseArealDensity::new(Some(0.817), Some(0.682), Some(0.000771)),
            -77.73,
            Some(-33.34),
        ),
    )?;

    registry.register(
        "urea",
        "尿素",
        "CH4N2O",
        ElementCategory::Compound,
        ElementPhysicalProperties {
            areal_density: PhaseArealDensity::new(Some(1.32), None, None),
            // Urea decomposes on heating; no stable liquid or gas is modeled.
            melting_point_celsius: None,
            boiling_point_celsius: None,
        },
    )?;

    for (id, name, symbol, speed) in [
        ("structural_protein", "结构蛋白", "C12H19N3O4", 0.30),
        ("membrane_protein", "膜蛋白", "C15H25N3O4", 0.40),
        ("contractile_protein", "收缩蛋白", "C12H20N4O5", 0.35),
        ("sensory_protein", "感知蛋白", "C14H21N5O5", 0.20),
        ("keratin_protein", "角质蛋白", "C10H16N2O3", 0.10),
        ("enzyme_protein", "酶蛋白", "C10H16N4O5", 0.50),
    ] {
        registry.register(
            id,
            name,
            symbol,
            ElementCategory::Compound,
            ElementPhysicalProperties {
                areal_density: PhaseArealDensity::new(Some(1.35), None, None),
                melting_point_celsius: None,
                boiling_point_celsius: None,
            },
        )?;
        registry.set_initial_processing_speed(id, speed)?;
    }

    use super::chemistry::{
        Atom::{C, H, N, O},
        ChemicalProperties,
    };
    // Scale all potentials together to balance chemistry against organ energy costs.
    const CHEMICAL_ENERGY_SCALE: f64 = 2.0;
    // Signed reference potentials relative to elemental substances.
    // Empirical and polymer repeat units are sufficient for mass-based conservation.
    for (id, atoms, energy) in [
        ("carbon", vec![(C, 1)], 0.0),
        ("hydrogen", vec![(H, 2)], 0.0),
        ("oxygen", vec![(O, 2)], 0.0),
        ("carbon_dioxide", vec![(C, 1), (O, 2)], -9.0),
        ("nitrogen", vec![(N, 2)], 0.0),
        ("water", vec![(H, 2), (O, 1)], -16.0),
        ("monosaccharide", vec![(C, 1), (H, 2), (O, 1)], -7.0),
        ("disaccharide", vec![(C, 12), (H, 22), (O, 11)], -6.5),
        ("polysaccharide", vec![(C, 6), (H, 10), (O, 5)], -5.5),
        ("cellulose", vec![(C, 6), (H, 10), (O, 5)], -5.5),
        ("ammonia", vec![(N, 1), (H, 3)], -2.7),
        ("urea", vec![(C, 1), (H, 4), (N, 2), (O, 1)], -5.5),
        ("lipid", vec![(C, 18), (H, 36), (O, 2)], -3.3),
        (
            "structural_protein",
            vec![(C, 12), (H, 19), (N, 3), (O, 4)],
            -5.0,
        ),
        (
            "membrane_protein",
            vec![(C, 15), (H, 25), (N, 3), (O, 4)],
            -5.0,
        ),
        (
            "contractile_protein",
            vec![(C, 12), (H, 20), (N, 4), (O, 5)],
            -5.0,
        ),
        (
            "sensory_protein",
            vec![(C, 14), (H, 21), (N, 5), (O, 5)],
            -5.0,
        ),
        (
            "keratin_protein",
            vec![(C, 10), (H, 16), (N, 2), (O, 3)],
            -5.0,
        ),
        (
            "enzyme_protein",
            vec![(C, 10), (H, 16), (N, 4), (O, 5)],
            -5.0,
        ),
    ] {
        registry.set_chemistry(
            id,
            ChemicalProperties::new(&atoms, energy * CHEMICAL_ENERGY_SCALE),
        )?;
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::element::Phase;

    #[test]
    fn registers_builtin_elements_with_physical_properties() {
        let registry = create_default_element_registry().unwrap();
        assert_eq!(registry.len(), 19);
        assert!(
            registry
                .iter()
                .all(|element| element.chemistry.as_ref().is_some_and(|p| p.is_valid()))
        );

        let carbon = registry.get("carbon").unwrap();
        assert_eq!(carbon.category, ElementCategory::Basic);
        assert_eq!(carbon.symbol, "C");

        let water = registry.get("water").unwrap();
        assert_eq!(water.name, "水");
        assert_eq!(water.areal_density(Phase::Solid), Some(0.917));
        assert_eq!(water.physical.melting_point_celsius, Some(0.0));
        assert_eq!(water.physical.boiling_point_celsius, Some(100.0));

        let carbon_dioxide = registry.get("carbon_dioxide").unwrap();
        assert_eq!(carbon_dioxide.name, "二氧化碳");
        assert_eq!(carbon_dioxide.symbol, "CO2");
        assert_eq!(carbon_dioxide.areal_density(Phase::Gas), Some(0.001977));

        let lipid = registry.get("lipid").unwrap();
        assert_eq!(lipid.name, "通用脂类");
        assert_eq!(lipid.initial_processing_speed, 0.25);

        let sugar = registry.get("monosaccharide").unwrap();
        assert_eq!(sugar.areal_density(Phase::Gas), None);
        assert_eq!(sugar.physical.boiling_point_celsius, None);
        assert_eq!(sugar.initial_processing_speed, 1.0);
        for (id, name, symbol, category, solid, liquid, gas, melting, boiling) in [
            (
                "nitrogen",
                "氮气",
                "N2",
                ElementCategory::Basic,
                1.026,
                Some(0.808),
                Some(0.001251),
                Some(-210.0),
                Some(-195.79),
            ),
            (
                "ammonia",
                "氨气",
                "NH3",
                ElementCategory::Compound,
                0.817,
                Some(0.682),
                Some(0.000771),
                Some(-77.73),
                Some(-33.34),
            ),
            (
                "urea",
                "尿素",
                "CH4N2O",
                ElementCategory::Compound,
                1.32,
                None,
                None,
                None,
                None,
            ),
        ] {
            let element = registry.get(id).unwrap();
            assert_eq!(element.name, name);
            assert_eq!(element.symbol, symbol);
            assert_eq!(element.category, category);
            assert_eq!(element.initial_processing_speed, 1.0);
            assert_eq!(element.areal_density(Phase::Solid), Some(solid));
            assert_eq!(element.areal_density(Phase::Liquid), liquid);
            assert_eq!(element.areal_density(Phase::Gas), gas);
            assert_eq!(element.physical.melting_point_celsius, melting);
            assert_eq!(element.physical.boiling_point_celsius, boiling);
        }
        for (id, name, density, speed) in [
            ("disaccharide", "二糖", 1.59, 0.8),
            ("polysaccharide", "多糖", 1.50, 0.5),
            ("cellulose", "纤维素", 1.50, 0.1),
        ] {
            let element = registry.get(id).unwrap();
            assert_eq!(element.name, name);
            assert_eq!(element.category, ElementCategory::Compound);
            assert_eq!(element.initial_processing_speed, speed);
            assert_eq!(element.areal_density(Phase::Solid), Some(density));
            assert_eq!(element.areal_density(Phase::Liquid), None);
            assert_eq!(element.areal_density(Phase::Gas), None);
            assert_eq!(element.physical.melting_point_celsius, None);
            assert_eq!(element.physical.boiling_point_celsius, None);
        }
    }

    #[test]
    fn processing_speed_validates_and_legacy_definitions_load() {
        let mut registry = create_default_element_registry().unwrap();
        for invalid in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
            assert!(
                registry
                    .set_initial_processing_speed("cellulose", invalid)
                    .is_err()
            );
            assert_eq!(
                registry.get("cellulose").unwrap().initial_processing_speed,
                0.1
            );
        }
        assert!(
            registry
                .set_initial_processing_speed("missing", 1.0)
                .is_err()
        );
        registry
            .set_initial_processing_speed("cellulose", 0.0)
            .unwrap();
        registry
            .set_initial_processing_speed("cellulose", 1.0)
            .unwrap();

        let mut legacy = serde_json::to_value(registry.get("water").unwrap()).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("initial_processing_speed");
        let restored: super::super::ElementDefinition = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.initial_processing_speed, 1.0);
        assert_eq!(restored.physical.melting_point_celsius, Some(0.0));
        let restored: ElementRegistry =
            serde_json::from_slice(&serde_json::to_vec(&registry).unwrap()).unwrap();
        assert_eq!(restored, registry);
    }
}
