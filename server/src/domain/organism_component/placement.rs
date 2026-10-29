use super::{Assembly, ComponentRegistry};
use crate::domain::{BiologicalOrganism, ComponentCategory, ComponentSize, SurfaceAnchor};
use std::collections::BTreeMap;

/// Resolve legacy defaults, then validate against the authoritative catalog.
pub fn normalize_placements(
    organism: &mut BiologicalOrganism,
    registry: &ComponentRegistry,
) -> Result<(), String> {
    for instance in organism.components.values_mut() {
        let definition = registry
            .get(instance.component.as_str())
            .ok_or("unknown component")?;
        if instance.placement.size == ComponentSize::default() {
            instance.placement.size = definition.model.size;
        }
        if definition.model.assembly != Assembly::Wrap
            && !definition.model.accepts_size(instance.placement.size)
        {
            return Err(format!("invalid size for {}", instance.component));
        }
        if definition.category == ComponentCategory::ExternalOrgan
            && instance.placement.size.width_px != 0
        {
            return Err("external organ cannot have physical thickness".into());
        }
        if instance
            .placement
            .surface_anchor
            .is_some_and(|a| a.perimeter_ppm > SurfaceAnchor::MAX_PERIMETER_PPM)
        {
            return Err("invalid surface anchor".into());
        }
        if definition.category == ComponentCategory::Body
            && (instance.placement.host_body.is_some()
                || instance.placement.surface_anchor.is_some())
        {
            return Err("body cannot have a host or surface anchor".into());
        }
        if definition.category == ComponentCategory::Body {
            instance.placement.position.get_or_insert_default();
        }
    }
    let snapshot = organism.components.clone();
    let mut hosts = BTreeMap::<_, Vec<_>>::new();
    for edge in &organism.connections {
        for (part, body) in [(edge.first, edge.second), (edge.second, edge.first)] {
            if let Some(body) = snapshot.get(&body).filter(|body| {
                registry
                    .get(body.component.as_str())
                    .is_some_and(|d| d.category == ComponentCategory::Body)
            }) {
                hosts.entry(part).or_default().push(body);
            }
        }
    }
    for instance in organism.components.values_mut() {
        let definition = registry
            .get(instance.component.as_str())
            .ok_or("unknown component")?;
        if definition.model.assembly == Assembly::Free {
            continue;
        }
        let connected_bodies = hosts
            .get(&instance.id)
            .ok_or("attached component requires a body")?;
        let host = match instance.placement.host_body {
            Some(id) => connected_bodies
                .iter()
                .copied()
                .find(|body| body.id == id)
                .ok_or("host must be a connected body")?,
            None => *connected_bodies
                .first()
                .ok_or("attached component requires a body")?,
        };
        instance.placement.host_body = Some(host.id);
        match definition.model.assembly {
            Assembly::Wrap => {
                let size = host.placement.size;
                instance.placement.size = ComponentSize {
                    width_px: 0,
                    length_px: (2.0 * f64::from(size.length_px - size.width_px)
                        + std::f64::consts::PI * f64::from(size.width_px))
                    .round() as u32,
                };
                instance.placement.position = host.placement.position;
                instance.placement.rotation_mdeg = host.placement.rotation_mdeg;
                if instance.placement.surface_anchor.is_some() {
                    return Err("wrapped component cannot have a surface anchor".into());
                }
            }
            Assembly::Surface => {
                if instance.placement.size.width_px != 0 {
                    return Err("surface organ cannot have physical thickness".into());
                }
                let anchor = instance
                    .placement
                    .surface_anchor
                    .get_or_insert(SurfaceAnchor::default());
                let (x, y, angle) = capsule_surface(host.placement.size, anchor.perimeter_ppm);
                let rotation = f64::from(host.placement.rotation_mdeg) / 1000.0;
                let radians = rotation.to_radians();
                let host_position = host.placement.position.unwrap_or_default();
                instance.placement.position = Some(crate::domain::MapPosition {
                    x: (f64::from(host_position.x) + x * radians.cos() - y * radians.sin()).round()
                        as i32,
                    y: (f64::from(host_position.y) + x * radians.sin() + y * radians.cos()).round()
                        as i32,
                });
                instance.placement.rotation_mdeg =
                    ((angle + rotation).rem_euclid(360.0) * 1000.0).round() as i32;
            }
            Assembly::Interior => {
                if instance.placement.surface_anchor.is_some() {
                    return Err("internal component cannot have a surface anchor".into());
                }
                // Internal organs occupy the host body; their position is runtime-derived.
                instance.placement.position = None;
            }
            Assembly::Free => {}
        }
    }
    Ok(())
}

fn capsule_surface(size: ComponentSize, ppm: u32) -> (f64, f64, f64) {
    let radius = f64::from(size.width_px) / 2.0;
    let straight = f64::from(size.length_px - size.width_px);
    let arc = std::f64::consts::PI * radius;
    let mut distance = f64::from(ppm) / 1_000_000.0 * (2.0 * straight + 2.0 * arc);
    if distance <= straight {
        return (-straight / 2.0 + distance, -radius, -90.0);
    }
    distance -= straight;
    if distance <= arc {
        let angle = -90.0 + distance / arc * 180.0;
        return (
            straight / 2.0 + angle.to_radians().cos() * radius,
            angle.to_radians().sin() * radius,
            angle,
        );
    }
    distance -= arc;
    if distance <= straight {
        return (straight / 2.0 - distance, radius, 90.0);
    }
    distance -= straight;
    let angle = 90.0 + distance / arc * 180.0;
    (
        -straight / 2.0 + angle.to_radians().cos() * radius,
        angle.to_radians().sin() * radius,
        angle,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ComponentInstance, OrganismProgramId};

    #[test]
    fn surface_position_is_derived_from_host_and_anchor() {
        let registry = super::super::create_default_component_registry().unwrap();
        let mut organism =
            BiologicalOrganism::new(OrganismProgramId::from_uuid(uuid::Uuid::new_v4()));
        let body = ComponentInstance::new("small_body_1", &registry).unwrap();
        let mut organ = ComponentInstance::new("jet_organ_1", &registry).unwrap();
        let (body_id, organ_id) = (body.id, organ.id);
        organ.placement.position = Some(crate::domain::MapPosition { x: 9999, y: 0 });
        organism.components.insert(body_id, body);
        organism.components.insert(organ_id, organ);
        organism.connect(body_id, organ_id, &registry).unwrap();
        normalize_placements(&mut organism, &registry).unwrap();
        assert_eq!(
            organism.components[&organ_id].placement.position.unwrap().x,
            -20
        );
        assert_eq!(
            organism.components[&organ_id].placement.position.unwrap().y,
            -20
        );
        organism
            .components
            .get_mut(&organ_id)
            .unwrap()
            .placement
            .surface_anchor = Some(SurfaceAnchor {
            perimeter_ppm: 1_000_001,
        });
        assert!(normalize_placements(&mut organism, &registry).is_err());
        organism
            .components
            .get_mut(&organ_id)
            .unwrap()
            .placement
            .surface_anchor = None;
        organism
            .components
            .get_mut(&organ_id)
            .unwrap()
            .placement
            .size
            .width_px = 1;
        assert!(normalize_placements(&mut organism, &registry).is_err());
    }

    #[test]
    fn resolves_defaults_and_wrap_size_and_rejects_forged_geometry() {
        let registry = super::super::create_default_component_registry().unwrap();
        let mut organism =
            BiologicalOrganism::new(OrganismProgramId::from_uuid(uuid::Uuid::new_v4()));
        let body = ComponentInstance::new("small_body_1", &registry).unwrap();
        let skin = ComponentInstance::new("skin_1", &registry).unwrap();
        let (body_id, skin_id) = (body.id, skin.id);
        organism.components.insert(body_id, body);
        organism.components.insert(skin_id, skin);
        organism.connect(body_id, skin_id, &registry).unwrap();
        normalize_placements(&mut organism, &registry).unwrap();
        assert_eq!(organism.components[&skin_id].placement.size.length_px, 206);
        assert_eq!(
            organism.components[&skin_id].placement.host_body,
            Some(body_id)
        );
        organism
            .components
            .get_mut(&body_id)
            .unwrap()
            .placement
            .size
            .width_px = 41;
        assert!(normalize_placements(&mut organism, &registry).is_err());
        organism
            .components
            .get_mut(&body_id)
            .unwrap()
            .placement
            .size
            .width_px = 40;
        organism
            .components
            .get_mut(&skin_id)
            .unwrap()
            .placement
            .host_body = Some(uuid::Uuid::new_v4());
        assert!(normalize_placements(&mut organism, &registry).is_err());
    }

    #[test]
    fn internal_positions_are_derived_inside_host_body() {
        let registry = super::super::create_default_component_registry().unwrap();
        let mut organism =
            BiologicalOrganism::new(OrganismProgramId::from_uuid(uuid::Uuid::new_v4()));
        let mut body = ComponentInstance::new("small_body_1", &registry).unwrap();
        body.placement.position = Some(crate::domain::MapPosition { x: 100, y: 200 });
        let mut first = ComponentInstance::new("reaction_structure_1", &registry).unwrap();
        let mut second = ComponentInstance::new("sensor_group_1", &registry).unwrap();
        first.placement.position = Some(crate::domain::MapPosition { x: 9999, y: -9999 });
        second.placement.position = Some(crate::domain::MapPosition { x: -5000, y: 8000 });
        let (body_id, first_id, second_id) = (body.id, first.id, second.id);
        organism.components.insert(body_id, body);
        organism.components.insert(first_id, first);
        organism.components.insert(second_id, second);
        organism.connect(body_id, first_id, &registry).unwrap();
        organism.connect(body_id, second_id, &registry).unwrap();
        normalize_placements(&mut organism, &registry).unwrap();
        assert_eq!(organism.components[&first_id].placement.position, None);
        assert_eq!(organism.components[&second_id].placement.position, None);
    }
}
