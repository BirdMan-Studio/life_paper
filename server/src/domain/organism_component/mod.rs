mod builtin;
mod definition;
mod registry;

pub use builtin::create_default_component_registry;
pub use definition::{
    ComponentCategory, ComponentDefinition, ComponentId, ComponentSlots, ComponentUnlockMethod,
    ComponentUnlockRule,
};
pub use registry::{ComponentRegistrationError, ComponentRegistry};
