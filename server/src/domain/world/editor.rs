use super::{MapPosition, TerrainArea, TerrainLayer, World};
use crate::domain::terrain::TerrainRegistry;
use std::fmt;

pub struct WorldGeneratorEditor<'a> {
    terrains: &'a TerrainRegistry,
    default_terrain: crate::domain::terrain::TerrainId,
    layers: Vec<TerrainLayer>,
}

impl<'a> WorldGeneratorEditor<'a> {
    pub fn new(
        terrains: &'a TerrainRegistry,
        default_terrain: &str,
    ) -> Result<Self, WorldGenerationError> {
        let default_terrain = registered_terrain(terrains, default_terrain)?;
        Ok(Self {
            terrains,
            default_terrain,
            layers: Vec::new(),
        })
    }

    pub fn fill_rectangle(
        &mut self,
        origin: MapPosition,
        size_x_px: u32,
        size_y_px: u32,
        terrain: &str,
    ) -> Result<&mut Self, WorldGenerationError> {
        validate_rectangle_size(size_x_px, size_y_px)?;
        self.push_layer(
            terrain,
            TerrainArea::Rectangle {
                origin,
                size_x_px,
                size_y_px,
            },
        )
    }

    pub fn fill_circle(
        &mut self,
        center: MapPosition,
        radius_px: u32,
        terrain: &str,
    ) -> Result<&mut Self, WorldGenerationError> {
        self.push_layer(terrain, TerrainArea::Circle { center, radius_px })
    }

    pub fn frame_rectangle(
        &mut self,
        origin: MapPosition,
        size_x_px: u32,
        size_y_px: u32,
        thickness_px: u32,
        terrain: &str,
    ) -> Result<&mut Self, WorldGenerationError> {
        validate_rectangle_size(size_x_px, size_y_px)?;
        if thickness_px == 0 || thickness_px > size_x_px.min(size_y_px) {
            return Err(WorldGenerationError::InvalidFrameThickness {
                size_x_px,
                size_y_px,
                thickness_px,
            });
        }
        self.push_layer(
            terrain,
            TerrainArea::RectangleFrame {
                origin,
                size_x_px,
                size_y_px,
                thickness_px,
            },
        )
    }

    pub fn build(self) -> World {
        World::new(self.default_terrain, self.layers)
    }

    fn push_layer(
        &mut self,
        terrain: &str,
        area: TerrainArea,
    ) -> Result<&mut Self, WorldGenerationError> {
        let terrain = registered_terrain(self.terrains, terrain)?;
        self.layers.push(TerrainLayer { terrain, area });
        Ok(self)
    }
}

fn registered_terrain(
    terrains: &TerrainRegistry,
    terrain: &str,
) -> Result<crate::domain::terrain::TerrainId, WorldGenerationError> {
    terrains
        .get(terrain)
        .map(|definition| definition.id.clone())
        .ok_or_else(|| WorldGenerationError::UnknownTerrain(terrain.to_owned()))
}

fn validate_rectangle_size(size_x_px: u32, size_y_px: u32) -> Result<(), WorldGenerationError> {
    if size_x_px == 0 || size_y_px == 0 {
        Err(WorldGenerationError::InvalidRectangleSize {
            size_x_px,
            size_y_px,
        })
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorldGenerationError {
    UnknownTerrain(String),
    InvalidRectangleSize {
        size_x_px: u32,
        size_y_px: u32,
    },
    InvalidFrameThickness {
        size_x_px: u32,
        size_y_px: u32,
        thickness_px: u32,
    },
}

impl fmt::Display for WorldGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTerrain(terrain) => write!(formatter, "unknown terrain: {terrain}"),
            Self::InvalidRectangleSize {
                size_x_px,
                size_y_px,
            } => write!(
                formatter,
                "world rectangle size must be at least 1x1 px, got {size_x_px}x{size_y_px}"
            ),
            Self::InvalidFrameThickness {
                size_x_px,
                size_y_px,
                thickness_px,
            } => write!(
                formatter,
                "frame thickness {thickness_px} is invalid for {size_x_px}x{size_y_px} px"
            ),
        }
    }
}

impl std::error::Error for WorldGenerationError {}
