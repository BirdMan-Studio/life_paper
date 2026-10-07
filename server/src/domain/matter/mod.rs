mod composition;
mod entity;
mod structure;

pub use crate::domain::world::MapPosition;
pub use composition::ElementMixture;
pub use entity::MatterEntity;
pub use structure::MatterStructure;

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum MatterError {
    EmptyComposition,
    UnknownElement(String),
    InvalidElementMass { element_id: String, mass_grams: f64 },
    InvalidStructureDimensions { size_x_px: u32, size_y_px: u32 },
    StructurePixelCountOverflow { size_x_px: u32, size_y_px: u32 },
    StructurePixelCountMismatch { expected: usize, actual: usize },
    EmptyStructure,
    InvalidTemperature(f64),
}

impl fmt::Display for MatterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyComposition => {
                formatter.write_str("matter must contain at least one element")
            }
            Self::UnknownElement(id) => write!(formatter, "unknown element: {id}"),
            Self::InvalidElementMass {
                element_id,
                mass_grams,
            } => write!(
                formatter,
                "element {element_id} must have a finite positive mass, got {mass_grams}"
            ),
            Self::InvalidStructureDimensions {
                size_x_px,
                size_y_px,
            } => write!(
                formatter,
                "matter structure dimensions must be at least 1x1 px, got {size_x_px}x{size_y_px}"
            ),
            Self::StructurePixelCountOverflow {
                size_x_px,
                size_y_px,
            } => write!(
                formatter,
                "matter structure dimensions are too large: {size_x_px}x{size_y_px}"
            ),
            Self::StructurePixelCountMismatch { expected, actual } => write!(
                formatter,
                "matter structure expected {expected} pixels, got {actual}"
            ),
            Self::EmptyStructure => {
                formatter.write_str("matter structure must occupy at least one pixel")
            }
            Self::InvalidTemperature(value) => {
                write!(formatter, "matter temperature must be finite, got {value}")
            }
        }
    }
}

impl std::error::Error for MatterError {}
