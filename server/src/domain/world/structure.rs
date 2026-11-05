use crate::domain::terrain::TerrainId;
use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapPosition {
    pub x: i32,
    pub y: i32,
}

/// 连续的地层属性。使用百分比整数便于存档、网络传输和后续规则调整。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GroundProperties {
    pub height: u8,
    pub moisture: u8,
    pub nutrients: u8,
    pub temperature: u8,
}

/// 地层分类所需的地形 ID。规则数据随世界保存，因此更换注册表或调参不会改变旧世界。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GroundTerrainProfile {
    pub cliff: TerrainId,
    pub valley: TerrainId,
    pub swamp: TerrainId,
    pub pond: TerrainId,
    pub grassland: TerrainId,
    pub wetland: TerrainId,
    pub wall: TerrainId,
}

impl Default for GroundTerrainProfile {
    fn default() -> Self {
        Self {
            cliff: TerrainId::new("cliff").expect("built-in terrain id"),
            valley: TerrainId::new("valley").expect("built-in terrain id"),
            swamp: TerrainId::new("swamp").expect("built-in terrain id"),
            pond: TerrainId::new("pond").expect("built-in terrain id"),
            grassland: TerrainId::new("grassland").expect("built-in terrain id"),
            wetland: TerrainId::new("wetland").expect("built-in terrain id"),
            wall: TerrainId::new("wall").expect("built-in terrain id"),
        }
    }
}

/// 地层生成参数。所有字段都是公开的，世界类型可以直接替换噪声尺度、种子和分类规则。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GroundGenerationConfig {
    pub seed: u64,
    pub noise_scale_px: u32,
    pub variation_percent: u8,
    pub profile: GroundTerrainProfile,
    pub area: Option<TerrainArea>,
}

impl Default for GroundGenerationConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            noise_scale_px: 384,
            variation_percent: 100,
            profile: GroundTerrainProfile::default(),
            area: None,
        }
    }
}

impl GroundGenerationConfig {
    pub fn perlin_permutations(&self) -> Vec<Vec<u8>> {
        use noise::permutationtable::{NoiseHasher, PermutationTable};
        (1..=4)
            .map(|channel| {
                let table = PermutationTable::new(self.seed.wrapping_add(channel) as u32);
                (0..256).map(|index| table.hash(&[index]) as u8).collect()
            })
            .collect()
    }

    pub fn classify(&self, properties: GroundProperties) -> &TerrainId {
        let h = properties.height;
        let m = properties.moisture;
        if h <= 10 {
            if m <= 90 {
                &self.profile.cliff
            } else {
                &self.profile.pond
            }
        } else if h <= 30 {
            if m <= 70 {
                &self.profile.valley
            } else if m <= 90 {
                &self.profile.swamp
            } else {
                &self.profile.pond
            }
        } else if h <= 95 {
            if m <= 70 {
                &self.profile.grassland
            } else {
                &self.profile.wetland
            }
        } else {
            &self.profile.wall
        }
    }
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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct World {
    #[serde(default)]
    pub matter: super::WorldMatter,
    #[serde(default)]
    pub ai_full_visibility: bool,
    #[serde(default)]
    pub organisms:
        std::collections::BTreeMap<uuid::Uuid, crate::domain::organism::OrganismInstance>,
    default_terrain: TerrainId,
    terrain_layers: Vec<TerrainLayer>,
    #[serde(default)]
    ground: Option<GroundGenerationConfig>,
}

impl World {
    pub(crate) fn new(default_terrain: TerrainId, terrain_layers: Vec<TerrainLayer>) -> Self {
        Self {
            matter: Default::default(),
            ai_full_visibility: false,
            default_terrain,
            organisms: Default::default(),
            terrain_layers,
            ground: None,
        }
    }

    pub(crate) fn with_ground(mut self, ground: GroundGenerationConfig) -> Self {
        self.ground = Some(ground);
        self
    }

    pub fn terrain_at(&self, position: MapPosition) -> &TerrainId {
        if let Some(terrain) = self
            .terrain_layers
            .iter()
            .rev()
            .find(|layer| layer.area.contains(position))
            .map(|layer| &layer.terrain)
        {
            return terrain;
        }
        if let Some(ground) = &self.ground {
            if ground
                .area
                .as_ref()
                .is_some_and(|area| !area.contains(position))
            {
                return &self.default_terrain;
            }
            return ground.classify(self.ground_properties(position));
        }
        &self.default_terrain
    }

    pub fn ground_properties(&self, position: MapPosition) -> GroundProperties {
        self.ground
            .as_ref()
            .map_or(GroundProperties::default(), |config| {
                let sample = |channel: u32| -> u8 {
                    let seed = config.seed.wrapping_add(u64::from(channel)) as u32;
                    let perlin = Perlin::new(seed);
                    let scale = f64::from(config.noise_scale_px.max(1));
                    let value = perlin.get([
                        f64::from(position.x) / scale + f64::from(channel) * 17.13,
                        f64::from(position.y) / scale - f64::from(channel) * 9.71,
                    ]);
                    (((value + 1.0) * 50.0).round().clamp(0.0, 100.0)) as u8
                };
                let vary = config.variation_percent as u16;
                let adjust =
                    |v: u8| ((50 + ((v as i16 - 50) * vary as i16 / 100)).clamp(0, 100)) as u8;
                GroundProperties {
                    height: adjust(sample(1)),
                    moisture: adjust(sample(2)),
                    nutrients: adjust(sample(3)),
                    temperature: adjust(sample(4)),
                }
            })
    }

    pub fn ground_config(&self) -> Option<&GroundGenerationConfig> {
        self.ground.as_ref()
    }

    pub fn default_terrain(&self) -> &TerrainId {
        &self.default_terrain
    }

    pub fn terrain_layers(&self) -> &[TerrainLayer] {
        &self.terrain_layers
    }
}
