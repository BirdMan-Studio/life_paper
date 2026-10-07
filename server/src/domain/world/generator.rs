use super::{MapPosition, World, WorldGenerationError, WorldGeneratorEditor};
use crate::domain::terrain::TerrainRegistry;

pub const BASIC_WORLD_SIZE_PX: u32 = 10_000;
pub const BASIC_WORLD_WATER_RADIUS_PX: u32 = 100;
pub const BASIC_WORLD_WALL_THICKNESS_PX: u32 = 1;

/// 创建基础教程世界：荒地为无限背景，墙内为草地，中央为潜水区域。
pub fn generate_basic_world(terrains: &TerrainRegistry) -> Result<World, WorldGenerationError> {
    let origin = MapPosition { x: 0, y: 0 };
    let center = MapPosition {
        x: (BASIC_WORLD_SIZE_PX / 2) as i32,
        y: (BASIC_WORLD_SIZE_PX / 2) as i32,
    };
    let mut editor = WorldGeneratorEditor::new(terrains, "wasteland")?;

    editor
        .fill_rectangle(
            origin,
            BASIC_WORLD_SIZE_PX,
            BASIC_WORLD_SIZE_PX,
            "grassland",
        )?
        .fill_circle(center, BASIC_WORLD_WATER_RADIUS_PX, "shallow_water")?
        .frame_rectangle(
            origin,
            BASIC_WORLD_SIZE_PX,
            BASIC_WORLD_SIZE_PX,
            BASIC_WORLD_WALL_THICKNESS_PX,
            "wall",
        )?;

    Ok(editor.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::create_default_terrain_registry;

    #[test]
    fn generates_compact_basic_world_with_expected_terrain() {
        let terrains = create_default_terrain_registry().unwrap();
        let world = generate_basic_world(&terrains).unwrap();

        assert_eq!(world.terrain_layers().len(), 3);
        assert_eq!(
            world.terrain_at(MapPosition { x: -1, y: 5 }).as_str(),
            "wasteland"
        );
        assert_eq!(
            world.terrain_at(MapPosition { x: 0, y: 5 }).as_str(),
            "wall"
        );
        assert_eq!(
            world.terrain_at(MapPosition { x: 1, y: 1 }).as_str(),
            "grassland"
        );
        assert_eq!(
            world
                .terrain_at(MapPosition { x: 5_000, y: 5_000 })
                .as_str(),
            "shallow_water"
        );
        assert_eq!(
            world
                .terrain_at(MapPosition { x: 5_101, y: 5_000 })
                .as_str(),
            "grassland"
        );
        assert_eq!(
            world
                .terrain_at(MapPosition { x: 9_999, y: 9_999 })
                .as_str(),
            "wall"
        );
    }
}
