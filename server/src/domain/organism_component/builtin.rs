//! 内置生物组件注册入口。
//!
//! 新增组件时只需在这里登记类别、连接槽位和功能代码。功能代码暂不执行。

use super::{ComponentCategory, ComponentRegistrationError, ComponentRegistry, ComponentSlots};

pub fn create_default_component_registry() -> Result<ComponentRegistry, ComponentRegistrationError>
{
    let mut registry = ComponentRegistry::new();

    registry.register(
        "photosensor_organ_1",
        "感光器官I",
        ComponentCategory::ExternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;
    registry.register(
        "small_body_1",
        "小型躯体I",
        ComponentCategory::Body,
        ComponentSlots::new()
            .with(ComponentCategory::ExternalOrgan, 4)
            .with(ComponentCategory::InternalOrgan, 3),
        None,
    )?;
    registry.register(
        "reaction_structure_1",
        "反应结构I",
        ComponentCategory::InternalOrgan,
        ComponentSlots::new().with(ComponentCategory::Body, 1),
        None,
    )?;

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_builtin_components_and_connection_slots() {
        let registry = create_default_component_registry().unwrap();
        assert_eq!(registry.len(), 3);

        let photosensor = registry.get("photosensor_organ_1").unwrap();
        assert_eq!(photosensor.category, ComponentCategory::ExternalOrgan);
        assert_eq!(photosensor.slots.count(ComponentCategory::Body), 1);
        assert_eq!(photosensor.slots.total(), 1);

        let body = registry.get("small_body_1").unwrap();
        assert_eq!(body.category, ComponentCategory::Body);
        assert_eq!(body.slots.count(ComponentCategory::ExternalOrgan), 4);
        assert_eq!(body.slots.count(ComponentCategory::InternalOrgan), 3);
        assert_eq!(body.slots.total(), 7);

        let reaction = registry.get("reaction_structure_1").unwrap();
        assert_eq!(reaction.category, ComponentCategory::InternalOrgan);
        assert_eq!(reaction.slots.count(ComponentCategory::Body), 1);
        assert_eq!(reaction.function_code, None);
    }
}
