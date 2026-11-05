use super::{GroundProperties, MapPosition, TerrainArea, World};
use crate::domain::element::ElementId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub type MatterAmounts = BTreeMap<ElementId, f64>;
const RETENTION_PER_TICK: f64 = 0.999;
const MAX_RADIUS_PX: u32 = 64;
pub const RESTORED_EPSILON_MG: f64 = 0.000001;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MatterSource {
    pub organism_id: Uuid,
    pub component_id: Uuid,
}

/// One spatial operation, rather than a stored copy of every affected cell.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MatterRecord {
    pub tick: u64,
    pub center: MapPosition,
    pub radius_px: u32,
    pub cell_count: u32,
    pub absorbed_mg: MatterAmounts,
    pub added_mg: MatterAmounts,
    pub removed_fraction: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MatterSource>,
    #[serde(default)]
    pub expires_at_tick: Option<u64>,
    #[serde(default)]
    pub updated_at_sequence: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gas_absorbed_ul: BTreeMap<ElementId, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gas_added_ul: BTreeMap<ElementId, u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(from = "SavedWorldMatter")]
pub struct WorldMatter {
    pub tick: u64,
    pub records: Vec<MatterRecord>,
    #[serde(skip)]
    active_records: Vec<usize>,
}

#[derive(Deserialize)]
struct SavedWorldMatter {
    tick: u64,
    records: Vec<MatterRecord>,
}

impl From<SavedWorldMatter> for WorldMatter {
    fn from(saved: SavedWorldMatter) -> Self {
        let mut result = Self {
            tick: saved.tick,
            records: saved.records,
            active_records: Vec::new(),
        };
        // Legacy histories have no deadlines. A shared conservative deadline preserves overlaps.
        if result.records.iter().any(|r| r.expires_at_tick.is_none()) {
            let last = result.records.iter().map(|r| r.tick).max().unwrap_or(0);
            let excess = result
                .records
                .iter()
                .flat_map(|r| r.added_mg.values())
                .sum();
            let deadline = last.saturating_add(recovery_ticks(excess));
            let revision = result.records.len() as u64;
            for record in &mut result.records {
                record.expires_at_tick = Some(deadline);
                record.updated_at_sequence = revision;
            }
        }
        result.active_records = result
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.expires_at_tick.is_some_and(|end| end > result.tick))
            .map(|(index, _)| index)
            .collect();
        result
    }
}

fn recovery_ticks(excess_mg: f64) -> u64 {
    // Every deficit recovers in <=1000 ticks. Total deposits bound every cell's excess.
    let decay = if excess_mg > RESTORED_EPSILON_MG {
        ((RESTORED_EPSILON_MG / excess_mg).ln() / RETENTION_PER_TICK.ln()).ceil() as u64
    } else {
        0
    };
    decay.max(1000).saturating_add(1)
}

fn overlaps(a: &MatterRecord, b: &MatterRecord) -> bool {
    let dx = i128::from(a.center.x) - i128::from(b.center.x);
    let dy = i128::from(a.center.y) - i128::from(b.center.y);
    let radius = i128::from(a.radius_px) + i128::from(b.radius_px);
    dx * dx + dy * dy <= radius * radius
}

#[derive(Clone, Debug)]
pub struct AreaMatter {
    pub cell_count: u32,
    pub total_mg: MatterAmounts,
    pub average_mg_per_cell: MatterAmounts,
}

#[derive(Debug, Serialize)]
pub struct MatterSync {
    pub tick: u64,
    pub cursor: u64,
    pub reset: bool,
    pub center: MapPosition,
    pub radius_px: u32,
    pub restored_epsilon_mg: f64,
    pub retention_per_tick: f64,
    pub replenishment_per_tick: f64,
    pub records: Vec<MatterUpdate>,
}

#[derive(Debug, Serialize)]
pub struct MatterUpdate {
    pub sequence: u64,
    #[serde(flatten)]
    pub record: MatterRecord,
}

#[derive(Debug, Serialize)]
pub struct MatterHistory {
    pub tick: u64,
    pub cursor: u64,
    pub has_more: bool,
    pub records: Vec<MatterUpdate>,
}

fn intersects_region(record: &MatterRecord, center: MapPosition, radius_px: u32) -> bool {
    let dx = i128::from(record.center.x) - i128::from(center.x);
    let dy = i128::from(record.center.y) - i128::from(center.y);
    let radius = i128::from(record.radius_px) + i128::from(radius_px);
    dx * dx + dy * dy <= radius * radius
}

pub fn grass_capacity_mg(properties: GroundProperties) -> f64 {
    match properties.nutrients {
        0..=20 => 2.0,
        21..=80 => 5.0,
        _ => 10.0,
    }
}

fn baseline(world: &World, position: MapPosition) -> MatterAmounts {
    if world.terrain_at(position).as_str() != "grassland" {
        return BTreeMap::new();
    }
    let capacity = grass_capacity_mg(world.ground_properties(position));
    [
        ("monosaccharide", 0.1),
        ("polysaccharide", 0.3),
        ("cellulose", 0.6),
    ]
    .into_iter()
    .map(|(id, fraction)| (ElementId::new(id).unwrap(), capacity * fraction))
    .collect()
}

fn cells(center: MapPosition, radius_px: u32) -> Result<Vec<MapPosition>, &'static str> {
    if radius_px > MAX_RADIUS_PX {
        return Err("matter query radius exceeds 64 pixels");
    }
    let area = TerrainArea::Circle { center, radius_px };
    let radius = i64::from(radius_px);
    let mut result = Vec::new();
    for y in i64::from(center.y) - radius..=i64::from(center.y) + radius {
        for x in i64::from(center.x) - radius..=i64::from(center.x) + radius {
            if let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) {
                let position = MapPosition { x, y };
                if area.contains(position) {
                    result.push(position);
                }
            }
        }
    }
    Ok(result)
}

fn relax(amounts: &mut MatterAmounts, target: &MatterAmounts, ticks: u64) {
    let retention = RETENTION_PER_TICK.powf(ticks as f64);
    // Sparse entries represent zero stock; only at() initializes the baseline.
    for id in target.keys() {
        amounts.entry(id.clone()).or_insert(0.0);
    }
    for (id, mass) in amounts.iter_mut() {
        let default = target.get(id).copied().unwrap_or(0.0);
        *mass = if *mass < default {
            (*mass + default * 0.001 * ticks as f64).min(default)
        } else {
            default + (*mass - default) * retention
        };
        if ticks > 0 && *mass >= default && *mass - default <= RESTORED_EPSILON_MG {
            *mass = default;
        }
    }
    amounts.retain(|_, mass| *mass > 0.0);
}

impl WorldMatter {
    pub fn sync(
        &self,
        center: MapPosition,
        radius_px: u32,
        since: Option<u64>,
    ) -> Result<MatterSync, &'static str> {
        if radius_px > MAX_RADIUS_PX {
            return Err("matter query radius exceeds 64 pixels");
        }
        if since.is_some_and(|cursor| cursor > self.records.len() as u64) {
            return Err("matter cursor exceeds history length");
        }
        let records = self
            .active_records
            .iter()
            .filter_map(|&index| {
                let record = &self.records[index];
                if record.expires_at_tick.is_some_and(|end| end > self.tick)
                    && since.is_none_or(|cursor| record.updated_at_sequence > cursor)
                    && intersects_region(record, center, radius_px)
                {
                    Some(MatterUpdate {
                        sequence: index as u64 + 1,
                        record: record.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();
        Ok(MatterSync {
            tick: self.tick,
            cursor: self.records.len() as u64,
            reset: since.is_none(),
            center,
            radius_px,
            restored_epsilon_mg: RESTORED_EPSILON_MG,
            retention_per_tick: RETENTION_PER_TICK,
            replenishment_per_tick: 0.001,
            records,
        })
    }

    pub fn history(&self, after: u64, limit: u32) -> Result<MatterHistory, &'static str> {
        if after > self.records.len() as u64 || !(1..=200).contains(&limit) {
            return Err("invalid matter history cursor or limit");
        }
        let end = (after as usize + limit as usize).min(self.records.len());
        Ok(MatterHistory {
            tick: self.tick,
            cursor: end as u64,
            has_more: end < self.records.len(),
            records: self.records[after as usize..end]
                .iter()
                .enumerate()
                .map(|(offset, record)| MatterUpdate {
                    sequence: after + offset as u64 + 1,
                    record: record.clone(),
                })
                .collect(),
        })
    }

    pub fn advance_tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        self.active_records.retain(|&index| {
            self.records[index]
                .expires_at_tick
                .is_some_and(|end| end > self.tick)
        });
    }

    fn push(&mut self, mut record: MatterRecord) {
        self.active_records.retain(|&index| {
            self.records[index]
                .expires_at_tick
                .is_some_and(|end| end > self.tick)
        });
        let mut group = Vec::new();
        let mut remaining = self.active_records.clone();
        // Connected overlap groups expire together; removing an older proportional
        // withdrawal alone would change the stock used by a later withdrawal.
        loop {
            let before = remaining.len();
            remaining.retain(|&index| {
                if overlaps(&record, &self.records[index])
                    || group
                        .iter()
                        .any(|&other| overlaps(&self.records[other], &self.records[index]))
                {
                    group.push(index);
                    false
                } else {
                    true
                }
            });
            if before == remaining.len() {
                break;
            }
        }
        let excess: f64 = record.added_mg.values().sum::<f64>()
            + group
                .iter()
                .flat_map(|&index| self.records[index].added_mg.values())
                .sum::<f64>();
        let deadline = self.tick.saturating_add(recovery_ticks(excess));
        let revision = self.records.len() as u64 + 1;
        for index in group {
            self.records[index].expires_at_tick = Some(deadline);
            self.records[index].updated_at_sequence = revision;
        }
        record.expires_at_tick = Some(deadline);
        record.updated_at_sequence = revision;
        self.active_records.push(self.records.len());
        self.records.push(record);
    }

    pub fn at(&self, world: &World, position: MapPosition) -> MatterAmounts {
        let target = baseline(world, position);
        let mut amounts = target.clone();
        let mut last_tick = 0;
        for &index in &self.active_records {
            let record = &self.records[index];
            if record.expires_at_tick.is_some_and(|end| end <= self.tick) {
                continue;
            }
            if !(TerrainArea::Circle {
                center: record.center,
                radius_px: record.radius_px,
            })
            .contains(position)
            {
                continue;
            }
            relax(&mut amounts, &target, record.tick.saturating_sub(last_tick));
            for mass in amounts.values_mut() {
                *mass *= 1.0 - record.removed_fraction;
            }
            for (id, added) in &record.added_mg {
                *amounts.entry(id.clone()).or_default() += added / f64::from(record.cell_count);
            }
            last_tick = record.tick;
        }
        relax(&mut amounts, &target, self.tick.saturating_sub(last_tick));
        amounts
    }

    pub fn area(
        &self,
        world: &World,
        center: MapPosition,
        radius_px: u32,
    ) -> Result<AreaMatter, &'static str> {
        let positions = cells(center, radius_px)?;
        let cell_count = positions.len() as u32;
        let mut total_mg = MatterAmounts::new();
        for position in positions {
            for (id, amount) in self.at(world, position) {
                *total_mg.entry(id).or_default() += amount;
            }
        }
        let average_mg_per_cell = total_mg
            .iter()
            .map(|(id, amount)| (id.clone(), amount / f64::from(cell_count)))
            .collect();
        Ok(AreaMatter {
            cell_count,
            total_mg,
            average_mg_per_cell,
        })
    }

    pub fn absorb(
        &mut self,
        world: &World,
        center: MapPosition,
        radius_px: u32,
        requested_mg: f64,
        source: Option<MatterSource>,
    ) -> Result<MatterAmounts, &'static str> {
        if !requested_mg.is_finite() || requested_mg < 0.0 {
            return Err("invalid absorption mass");
        }
        let area = self.area(world, center, radius_px)?;
        let total: f64 = area.total_mg.values().sum();
        if total <= 0.0 || requested_mg == 0.0 {
            return Ok(BTreeMap::new());
        }
        let fraction = (requested_mg / total).min(1.0);
        let absorbed_mg: MatterAmounts = area
            .total_mg
            .into_iter()
            .map(|(id, mass)| (id, mass * fraction))
            .collect();
        self.push(MatterRecord {
            tick: self.tick,
            center,
            radius_px,
            cell_count: area.cell_count,
            absorbed_mg: absorbed_mg.clone(),
            added_mg: BTreeMap::new(),
            removed_fraction: fraction,
            source,
            expires_at_tick: None,
            updated_at_sequence: 0,
            gas_absorbed_ul: BTreeMap::new(),
            gas_added_ul: BTreeMap::new(),
        });
        Ok(absorbed_mg)
    }

    /// Spread deposited solids/liquids uniformly over the covered cells.
    pub fn deposit(
        &mut self,
        center: MapPosition,
        radius_px: u32,
        amounts: MatterAmounts,
        source: Option<MatterSource>,
    ) -> Result<(), &'static str> {
        if amounts.is_empty()
            || amounts.values().any(|m| !m.is_finite() || *m <= 0.0)
            || !amounts.values().sum::<f64>().is_finite()
        {
            return Err("invalid deposited mass");
        }
        let cell_count = cells(center, radius_px)?.len() as u32;
        self.push(MatterRecord {
            tick: self.tick,
            center,
            radius_px,
            cell_count,
            absorbed_mg: BTreeMap::new(),
            added_mg: amounts,
            removed_fraction: 0.0,
            source,
            expires_at_tick: None,
            updated_at_sequence: 0,
            gas_absorbed_ul: BTreeMap::new(),
            gas_added_ul: BTreeMap::new(),
        });
        Ok(())
    }

    pub fn record_gas_exchange(
        &mut self,
        position: MapPosition,
        source: MatterSource,
        absorbed_ul: BTreeMap<ElementId, u64>,
        added_ul: BTreeMap<ElementId, u64>,
    ) {
        if absorbed_ul
            .values()
            .chain(added_ul.values())
            .all(|&amount| amount == 0)
        {
            return;
        }
        self.records.push(MatterRecord {
            tick: self.tick,
            center: position,
            radius_px: 0,
            cell_count: 1,
            absorbed_mg: BTreeMap::new(),
            added_mg: BTreeMap::new(),
            removed_fraction: 0.0,
            source: Some(source),
            expires_at_tick: Some(self.tick),
            updated_at_sequence: self.records.len() as u64 + 1,
            gas_absorbed_ul: absorbed_ul,
            gas_added_ul: added_ul,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{WorldGeneratorEditor, create_default_terrain_registry};

    fn flat(terrain: &str) -> World {
        WorldGeneratorEditor::new(&create_default_terrain_registry().unwrap(), terrain)
            .unwrap()
            .build()
    }
    fn total(amounts: &MatterAmounts) -> f64 {
        amounts.values().sum()
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn fertility_boundaries_and_default_composition() {
        for (nutrients, capacity) in [
            (0, 2.0),
            (20, 2.0),
            (21, 5.0),
            (80, 5.0),
            (81, 10.0),
            (100, 10.0),
        ] {
            assert_eq!(
                grass_capacity_mg(GroundProperties {
                    nutrients,
                    ..Default::default()
                }),
                capacity
            );
        }
        let world = flat("grassland");
        let area = world
            .matter
            .area(&world, MapPosition::default(), 5)
            .unwrap();
        assert_eq!(area.cell_count, 81);
        close(total(&area.total_mg), 162.0);
        close(total(&area.average_mg_per_cell), 2.0);
        close(
            area.average_mg_per_cell[&ElementId::new("cellulose").unwrap()],
            1.2,
        );
        let empty = flat("wasteland");
        assert!(empty.matter.at(&empty, MapPosition::default()).is_empty());
    }

    #[test]
    fn depleted_grass_recovers_fixed_capacity_fraction_and_stops_at_full() {
        let world = flat("grassland");
        let mut matter = WorldMatter::default();
        close(
            total(
                &matter
                    .absorb(&world, MapPosition::default(), 0, 2.0, None)
                    .unwrap(),
            ),
            2.0,
        );
        matter.advance_tick();
        close(total(&matter.at(&world, MapPosition::default())), 0.002);
        matter.tick = 500;
        close(total(&matter.at(&world, MapPosition::default())), 1.0);
        matter.tick = 1000;
        close(total(&matter.at(&world, MapPosition::default())), 2.0);
        matter.tick = 2000;
        close(total(&matter.at(&world, MapPosition::default())), 2.0);
    }

    #[test]
    fn depleted_cell_stays_empty_during_overlapping_absorption_in_the_same_tick() {
        let world = flat("grassland");
        let center = MapPosition::default();
        let mut matter = WorldMatter::default();
        close(
            total(&matter.absorb(&world, center, 0, 2.0, None).unwrap()),
            2.0,
        );
        assert!(matter.at(&world, center).is_empty());
        close(
            total(&matter.absorb(&world, center, 1, 1.0, None).unwrap()),
            1.0,
        );
        assert!(matter.at(&world, center).is_empty());
        close(
            total(&matter.area(&world, center, 1).unwrap().total_mg),
            7.0,
        );
        close(
            total(&matter.absorb(&world, center, 1, 100.0, None).unwrap()),
            7.0,
        );
        assert!(matter.area(&world, center, 1).unwrap().total_mg.is_empty());
        assert!(
            matter
                .absorb(&world, center, 1, 1.0, None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn deposit_on_depleted_cell_restores_nutrients_only_for_elapsed_ticks() {
        let world = flat("grassland");
        let center = MapPosition::default();
        let water = ElementId::new("water").unwrap();
        let mut matter = WorldMatter::default();
        matter.absorb(&world, center, 0, 2.0, None).unwrap();
        matter
            .deposit(center, 0, [(water.clone(), 1.0)].into(), None)
            .unwrap();
        assert_eq!(matter.at(&world, center), [(water.clone(), 1.0)].into());
        let mut restored: WorldMatter =
            serde_json::from_slice(&serde_json::to_vec(&matter).unwrap()).unwrap();
        assert_eq!(restored.at(&world, center), matter.at(&world, center));
        restored.advance_tick();
        let amounts = restored.at(&world, center);
        close(amounts[&water], 0.999);
        for (id, mass) in baseline(&world, center) {
            close(amounts[&id], mass * 0.001);
        }
        close(total(&amounts), 1.001);
    }

    #[test]
    fn excess_decays_exponentially_until_the_restoration_threshold() {
        let world = flat("wasteland");
        let mut matter = WorldMatter::default();
        matter
            .deposit(
                MapPosition::default(),
                0,
                [(ElementId::new("water").unwrap(), 10.0)].into(),
                None,
            )
            .unwrap();
        matter.advance_tick();
        close(total(&matter.at(&world, MapPosition::default())), 9.99);
        matter.tick = 10000;
        let remaining = total(&matter.at(&world, MapPosition::default()));
        close(remaining, 10.0 * 0.999_f64.powi(10000));
        assert!(remaining > 0.0);
        assert!(matter.at(&world, MapPosition { x: 1, y: 0 }).is_empty());
        matter.tick = matter.records[0].expires_at_tick.unwrap();
        assert!(matter.at(&world, MapPosition::default()).is_empty());
        matter.advance_tick();
        assert!(matter.active_records.is_empty());
        assert_eq!(matter.records.len(), 1);
    }

    #[test]
    fn overlapping_records_conserve_mass_and_survive_serialization() {
        let world = flat("grassland");
        let mut matter = WorldMatter::default();
        let first = MapPosition::default();
        let second = MapPosition { x: 2, y: 0 };
        let positions: Vec<_> = cells(first, 7).unwrap();
        let before: f64 = positions
            .iter()
            .map(|&p| total(&matter.at(&world, p)))
            .sum();
        close(
            total(&matter.absorb(&world, first, 5, 50.0, None).unwrap()),
            50.0,
        );
        close(
            total(&matter.absorb(&world, second, 5, 50.0, None).unwrap()),
            50.0,
        );
        let after: f64 = positions
            .iter()
            .map(|&p| total(&matter.at(&world, p)))
            .sum();
        close(before - after, 100.0);
        let restored: WorldMatter =
            serde_json::from_slice(&serde_json::to_vec(&matter).unwrap()).unwrap();
        assert_eq!(restored, matter);
        for position in positions {
            assert_eq!(restored.at(&world, position), matter.at(&world, position));
        }
        let mut value = serde_json::to_value(&world).unwrap();
        value.as_object_mut().unwrap().remove("matter");
        let legacy: World = serde_json::from_value(value).unwrap();
        assert_eq!(legacy, world);
        assert!(matter.absorb(&world, first, 5, f64::NAN, None).is_err());
        assert!(matter.area(&world, first, 65).is_err());
    }

    #[test]
    fn restored_history_is_retained_but_no_longer_queried_after_restart() {
        let world = flat("grassland");
        let center = MapPosition::default();
        let mut matter = WorldMatter::default();
        let source = MatterSource {
            organism_id: Uuid::new_v4(),
            component_id: Uuid::new_v4(),
        };
        matter
            .absorb(&world, center, 0, 2.0, Some(source.clone()))
            .unwrap();
        let deadline = matter.records[0].expires_at_tick.unwrap();
        matter.tick = deadline - 1;
        matter.advance_tick();
        assert!(matter.active_records.is_empty());
        assert_eq!(matter.at(&world, center), baseline(&world, center));
        assert_eq!(matter.records[0].source, Some(source));
        let saved = serde_json::to_value(&matter).unwrap();
        assert!(saved.get("active_records").is_none());
        let mut restored: WorldMatter = serde_json::from_value(saved).unwrap();
        assert_eq!(restored, matter);
        restored.absorb(&world, center, 0, 1.0, None).unwrap();
        close(total(&restored.at(&world, center)), 1.0);
        assert_eq!(restored.active_records, vec![1]);
        assert_eq!(restored.records.len(), 2);
    }

    #[test]
    fn overlap_chains_extend_old_deadlines_and_deltas_include_the_updates() {
        let world = flat("grassland");
        let mut matter = WorldMatter::default();
        let first = MapPosition::default();
        let second = MapPosition { x: 2, y: 0 };
        let third = MapPosition { x: 4, y: 0 };
        matter.absorb(&world, first, 1, 5.0, None).unwrap();
        let initial = matter.sync(first, 5, None).unwrap();
        assert!(initial.reset);
        assert_eq!(initial.records.len(), 1);
        let old_deadline = matter.records[0].expires_at_tick.unwrap();
        matter.tick = 900;
        matter.absorb(&world, second, 1, 5.0, None).unwrap();
        matter.tick = 950;
        matter.absorb(&world, third, 1, 5.0, None).unwrap();
        let deadline = matter.records[2].expires_at_tick.unwrap();
        assert!(deadline > old_deadline);
        assert!(
            matter
                .records
                .iter()
                .all(|record| record.expires_at_tick == Some(deadline))
        );
        let delta = matter.sync(first, 5, Some(initial.cursor)).unwrap();
        assert!(!delta.reset);
        assert_eq!(delta.records.len(), 3);
        assert_eq!(delta.records[0].sequence, 1);
        assert_eq!(delta.records[0].record.expires_at_tick, Some(deadline));
        assert!(
            matter
                .sync(first, 5, Some(delta.cursor))
                .unwrap()
                .records
                .is_empty()
        );
        matter.tick = old_deadline;
        assert!(total(&matter.at(&world, third)) < 2.0);
        matter.tick = deadline;
        matter.advance_tick();
        assert!(matter.active_records.is_empty());
        for position in cells(first, 6).unwrap() {
            assert_eq!(matter.at(&world, position), baseline(&world, position));
        }
    }

    #[test]
    fn distant_records_expire_independently_and_history_is_paginated() {
        let world = flat("grassland");
        let mut matter = WorldMatter::default();
        let first = MapPosition::default();
        matter.absorb(&world, first, 0, 1.0, None).unwrap();
        matter.tick = 900;
        matter
            .absorb(&world, MapPosition { x: 10, y: 0 }, 0, 1.0, None)
            .unwrap();
        matter.tick = matter.records[0].expires_at_tick.unwrap();
        matter.advance_tick();
        assert_eq!(matter.active_records, vec![1]);
        assert!(matter.sync(first, 0, None).unwrap().records.is_empty());
        let page = matter.history(0, 1).unwrap();
        assert!(page.has_more);
        assert_eq!(page.records[0].sequence, 1);
        let page = matter.history(page.cursor, 1).unwrap();
        assert!(!page.has_more);
        assert_eq!(page.records[0].sequence, 2);
        assert!(matter.history(3, 1).is_err());
        assert!(matter.history(0, 201).is_err());
        assert!(matter.sync(first, 65, None).is_err());
        assert!(matter.sync(first, 0, Some(3)).is_err());
    }

    #[test]
    fn legacy_records_migrate_and_gas_history_never_enters_the_active_index() {
        let world = flat("grassland");
        let center = MapPosition::default();
        let mut original = WorldMatter::default();
        original.absorb(&world, center, 0, 1.0, None).unwrap();
        let mut saved = serde_json::to_value(&original).unwrap();
        for record in saved["records"].as_array_mut().unwrap() {
            for key in [
                "expires_at_tick",
                "updated_at_sequence",
                "source",
                "gas_absorbed_ul",
                "gas_added_ul",
            ] {
                record.as_object_mut().unwrap().remove(key);
            }
        }
        let mut restored: WorldMatter = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.at(&world, center), original.at(&world, center));
        restored.record_gas_exchange(
            center,
            MatterSource {
                organism_id: Uuid::new_v4(),
                component_id: Uuid::new_v4(),
            },
            [(ElementId::new("oxygen").unwrap(), 100)].into(),
            BTreeMap::new(),
        );
        assert_eq!(restored.at(&world, center), original.at(&world, center));
        assert_eq!(restored.active_records, vec![0]);
        assert_eq!(restored.history(0, 100).unwrap().records.len(), 2);
        assert_eq!(restored.sync(center, 0, None).unwrap().records.len(), 1);
    }

    #[test]
    fn mixed_overlapping_operations_match_full_history_replay_until_recovery() {
        let world = flat("grassland");
        let center = MapPosition::default();
        let mut matter = WorldMatter::default();
        matter.absorb(&world, center, 2, 10.0, None).unwrap();
        matter.tick = 700;
        matter
            .deposit(
                MapPosition { x: 2, y: 0 },
                1,
                [
                    (ElementId::new("water").unwrap(), 3.0),
                    (ElementId::new("cellulose").unwrap(), 20.0),
                ]
                .into(),
                None,
            )
            .unwrap();
        matter.tick = 900;
        matter
            .absorb(&world, MapPosition { x: 1, y: 0 }, 2, 7.0, None)
            .unwrap();
        for tick in [900, 1001, 1901, 10000, 30000] {
            matter.tick = tick;
            matter.advance_tick();
            let restored: WorldMatter =
                serde_json::from_slice(&serde_json::to_vec(&matter).unwrap()).unwrap();
            for position in cells(center, 4).unwrap() {
                let target = baseline(&world, position);
                let mut full = target.clone();
                let mut last = 0;
                for record in &matter.records {
                    if !intersects_region(record, position, 0) {
                        continue;
                    }
                    relax(&mut full, &target, record.tick - last);
                    for mass in full.values_mut() {
                        *mass *= 1.0 - record.removed_fraction;
                    }
                    for (id, added) in &record.added_mg {
                        *full.entry(id.clone()).or_default() +=
                            added / f64::from(record.cell_count);
                    }
                    last = record.tick;
                }
                relax(&mut full, &target, matter.tick - last);
                assert_eq!(matter.at(&world, position), full);
                assert_eq!(restored.at(&world, position), full);
            }
        }
        assert!(matter.active_records.is_empty());
        assert_eq!(matter.records.len(), 3);
    }
}
