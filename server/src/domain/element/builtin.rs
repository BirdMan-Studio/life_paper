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

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::element::Phase;

    #[test]
    fn registers_builtin_elements_with_physical_properties() {
        let registry = create_default_element_registry().unwrap();
        assert_eq!(registry.len(), 5);

        let carbon = registry.get("carbon").unwrap();
        assert_eq!(carbon.category, ElementCategory::Basic);
        assert_eq!(carbon.symbol, "C");

        let water = registry.get("water").unwrap();
        assert_eq!(water.name, "水");
        assert_eq!(water.areal_density(Phase::Solid), Some(0.917));
        assert_eq!(water.physical.melting_point_celsius, 0.0);
        assert_eq!(water.physical.boiling_point_celsius, Some(100.0));

        let sugar = registry.get("monosaccharide").unwrap();
        assert_eq!(sugar.areal_density(Phase::Gas), None);
        assert_eq!(sugar.physical.boiling_point_celsius, None);
    }
}
