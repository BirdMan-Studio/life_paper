//! 内置世界地基注册入口。
//!
//! 地基的地图生成、移动影响和物质生成逻辑不在这里实现；这里只登记稳定 ID
//! 与后续系统读取的规则元数据。

use super::{TerrainRegistrationError, TerrainRegistry};

pub fn create_default_terrain_registry() -> Result<TerrainRegistry, TerrainRegistrationError> {
    let mut registry = TerrainRegistry::new();

    registry.register("wasteland", "荒地", false, false, false, None)?;
    registry.register(
        "grassland",
        "草地",
        false,
        false,
        true,
        Some("grass_matter"),
    )?;
    registry.register("shallow_water", "潜水", true, false, true, None)?;
    registry.register("deep_water", "深水", true, false, false, None)?;
    registry.register("wall", "墙", false, true, false, None)?;

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_default_terrain_rules() {
        let registry = create_default_terrain_registry().unwrap();
        assert_eq!(registry.len(), 5);

        let wasteland = registry.get("wasteland").unwrap();
        assert!(!wasteland.slows_movement);
        assert!(!wasteland.blocks_movement);
        assert!(!wasteland.generates_oxygen);
        assert_eq!(wasteland.generated_matter_recipe, None);

        let grassland = registry.get("grassland").unwrap();
        assert_eq!(
            grassland.generated_matter_recipe.as_deref(),
            Some("grass_matter")
        );

        let shallow_water = registry.get("shallow_water").unwrap();
        assert!(shallow_water.slows_movement);
        assert!(shallow_water.generates_oxygen);

        let deep_water = registry.get("deep_water").unwrap();
        assert!(deep_water.slows_movement);
        assert!(!deep_water.generates_oxygen);

        let wall = registry.get("wall").unwrap();
        assert!(wall.blocks_movement);
    }
}
