mod builtin;
mod definition;
mod model;
mod placement;
mod registry;
pub use model::{Appearance, Assembly, Collision, ComponentModel, Geometry, SizeBounds};
pub use placement::normalize_placements;

pub use builtin::create_default_component_registry;
pub use definition::{
    ComponentCategory, ComponentDefinition, ComponentId, ComponentSlots, ComponentUnlockMethod,
    ComponentUnlockRule,
};
pub use registry::{ComponentRegistrationError, ComponentRegistry};
