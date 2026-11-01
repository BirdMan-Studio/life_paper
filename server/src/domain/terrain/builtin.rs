//! 内置世界地基注册入口。
//!
//! 地基的地图生成、移动影响和物质生成逻辑不在这里实现；这里只登记稳定 ID
//! 与后续系统读取的规则元数据。

use super::{TerrainRegistrationError, TerrainRegistry, TerrainVisual};

fn visual(registry: &mut TerrainRegistry, id: &str, color: &str, pattern: &str) {
    registry.set_visual(
        id,
        TerrainVisual {
            color: color.to_owned(),
            pattern: pattern.to_owned(),
        },
    );
}

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
    registry.register("pond", "水潭", true, false, true, None)?;
    registry.register("deep_water", "深水", true, false, false, None)?;
    registry.register("wall", "墙", false, true, false, None)?;
    registry.register("cliff", "悬崖", false, true, false, None)?;
    registry.register("valley", "谷地", true, false, true, None)?;
    registry.register("swamp", "沼泽", true, false, true, None)?;
    registry.register("wetland", "湿地", true, false, true, None)?;

    visual(&mut registry, "wasteland", "#4c5560", "dust");
    visual(&mut registry, "grassland", "#6fa85b", "grass");
    visual(&mut registry, "shallow_water", "#3f91a6", "ripples");
    visual(&mut registry, "pond", "#4e9fb0", "ripples");
    visual(&mut registry, "deep_water", "#24516d", "waves");
    visual(&mut registry, "wall", "#7b8791", "blocks");
    visual(&mut registry, "cliff", "#6f5d52", "rock");
    visual(&mut registry, "valley", "#769b7c", "mist");
    visual(&mut registry, "swamp", "#627f62", "mud");
    visual(&mut registry, "wetland", "#7fa866", "reeds");

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_default_terrain_rules() {
        let registry = create_default_terrain_registry().unwrap();
        assert_eq!(registry.len(), 10);

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
