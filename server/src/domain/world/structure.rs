use crate::domain::terrain::TerrainId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapPosition {
    pub x: i32,
    pub y: i32,
}

/// 一个紧凑的地基覆盖区域，不按像素展开存储。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum TerrainArea {
    Rectangle {
        origin: MapPosition,
        size_x_px: u32,
        size_y_px: u32,
    },
    Circle {
        center: MapPosition,
        radius_px: u32,
    },
    RectangleFrame {
        origin: MapPosition,
        size_x_px: u32,
        size_y_px: u32,
        thickness_px: u32,
    },
}

impl TerrainArea {
    pub fn contains(&self, position: MapPosition) -> bool {
        match *self {
            Self::Rectangle {
                origin,
                size_x_px,
                size_y_px,
            } => rectangle_contains(origin, size_x_px, size_y_px, position),
            Self::Circle { center, radius_px } => {
                let delta_x = i128::from(position.x) - i128::from(center.x);
                let delta_y = i128::from(position.y) - i128::from(center.y);
                let radius = i128::from(radius_px);
                delta_x * delta_x + delta_y * delta_y <= radius * radius
            }
            Self::RectangleFrame {
                origin,
                size_x_px,
                size_y_px,
                thickness_px,
            } => {
                if !rectangle_contains(origin, size_x_px, size_y_px, position) {
                    return false;
                }

                let local_x = i64::from(position.x) - i64::from(origin.x);
                let local_y = i64::from(position.y) - i64::from(origin.y);
                let far_x = i64::from(size_x_px) - 1 - local_x;
                let far_y = i64::from(size_y_px) - 1 - local_y;
                let thickness = i64::from(thickness_px);
                local_x < thickness || local_y < thickness || far_x < thickness || far_y < thickness
            }
        }
    }
}

fn rectangle_contains(
    origin: MapPosition,
    size_x_px: u32,
    size_y_px: u32,
    position: MapPosition,
) -> bool {
    let local_x = i64::from(position.x) - i64::from(origin.x);
    let local_y = i64::from(position.y) - i64::from(origin.y);
    local_x >= 0 && local_y >= 0 && local_x < i64::from(size_x_px) && local_y < i64::from(size_y_px)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerrainLayer {
    pub terrain: TerrainId,
    pub area: TerrainArea,
}

/// 一个逻辑上无限的世界。
///
/// 未被任何区域覆盖的坐标使用 `default_terrain`。区域按写入顺序覆盖，后写入者优先。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct World {
    default_terrain: TerrainId,
    terrain_layers: Vec<TerrainLayer>,
}

impl World {
    pub(crate) fn new(default_terrain: TerrainId, terrain_layers: Vec<TerrainLayer>) -> Self {
        Self {
            default_terrain,
            terrain_layers,
        }
    }

    pub fn terrain_at(&self, position: MapPosition) -> &TerrainId {
        self.terrain_layers
            .iter()
            .rev()
            .find(|layer| layer.area.contains(position))
            .map_or(&self.default_terrain, |layer| &layer.terrain)
    }

    pub fn default_terrain(&self) -> &TerrainId {
        &self.default_terrain
    }

    pub fn terrain_layers(&self) -> &[TerrainLayer] {
        &self.terrain_layers
    }
}
