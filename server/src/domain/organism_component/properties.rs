use serde::{Deserialize, Serialize};

/// Integer base units keep runtime quantities and snapshots exact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComponentProperties {
    #[serde(default)]
    pub api: Option<super::ComponentApi>,
    pub max_durability: u32,
    pub damage_weight: u32,
    pub repair_weight: u32,
    pub mass_mg: Option<u64>,
    pub energy_capacity_mj: u64,
    pub non_gas_capacity_mg: u64,
    pub gas_capacity_ul: u64,
    pub reaction_min_mj_per_tick: i32,
    pub reaction_max_mj_per_tick: i32,
    pub sensor_slots: u32,
}

impl Default for ComponentProperties {
    fn default() -> Self {
        Self {
            api: None,
            max_durability: 100,
            damage_weight: 10,
            repair_weight: 10,
            mass_mg: None,
            energy_capacity_mj: 0,
            non_gas_capacity_mg: 0,
            gas_capacity_ul: 0,
            reaction_min_mj_per_tick: 0,
            reaction_max_mj_per_tick: 0,
            sensor_slots: 0,
        }
    }
}
