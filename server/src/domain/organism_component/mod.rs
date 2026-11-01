mod builtin;
mod control;
pub use control::{
    ComponentAction, ComponentApi, ComponentCommand, RateControl, builtin_component_api,
};
mod definition;
mod model;
mod placement;
mod properties;
mod registry;
pub use model::{Appearance, Assembly, Collision, ComponentModel, Geometry, SizeBounds};
pub use placement::normalize_placements;
pub use properties::ComponentProperties;

pub use builtin::create_default_component_registry;
pub use definition::{
    ComponentCategory, ComponentDefinition, ComponentId, ComponentSlots, ComponentUnlockMethod,
    ComponentUnlockRule,
};
pub use registry::{ComponentRegistrationError, ComponentRegistry};
