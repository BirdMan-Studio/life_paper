use crate::domain::{
    MapPosition, World,
    organism::{ComponentInstance, OrganismInstance},
    organism_component::{ComponentAction, builtin_component_api},
    terrain::TerrainRegistry,
};
use rapier2d_f64::{parry::query::contact, prelude::*};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

const DT: f64 = 0.05;
const STEPS: usize = 20;
const THRUST_PER_MG: f64 = 8.0;
const MAX_SPEED: f64 = 120.0;
const MAX_ANGULAR_SPEED: f64 = 3.0;
const MAX_RADIUS: f64 = 512.0;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MotionState {
    pub x: f64,
    pub y: f64,
    pub angle_rad: f64,
    pub velocity_x: f64,
    pub velocity_y: f64,
    pub angular_velocity: f64,
}

impl MotionState {
    fn valid(&self) -> bool {
        [
            self.x,
            self.y,
            self.angle_rad,
            self.velocity_x,
            self.velocity_y,
            self.angular_velocity,
        ]
        .into_iter()
        .all(f64::is_finite)
            && self.x.abs() < f64::from(i32::MAX) - 2048.0
            && self.y.abs() < f64::from(i32::MAX) - 2048.0
    }
}

fn local_position(part: &ComponentInstance, organism: &OrganismInstance) -> Vector<f64> {
    let p = part
        .placement
        .position
        .or_else(|| {
            part.placement
                .host_body
                .and_then(|id| organism.composition.components.get(&id))
                .and_then(|host| host.placement.position)
        })
        .unwrap_or_default();
    vector![f64::from(p.x), f64::from(p.y)]
}

struct Geometry {
    shape: SharedShape,
    mass: MassProperties,
    radius: f64,
}

fn geometry(organism: &OrganismInstance) -> Option<Geometry> {
    let mut shapes = Vec::new();
    let mut masses = Vec::new();
    let mut radius: f64 = 0.0;
    for (id, part) in &organism.composition.components {
        let state = organism.component_states.get(id)?;
        let p = local_position(part, organism);
        // Only bodies have thickness and no host in the normalized composition.
        let body = part.placement.host_body.is_none() && part.placement.size.width_px > 0;
        let mass = state.properties.mass_mg.unwrap_or(1_000).max(1) as f64 / 1000.0;
        let angle = f64::from(part.placement.rotation_mdeg).to_radians() / 1000.0;
        let inertia = if body {
            let size = part.placement.size;
            let r = f64::from(size.width_px) / 2.0;
            let half = f64::from(size.length_px - size.width_px) / 2.0;
            radius = radius.max(p.norm() + half + r);
            shapes.push((Isometry::new(p, angle), SharedShape::capsule_x(half, r)));
            mass * (f64::from(size.length_px).powi(2) + f64::from(size.width_px).powi(2)) / 12.0
        } else {
            0.0
        };
        masses.push(MassProperties::new(Point::from(p), mass, inertia));
    }
    if shapes.is_empty() || radius > MAX_RADIUS {
        return None;
    }
    Some(Geometry {
        shape: SharedShape::compound(shapes),
        mass: masses.into_iter().sum(),
        radius,
    })
}

fn pose(organism: &OrganismInstance) -> Isometry<f64> {
    if let Some(motion) = organism.motion.as_ref().filter(|m| m.valid()) {
        Isometry::new(vector![motion.x, motion.y], motion.angle_rad)
    } else {
        Isometry::new(
            vector![
                f64::from(organism.position.x),
                f64::from(organism.position.y)
            ],
            f64::from(organism.rotation_mdeg).to_radians() / 1000.0,
        )
    }
}

fn blocked(world: &World, terrains: &TerrainRegistry, x: i32, y: i32) -> bool {
    terrains
        .get(world.terrain_at(MapPosition { x, y }).as_str())
        .is_none_or(|t| t.blocks_movement)
}

fn legal(
    world: &World,
    terrains: &TerrainRegistry,
    shape: &SharedShape,
    p: &Isometry<f64>,
) -> bool {
    let aabb = shape.compute_aabb(p);
    let tile = Cuboid::new(vector![0.5, 0.5]);
    for y in (aabb.mins.y.floor() as i32)..=(aabb.maxs.y.floor() as i32) {
        for x in (aabb.mins.x.floor() as i32)..=(aabb.maxs.x.floor() as i32) {
            if blocked(world, terrains, x, y)
                && contact(
                    p,
                    shape.as_ref(),
                    &Isometry::translation(f64::from(x) + 0.5, f64::from(y) + 0.5),
                    &tile,
                    0.0,
                )
                .map_or(true, |hit| hit.is_some_and(|hit| hit.dist < -0.01))
            {
                return false;
            }
        }
    }
    true
}

fn publish(organism: &mut OrganismInstance, p: &Isometry<f64>, velocity: Vector<f64>, omega: f64) {
    let angle = p.rotation.angle().rem_euclid(std::f64::consts::TAU);
    organism.position = MapPosition {
        x: p.translation.x.floor() as i32,
        y: p.translation.y.floor() as i32,
    };
    organism.rotation_mdeg = ((angle.to_degrees() * 1000.0).round() as i32).rem_euclid(360_000);
    organism.motion = Some(MotionState {
        x: p.translation.x,
        y: p.translation.y,
        angle_rad: angle,
        velocity_x: velocity.x,
        velocity_y: velocity.y,
        angular_velocity: omega,
    });
}

/// Find a nearby legal origin without changing valid old save positions.
pub fn place(organism: &mut OrganismInstance, world: &World, terrains: &TerrainRegistry) -> bool {
    let Some(g) = geometry(organism) else {
        return false;
    };
    let p = pose(organism);
    if legal(world, terrains, &g.shape, &p) {
        return true;
    }
    for radius in (16i32..=256).step_by(16) {
        for y in (-radius..=radius).step_by(16) {
            for x in (-radius..=radius).step_by(16) {
                if x.abs() != radius && y.abs() != radius {
                    continue;
                }
                let candidate = Isometry::new(
                    p.translation.vector + vector![f64::from(x), f64::from(y)],
                    p.rotation.angle(),
                );
                if legal(world, terrains, &g.shape, &candidate) {
                    publish(organism, &candidate, Vector::zeros(), 0.0);
                    return true;
                }
            }
        }
    }
    false
}

fn spend(organism: &mut OrganismInstance, mut cost: u64) -> u64 {
    let requested = cost;
    if let Some(root) = organism.composition.root_body {
        if let Some(state) = organism.component_states.get_mut(&root) {
            let paid = cost.min(state.energy_mj);
            state.energy_mj -= paid;
            cost -= paid;
        }
    }
    for state in organism.component_states.values_mut() {
        let paid = cost.min(state.energy_mj);
        state.energy_mj -= paid;
        cost -= paid;
        if cost == 0 {
            break;
        }
    }
    requested - cost
}

#[derive(Default)]
struct Drive {
    force: Vector<f64>,
    torque: f64,
}

/// Settle energy, air intake, and prototype jet thrust once per tick.
#[cfg(test)]
fn execute(
    organism: &mut OrganismInstance,
    center: Point<f64>,
    world: &World,
    terrains: &TerrainRegistry,
) -> Drive {
    let mut matter = world.matter.clone();
    execute_with_matter(organism, center, world, terrains, &mut matter)
}

fn execute_with_matter(
    organism: &mut OrganismInstance,
    center: Point<f64>,
    world: &World,
    terrains: &TerrainRegistry,
    matter: &mut crate::domain::world::WorldMatter,
) -> Drive {
    let mut base = 0u64;
    for (id, state) in &organism.component_states {
        if state.durability == 0 {
            continue;
        }
        let Some(part) = organism.composition.components.get(id) else {
            continue;
        };
        let builtin = builtin_component_api(part.component.as_str());
        let api = state.properties.api.as_ref().unwrap_or(&builtin);
        base = base
            .saturating_add(u64::from(
                api.base_energy_mj.or(builtin.base_energy_mj).unwrap_or(0),
            ))
            .saturating_add(u64::from(api.jet.as_ref().map_or(0, |s| s.base_energy_mj)))
            .saturating_add(u64::from(
                api.absorption.as_ref().map_or(0, |s| s.base_energy_mj),
            ));
    }
    if spend(organism, base) < base {
        organism.ai.pending_commands.clear();
        return Drive::default();
    }
    let mut drive = Drive::default();
    let commands: BTreeMap<_, _> = organism
        .ai
        .pending_commands
        .iter()
        .map(|command| (command.component_id, command.clone()))
        .collect();
    for command in commands.into_values() {
        let exchange_source = crate::domain::world::MatterSource {
            organism_id: organism.id,
            component_id: command.component_id,
        };
        let Some(part) = organism.composition.components.get(&command.component_id) else {
            continue;
        };
        let Some(state) = organism.component_states.get(&command.component_id) else {
            continue;
        };
        if state.durability == 0 {
            continue;
        }
        let builtin = builtin_component_api(part.component.as_str());
        let api = state.properties.api.as_ref().unwrap_or(&builtin);
        let (rate_mg_per_tick, material, angle_deg) = match command.action {
            ComponentAction::Jet {
                rate_mg_per_tick,
                material,
                angle_deg,
            } => (rate_mg_per_tick, material, angle_deg),
            ComponentAction::AbsorbAir { rate_ml_per_tick } => {
                let Some(spec) = api.absorption.clone() else {
                    continue;
                };
                let host = part.placement.host_body;
                let intake = pose(organism) * Point::from(local_position(part, organism));
                let terrain = terrains.get(
                    world
                        .terrain_at(MapPosition {
                            x: intake.x.floor() as i32,
                            y: intake.y.floor() as i32,
                        })
                        .as_str(),
                );
                if !terrain.is_some_and(|t| {
                    t.allows_air_absorption && t.id.as_str() != "deep_water" && !t.blocks_movement
                }) || !rate_ml_per_tick.is_finite()
                    || !(0.0..=5.0).contains(&rate_ml_per_tick)
                {
                    continue;
                }
                let efficiency = (f64::from(state.durability)
                    / f64::from(state.properties.max_durability.max(1)))
                .min(1.0);
                let rate = (rate_ml_per_tick * 2.0).floor() / 2.0;
                let volume = rate
                    * spec
                        .gas_throughput_fraction(rate * 1.293, rate)
                        .unwrap_or(0.0)
                    * efficiency;
                let Some(host) = host else {
                    continue;
                };
                let Some(target) = organism
                    .component_states
                    .get(&host)
                    .filter(|s| s.durability > 0)
                else {
                    continue;
                };
                let used = target
                    .container
                    .gas_contents_ul
                    .values()
                    .fold(0u64, |a, b| a.saturating_add(*b));
                let free = target.container.gas_capacity_ul.saturating_sub(used);
                let volume = volume.min(free as f64 / 1000.0);
                if volume <= 0.0 {
                    continue;
                }
                let cost = (f64::from(spec.max_variable_energy_mj) * volume / 5.0).ceil() as u64;
                let paid = spend(organism, cost);
                let fraction = if cost == 0 {
                    1.0
                } else {
                    paid as f64 / cost as f64
                };
                let total_ul = (volume * fraction * 1000.0).floor() as u64;
                let oxygen_ul = total_ul * 21 / 100;
                matter.record_gas_exchange(
                    MapPosition {
                        x: intake.x.floor() as i32,
                        y: intake.y.floor() as i32,
                    },
                    exchange_source,
                    [
                        (
                            crate::domain::element::ElementId::new("oxygen").unwrap(),
                            oxygen_ul,
                        ),
                        (
                            crate::domain::element::ElementId::new("nitrogen").unwrap(),
                            total_ul - oxygen_ul,
                        ),
                    ]
                    .into(),
                    BTreeMap::new(),
                );
                let container = &mut organism.component_states.get_mut(&host).unwrap().container;
                for (id, amount) in [("oxygen", oxygen_ul), ("nitrogen", total_ul - oxygen_ul)] {
                    *container
                        .gas_contents_ul
                        .entry(crate::domain::element::ElementId::new(id).unwrap())
                        .or_default() += amount;
                }
                continue;
            }
            ComponentAction::AbsorbMatter { rate_mg_per_tick } => {
                let Some(spec) = api.absorption.clone() else {
                    continue;
                };
                if spec.validate(rate_mg_per_tick).is_err() {
                    continue;
                }
                let Some(host) = part.placement.host_body else {
                    continue;
                };
                let intake = pose(organism) * Point::from(local_position(part, organism));
                let position = MapPosition {
                    x: intake.x.floor() as i32,
                    y: intake.y.floor() as i32,
                };
                let efficiency = (f64::from(state.durability)
                    / f64::from(state.properties.max_durability.max(1)))
                .min(1.0);
                let Some(target) = organism
                    .component_states
                    .get(&host)
                    .filter(|s| s.durability > 0)
                else {
                    continue;
                };
                let used: f64 = target.container.non_gas_contents_mg.values().sum();
                let free = (target.container.non_gas_capacity_mg as f64 - used).max(0.0);
                let Ok(area) = matter.area(world, position, 5) else {
                    continue;
                };
                let available: f64 = area.total_mg.values().sum();
                let quantum =
                    f64::from(spec.max_rate_mg_per_tick) / f64::from(spec.subdivisions.max(1));
                if quantum <= 0.0 {
                    continue;
                }
                let rate = (rate_mg_per_tick / quantum).floor() * quantum * efficiency;
                // Initial nutrient particles use an explicit game density of 1.5 g/mL.
                let rate = rate * spec.throughput_fraction(rate, rate / 1500.0).unwrap_or(0.0);
                let rate = rate.min(free).min(available);
                if rate <= 0.0 {
                    continue;
                }
                let cost = spec
                    .energy_cost(rate)
                    .saturating_sub(u64::from(spec.base_energy_mj));
                let paid = spend(organism, cost);
                let fraction = if cost == 0 {
                    1.0
                } else {
                    paid as f64 / cost as f64
                };
                let absorbed = matter
                    .absorb(world, position, 5, rate * fraction, Some(exchange_source))
                    .expect("validated absorption");
                let target = &mut organism.component_states.get_mut(&host).unwrap().container;
                for (id, amount) in absorbed {
                    *target.non_gas_contents_mg.entry(id).or_default() += amount;
                }
                continue;
            }
        };
        let Some(spec) = api.jet.as_ref() else {
            continue;
        };
        if spec.max_rate_mg_per_tick == 0 {
            continue;
        }
        if spec.validate(rate_mg_per_tick).is_err() {
            continue;
        }
        let quantum = f64::from(spec.max_rate_mg_per_tick) / f64::from(spec.subdivisions.max(1));
        let rate = (rate_mg_per_tick / quantum).floor() * quantum;
        if !angle_deg.is_finite() || !(-5.0..=5.0).contains(&angle_deg) {
            continue;
        }
        let Some(host) = part.placement.host_body else {
            continue;
        };
        let Some(source) = organism
            .component_states
            .get(&host)
            .filter(|s| s.durability > 0)
        else {
            continue;
        };
        let Ok(id) = crate::domain::element::ElementId::new(&material) else {
            continue;
        };
        let gas_ul = source
            .container
            .gas_contents_ul
            .get(&id)
            .copied()
            .unwrap_or(0);
        // Reference mg/mL is independent of 2D areal density.
        let gas_density = match material.as_str() {
            "hydrogen" => Some(0.0899),
            "oxygen" => Some(1.429),
            "nitrogen" => Some(1.251),
            "carbon_dioxide" => Some(1.977),
            "ammonia" => Some(0.771),
            "water" => Some(0.804),
            _ => None,
        };
        let is_gas = gas_ul > 0;
        let density = if is_gas {
            let Some(density) = gas_density else {
                continue;
            };
            density
        } else {
            1500.0
        };
        let available = if is_gas {
            gas_ul as f64 / 1000.0 * density
        } else {
            source
                .container
                .non_gas_contents_mg
                .get(&id)
                .copied()
                .unwrap_or(0.0)
        };
        if !available.is_finite() || available <= 0.0 {
            continue;
        }
        let rate = rate.min(available);
        let volume = rate / density;
        let rate = rate
            * if is_gas {
                spec.gas_throughput_fraction(rate, volume)
            } else {
                spec.throughput_fraction(rate, volume)
            }
            .unwrap_or(0.0);
        if rate <= 0.0 {
            continue;
        }
        let efficiency = (f64::from(state.durability)
            / f64::from(state.properties.max_durability.max(1)))
        .min(1.0);
        let cost = spec
            .energy_cost(rate)
            .saturating_sub(u64::from(spec.base_energy_mj));
        let angle = f64::from(part.placement.rotation_mdeg).to_radians() / 1000.0
            + angle_deg.round().to_radians();
        let r = local_position(part, organism) - center.coords;
        let outlet = pose(organism) * Point::from(local_position(part, organism));
        let paid = spend(organism, cost);
        // Energy is allocated deterministically in component UUID order.
        let fraction = if cost == 0 {
            1.0
        } else {
            paid as f64 / cost as f64
        };
        let mut actual_mass = rate * efficiency * fraction;
        let source = &mut organism.component_states.get_mut(&host).unwrap().container;
        if is_gas {
            let used_ul = (actual_mass / density * 1000.0).floor().min(gas_ul as f64) as u64;
            actual_mass = used_ul as f64 / 1000.0 * density;
            if used_ul == gas_ul {
                source.gas_contents_ul.remove(&id);
            } else {
                source.gas_contents_ul.insert(id.clone(), gas_ul - used_ul);
            }
            matter.record_gas_exchange(
                MapPosition {
                    x: outlet.x.floor() as i32,
                    y: outlet.y.floor() as i32,
                },
                exchange_source,
                BTreeMap::new(),
                [(id, used_ul)].into(),
            );
        } else {
            let remaining = (available - actual_mass).max(0.0);
            if remaining == 0.0 {
                source.non_gas_contents_mg.remove(&id);
            } else {
                source.non_gas_contents_mg.insert(id.clone(), remaining);
            }
            if actual_mass > 0.0 {
                matter
                    .deposit(
                        MapPosition {
                            x: outlet.x.floor() as i32,
                            y: outlet.y.floor() as i32,
                        },
                        0,
                        [(id, actual_mass)].into(),
                        Some(exchange_source),
                    )
                    .expect("finite jet mass");
            }
        }
        let force = -vector![angle.cos(), angle.sin()] * actual_mass * THRUST_PER_MG;
        drive.force += force;
        drive.torque += r.x * force.y - r.y * force.x;
    }
    if organism.component_states.values().all(|s| s.energy_mj == 0) {
        organism.ai.pending_commands.clear();
    }
    drive
}

fn damping(
    world: &World,
    terrains: &TerrainRegistry,
    shape: &SharedShape,
    p: &Isometry<f64>,
) -> (f64, f64) {
    let aabb = shape.compute_aabb(p);
    let mut linear = 0.0;
    let mut angular = 0.0;
    let mut count = 0.0;
    // Sample the union shape, so overlapping bodies do not double-count terrain.
    for y in ((aabb.mins.y.floor() as i32)..=(aabb.maxs.y.floor() as i32)).step_by(4) {
        for x in ((aabb.mins.x.floor() as i32)..=(aabb.maxs.x.floor() as i32)).step_by(4) {
            if !shape.contains_point(p, &point![f64::from(x) + 0.5, f64::from(y) + 0.5]) {
                continue;
            }
            let Some(t) = terrains.get(world.terrain_at(MapPosition { x, y }).as_str()) else {
                continue;
            };
            let fallback = match t.id.as_str() {
                "swamp" => 4.0,
                "pond" | "shallow_water" => 1.0,
                _ if t.slows_movement => 1.5,
                _ => 0.4,
            };
            linear += t
                .linear_damping
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(fallback);
            angular += t
                .angular_damping
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(fallback + 0.2);
            count += 1.0;
        }
    }
    if count == 0.0 {
        (0.4, 0.6)
    } else {
        (linear / count, angular / count)
    }
}

#[cfg(test)]
pub fn advance(organism: &mut OrganismInstance, world: &World, terrains: &TerrainRegistry) {
    let mut matter = world.matter.clone();
    advance_with_matter(organism, world, terrains, &mut matter);
}

pub fn advance_with_matter(
    organism: &mut OrganismInstance,
    world: &World,
    terrains: &TerrainRegistry,
    matter: &mut crate::domain::world::WorldMatter,
) {
    let Some(g) = geometry(organism) else {
        return;
    };
    let drive = execute_with_matter(organism, g.mass.local_com, world, terrains, matter);
    if !place(organism, world, terrains) {
        publish(organism, &pose(organism), Vector::zeros(), 0.0);
        return;
    }
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();
    let p = pose(organism);
    let initial = organism
        .motion
        .clone()
        .filter(|m| m.valid())
        .unwrap_or_default();
    let handle = bodies.insert(
        RigidBodyBuilder::dynamic()
            .position(p)
            .linvel(vector![initial.velocity_x, initial.velocity_y].cap_magnitude(MAX_SPEED))
            .angvel(
                initial
                    .angular_velocity
                    .clamp(-MAX_ANGULAR_SPEED, MAX_ANGULAR_SPEED),
            )
            .additional_mass_properties(g.mass)
            .ccd_enabled(true)
            .can_sleep(false)
            .build(),
    );
    colliders.insert_with_parent(
        ColliderBuilder::new(g.shape.clone())
            .density(0.0)
            .friction(0.0)
            .restitution(0.0)
            .build(),
        handle,
        &mut bodies,
    );
    let mut pipeline = PhysicsPipeline::new();
    let mut islands = IslandManager::new();
    let mut broad = BroadPhaseMultiSap::new();
    let mut narrow = NarrowPhase::new();
    let mut impulse = ImpulseJointSet::new();
    let mut multibody = MultibodyJointSet::new();
    let mut ccd = CCDSolver::new();
    let params = IntegrationParameters {
        dt: DT,
        max_ccd_substeps: 8,
        ..Default::default()
    };
    let mut added = HashSet::new();
    let mut terrain_cache = BTreeMap::new();
    for _ in 0..STEPS {
        let body = &mut bodies[handle];
        let rotation = *body.rotation();
        body.reset_forces(false);
        body.reset_torques(false);
        body.apply_impulse(rotation * drive.force * DT, true);
        body.apply_torque_impulse(drive.torque * DT, true);
        let (linear, angular) = damping(world, terrains, &g.shape, body.position());
        body.set_linvel(
            (body.linvel() * (-linear * DT).exp()).cap_magnitude(MAX_SPEED),
            false,
        );
        body.set_angvel(
            (body.angvel() * (-angular * DT).exp()).clamp(-MAX_ANGULAR_SPEED, MAX_ANGULAR_SPEED),
            false,
        );
        // A disk around the assembly origin covers every orientation this substep.
        let origin = body.translation();
        let reach = g.radius
            + MAX_SPEED * DT
            + g.mass.local_com.coords.norm() * MAX_ANGULAR_SPEED * DT
            + 4.0;
        for y in ((origin.y - reach).floor() as i32)..=((origin.y + reach).ceil() as i32) {
            for x in ((origin.x - reach).floor() as i32)..=((origin.x + reach).ceil() as i32) {
                let wall = *terrain_cache
                    .entry((x, y))
                    .or_insert_with(|| blocked(world, terrains, x, y));
                if wall && added.insert((x, y)) {
                    colliders.insert(
                        ColliderBuilder::cuboid(0.5, 0.5)
                            .translation(vector![f64::from(x) + 0.5, f64::from(y) + 0.5])
                            .friction(0.0)
                            .restitution(0.0)
                            .build(),
                    );
                }
            }
        }
        pipeline.step(
            &Vector::zeros(),
            &params,
            &mut islands,
            &mut broad,
            &mut narrow,
            &mut bodies,
            &mut colliders,
            &mut impulse,
            &mut multibody,
            &mut ccd,
            None,
            &(),
            &(),
        );
        // Rapier resolves sliding contacts; reject any residual overlap conservatively.
        let body = &mut bodies[handle];
        if !legal(world, terrains, &g.shape, body.position()) {
            let proposed = *body.position();
            let mut safe = pose(organism);
            let mut velocity = *body.linvel();
            // Preserve a legal tangent when contact correction leaves a small overlap.
            for axis in 0..2 {
                let mut candidate = safe;
                candidate.translation.vector[axis] = proposed.translation.vector[axis];
                if legal(world, terrains, &g.shape, &candidate) {
                    safe = candidate;
                } else {
                    velocity[axis] = 0.0;
                }
            }
            let mut rotated = safe;
            rotated.rotation = proposed.rotation;
            if legal(world, terrains, &g.shape, &rotated) {
                safe = rotated;
            } else {
                body.set_angvel(0.0, true);
            }
            body.set_position(safe, true);
            body.set_linvel(velocity, true);
        }
        publish(
            organism,
            body.position(),
            body.linvel().cap_magnitude(MAX_SPEED),
            body.angvel().clamp(-MAX_ANGULAR_SPEED, MAX_ANGULAR_SPEED),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        BiologicalOrganism, OrganismProgramId, WorldGeneratorEditor,
        create_default_component_registry, create_default_terrain_registry,
        organism_component::ComponentCommand,
    };

    fn creature(offset_x: i32) -> OrganismInstance {
        let registry = create_default_component_registry().unwrap();
        let mut model = BiologicalOrganism::new(OrganismProgramId::new());
        let root = model.add_component("small_body_1", &registry).unwrap();
        let jet = model.add_component("jet_organ_1", &registry).unwrap();
        model.connect(root, jet, &registry).unwrap();
        let mut organism =
            OrganismInstance::from_model(model.id, 7, "test".into(), model, &registry).unwrap();
        organism
            .component_states
            .get_mut(&root)
            .unwrap()
            .container
            .non_gas_contents_mg
            .insert(
                crate::domain::element::ElementId::new("water").unwrap(),
                2000.0,
            );
        let part = organism.composition.components.get_mut(&jet).unwrap();
        part.placement.position = Some(MapPosition {
            x: offset_x,
            y: -20,
        });
        part.placement.rotation_mdeg = 270_000;
        organism.ai.pending_commands.push(ComponentCommand {
            component_id: jet,
            action: ComponentAction::Jet {
                material: "water".into(),
                rate_mg_per_tick: 200.0,
                angle_deg: 0.0,
            },
        });
        organism
    }

    fn flat(terrain: &str) -> (World, TerrainRegistry) {
        let terrains = create_default_terrain_registry().unwrap();
        let world = WorldGeneratorEditor::new(&terrains, terrain)
            .unwrap()
            .build();
        (world, terrains)
    }

    #[test]
    fn jet_consumes_material_and_deposits_actual_mass_with_angle_steps() {
        let (world, terrains) = flat("grassland");
        let mut organism = creature(0);
        let root = organism.composition.root_body.unwrap();
        let water = crate::domain::element::ElementId::new("water").unwrap();
        organism
            .component_states
            .get_mut(&root)
            .unwrap()
            .container
            .non_gas_contents_mg
            .insert(water.clone(), 25.0);
        organism.ai.pending_commands[0].action = ComponentAction::Jet {
            material: "water".into(),
            rate_mg_per_tick: 200.0,
            angle_deg: 2.6,
        };
        let center = geometry(&organism).unwrap().mass.local_com;
        let mut matter = world.matter.clone();
        let drive = execute_with_matter(&mut organism, center, &world, &terrains, &mut matter);
        assert!((drive.force.norm() - 200.0).abs() < 1e-8);
        assert!((drive.force.x / drive.force.y + 3.0_f64.to_radians().tan()).abs() < 1e-8);
        assert!(
            !organism.component_states[&root]
                .container
                .non_gas_contents_mg
                .contains_key(&water)
        );
        assert_eq!(matter.records.len(), 1);
        assert_eq!(matter.records[0].added_mg[&water], 25.0);
        assert_eq!(
            matter.records[0].source.as_ref().unwrap().organism_id,
            organism.id
        );
        assert_eq!(
            matter.records[0].source.as_ref().unwrap().component_id,
            organism.ai.pending_commands[0].component_id
        );
        let energy = organism.component_states[&root].energy_mj;
        assert_eq!(
            execute_with_matter(&mut organism, center, &world, &terrains, &mut matter)
                .force
                .norm(),
            0.0
        );
        assert_eq!(organism.component_states[&root].energy_mj, energy - 5);
        assert_eq!(matter.records.len(), 1);
        let restored: OrganismInstance =
            serde_json::from_slice(&serde_json::to_vec(&organism).unwrap()).unwrap();
        assert_eq!(restored, organism);
    }

    #[test]
    fn gas_jet_respects_volume_and_energy_limits_without_world_deposit() {
        let (world, terrains) = flat("grassland");
        let mut organism = creature(0);
        let root = organism.composition.root_body.unwrap();
        let oxygen = crate::domain::element::ElementId::new("oxygen").unwrap();
        organism
            .component_states
            .get_mut(&root)
            .unwrap()
            .container
            .gas_contents_ul
            .insert(oxygen.clone(), 10000);
        organism.ai.pending_commands[0].action = ComponentAction::Jet {
            material: "oxygen".into(),
            rate_mg_per_tick: 200.0,
            angle_deg: -5.0,
        };
        let center = geometry(&organism).unwrap().mass.local_com;
        let mut matter = world.matter.clone();
        let drive = execute_with_matter(&mut organism, center, &world, &terrains, &mut matter);
        let remaining = organism.component_states[&root].container.gas_contents_ul[&oxygen];
        assert!((7999..=8001).contains(&remaining));
        assert!(
            (drive.force.norm() - (10000 - remaining) as f64 / 1000.0 * 1.429 * THRUST_PER_MG)
                .abs()
                < 1e-8
        );
        assert_eq!(matter.records.len(), 1);
        assert_eq!(matter.records[0].gas_added_ul[&oxygen], 10000 - remaining);
        assert!(matter.records[0].added_mg.is_empty());
        assert_eq!(matter.records[0].expires_at_tick, Some(matter.tick));
        assert_eq!(
            matter.at(&world, matter.records[0].center),
            world.matter.at(&world, matter.records[0].center)
        );
        organism.component_states.get_mut(&root).unwrap().energy_mj = 5;
        assert_eq!(
            execute_with_matter(&mut organism, center, &world, &terrains, &mut matter)
                .force
                .norm(),
            0.0
        );
        assert_eq!(
            organism.component_states[&root].container.gas_contents_ul[&oxygen],
            remaining
        );
    }

    #[test]
    fn solid_absorption_conserves_world_and_container_mass_with_capacity_limits() {
        let registry = create_default_component_registry().unwrap();
        let mut model = BiologicalOrganism::new(OrganismProgramId::new());
        let root = model.add_component("small_body_1", &registry).unwrap();
        let organ = model
            .add_component("absorption_organ_1", &registry)
            .unwrap();
        model.connect(root, organ, &registry).unwrap();
        let mut organism =
            OrganismInstance::from_model(model.id, 7, "solid".into(), model, &registry).unwrap();
        organism.ai.pending_commands.push(ComponentCommand {
            component_id: organ,
            action: ComponentAction::AbsorbMatter {
                rate_mg_per_tick: 13.25,
            },
        });
        let (world, terrains) = flat("grassland");
        let center = geometry(&organism).unwrap().mass.local_com;
        let mut matter = world.matter.clone();
        execute_with_matter(&mut organism, center, &world, &terrains, &mut matter);
        let contents = &organism.component_states[&root]
            .container
            .non_gas_contents_mg;
        assert!((contents.values().sum::<f64>() - 10.0).abs() < 1e-9);
        assert!(
            (contents[&crate::domain::element::ElementId::new("cellulose").unwrap()] - 6.0).abs()
                < 1e-9
        );
        let record = &matter.records[0];
        assert_eq!(record.radius_px, 5);
        assert_eq!(record.cell_count, 81);
        assert!((record.absorbed_mg.values().sum::<f64>() - 10.0).abs() < 1e-9);
        let remaining = matter
            .area(&world, record.center, 5)
            .unwrap()
            .total_mg
            .values()
            .sum::<f64>();
        assert!((remaining - 152.0).abs() < 1e-9);
        organism
            .component_states
            .get_mut(&root)
            .unwrap()
            .container
            .non_gas_capacity_mg = 11;
        execute_with_matter(&mut organism, center, &world, &terrains, &mut matter);
        assert!(
            (organism.component_states[&root]
                .container
                .non_gas_contents_mg
                .values()
                .sum::<f64>()
                - 11.0)
                .abs()
                < 1e-9
        );
        assert!((matter.records[1].absorbed_mg.values().sum::<f64>() - 1.0).abs() < 1e-9);
        execute_with_matter(&mut organism, center, &world, &terrains, &mut matter);
        assert_eq!(matter.records.len(), 2);
        let (empty, _) = flat("wasteland");
        let mut empty_matter = empty.matter.clone();
        organism
            .component_states
            .get_mut(&root)
            .unwrap()
            .container
            .non_gas_capacity_mg = 2000;
        execute_with_matter(&mut organism, center, &empty, &terrains, &mut empty_matter);
        assert!(empty_matter.records.is_empty());
    }

    #[test]
    fn air_absorption_checks_terrain_capacity_energy_and_precision() {
        let registry = create_default_component_registry().unwrap();
        let mut model = BiologicalOrganism::new(OrganismProgramId::new());
        let root = model.add_component("small_body_1", &registry).unwrap();
        let organ = model
            .add_component("absorption_organ_1", &registry)
            .unwrap();
        model.connect(root, organ, &registry).unwrap();
        let mut organism =
            OrganismInstance::from_model(model.id, 7, "air".into(), model, &registry).unwrap();
        organism.ai.pending_commands.push(ComponentCommand {
            component_id: organ,
            action: ComponentAction::AbsorbAir {
                rate_ml_per_tick: 4.9,
            },
        });
        let (world, terrains) = flat("wasteland");
        let center = geometry(&organism).unwrap().mass.local_com;
        execute(&mut organism, center, &world, &terrains);
        let gas = &organism.component_states[&root].container.gas_contents_ul;
        assert_eq!(gas.values().sum::<u64>(), 4500);
        assert_eq!(
            gas[&crate::domain::element::ElementId::new("oxygen").unwrap()],
            945
        );
        assert_eq!(organism.component_states[&root].energy_mj, 992);
        let saved = serde_json::to_vec(&organism).unwrap();
        let restored: OrganismInstance = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored, organism);
        let (deep, _) = flat("deep_water");
        execute(&mut organism, center, &deep, &terrains);
        assert_eq!(
            organism.component_states[&root]
                .container
                .gas_contents_ul
                .values()
                .sum::<u64>(),
            4500
        );
        organism
            .component_states
            .get_mut(&root)
            .unwrap()
            .container
            .gas_capacity_ul = 4600;
        execute(&mut organism, center, &world, &terrains);
        assert_eq!(
            organism.component_states[&root]
                .container
                .gas_contents_ul
                .values()
                .sum::<u64>(),
            4600
        );
        let energy = organism.component_states[&root].energy_mj;
        execute(&mut organism, center, &world, &terrains);
        assert_eq!(organism.component_states[&root].energy_mj, energy - 3);
        organism
            .component_states
            .get_mut(&root)
            .unwrap()
            .container
            .gas_capacity_ul = 20000;
        organism.component_states.get_mut(&root).unwrap().energy_mj = 3;
        execute(&mut organism, center, &world, &terrains);
        assert_eq!(
            organism.component_states[&root]
                .container
                .gas_contents_ul
                .values()
                .sum::<u64>(),
            4600
        );
    }

    #[test]
    fn central_thrust_translates_and_off_center_thrust_rotates() {
        let (world, terrains) = flat("grassland");
        let mut straight = creature(0);
        advance(&mut straight, &world, &terrains);
        let m = straight.motion.as_ref().unwrap();
        assert!(m.y > 10.0 && m.velocity_y > 20.0, "{m:?}");
        assert!(m.angular_velocity.abs() < 1e-8 && m.x.abs() < 1e-8, "{m:?}");
        assert_eq!(
            straight.component_states[&straight.composition.root_body.unwrap()].energy_mj,
            985
        );
        let mut turning = creature(20);
        advance(&mut turning, &world, &terrains);
        assert!(turning.motion.as_ref().unwrap().angular_velocity > 0.1);
    }

    #[test]
    fn thrust_follows_world_rotation() {
        let (world, terrains) = flat("grassland");
        let mut organism = creature(0);
        organism.rotation_mdeg = 90_000;
        advance(&mut organism, &world, &terrains);
        let m = organism.motion.unwrap();
        assert!(m.velocity_x < -20.0 && m.velocity_y.abs() < 1e-8, "{m:?}");
    }

    #[test]
    fn empty_energy_coasts_and_swamp_slows_more() {
        let (grass, terrains) = flat("grassland");
        let (swamp, _) = flat("swamp");
        let mut a = creature(0);
        for state in a.component_states.values_mut() {
            state.energy_mj = 0;
        }
        a.motion = Some(MotionState {
            velocity_x: 30.0,
            angular_velocity: 1.0,
            ..Default::default()
        });
        let mut b = a.clone();
        advance(&mut a, &grass, &terrains);
        advance(&mut b, &swamp, &terrains);
        let (a, b) = (a.motion.unwrap(), b.motion.unwrap());
        assert!(a.x > b.x && b.x > 0.0);
        assert!(a.velocity_x > b.velocity_x && a.velocity_x < 30.0);
        assert!(a.angular_velocity > b.angular_velocity && a.angular_velocity < 1.0);
    }

    #[test]
    fn precision_durability_and_energy_limit_effective_thrust() {
        let (world, terrains) = flat("grassland");
        let mut organism = creature(0);
        let jet = organism.ai.pending_commands[0].component_id;
        organism.ai.pending_commands[0].action = ComponentAction::Jet {
            material: "water".into(),
            rate_mg_per_tick: 31.0,
            angle_deg: 0.0,
        };
        organism.component_states.get_mut(&jet).unwrap().durability = 25;
        let g = geometry(&organism).unwrap();
        let drive = execute(&mut organism, g.mass.local_com, &world, &terrains);
        assert!((drive.force.y - 80.0).abs() < 1e-8);
        assert_eq!(
            organism.component_states[&organism.composition.root_body.unwrap()].energy_mj,
            994
        );
        organism.component_states.get_mut(&jet).unwrap().durability = 0;
        assert_eq!(
            execute(&mut organism, g.mass.local_com, &world, &terrains)
                .force
                .norm(),
            0.0
        );
        organism.component_states.get_mut(&jet).unwrap().durability = 50;
        organism
            .component_states
            .get_mut(&organism.composition.root_body.unwrap())
            .unwrap()
            .energy_mj = 5;
        assert_eq!(
            execute(&mut organism, g.mass.local_com, &world, &terrains)
                .force
                .norm(),
            0.0
        );
    }

    #[test]
    fn fast_body_cannot_cross_thin_wall_and_can_slide() {
        let terrains = create_default_terrain_registry().unwrap();
        let mut editor = WorldGeneratorEditor::new(&terrains, "grassland").unwrap();
        editor
            .fill_rectangle(MapPosition { x: 50, y: -200 }, 1, 400, "wall")
            .unwrap();
        let world = editor.build();
        let mut organism = creature(0);
        organism.ai.pending_commands.clear();
        organism.motion = Some(MotionState {
            velocity_x: 110.0,
            velocity_y: 35.0,
            ..Default::default()
        });
        advance(&mut organism, &world, &terrains);
        let m = organism.motion.as_ref().unwrap();
        assert!(m.x < 11.0 && m.y > 5.0, "{m:?}");
        assert!(legal(
            &world,
            &terrains,
            &geometry(&organism).unwrap().shape,
            &pose(&organism)
        ));
    }

    #[test]
    fn rotation_respects_wall_and_old_overlap_has_bounded_recovery() {
        let terrains = create_default_terrain_registry().unwrap();
        let mut editor = WorldGeneratorEditor::new(&terrains, "grassland").unwrap();
        editor
            .fill_rectangle(MapPosition { x: -100, y: 25 }, 200, 1, "wall")
            .unwrap();
        let world = editor.build();
        let mut organism = creature(0);
        organism.ai.pending_commands.clear();
        organism.motion = Some(MotionState {
            angular_velocity: 3.0,
            ..Default::default()
        });
        advance(&mut organism, &world, &terrains);
        assert!(legal(
            &world,
            &terrains,
            &geometry(&organism).unwrap().shape,
            &pose(&organism)
        ));
        organism.motion = None;
        organism.position.y = 25;
        assert!(place(&mut organism, &world, &terrains));
        let (wall, _) = flat("wall");
        assert!(!place(&mut organism, &wall, &terrains));
    }

    #[test]
    fn legacy_and_motion_saves_round_trip() {
        let original = creature(0);
        let mut value = serde_json::to_value(&original).unwrap();
        value.as_object_mut().unwrap().remove("motion");
        let mut restored: OrganismInstance = serde_json::from_value(value).unwrap();
        assert!(restored.motion.is_none());
        let (world, terrains) = flat("grassland");
        advance(&mut restored, &world, &terrains);
        let reloaded: OrganismInstance =
            serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
        assert_eq!(reloaded, restored);
    }
}
