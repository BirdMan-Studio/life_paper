mod builtin;
mod definition;
mod registry;

pub use builtin::create_default_terrain_registry;
pub use definition::{TerrainDefinition, TerrainId, TerrainVisual};
pub use registry::{TerrainRegistrationError, TerrainRegistry};
