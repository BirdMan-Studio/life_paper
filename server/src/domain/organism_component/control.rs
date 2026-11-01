use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RateControl {
    pub max_rate_mg_per_tick: u32,
    pub subdivisions: u32,
    pub base_energy_mj: u32,
    pub max_variable_energy_mj: u32,
}

impl RateControl {
    pub fn validate(&self, rate: f64) -> Result<(), &'static str> {
        if !rate.is_finite() || rate < 0.0 || rate > f64::from(self.max_rate_mg_per_tick) {
            return Err("rate is outside the component's allowed range");
        }
        Ok(())
    }

    /// Energy consumed by one accepted command at this tick's requested rate.
    pub fn energy_cost(&self, rate: f64) -> u64 {
        let ratio = (rate.max(0.0) / f64::from(self.max_rate_mg_per_tick.max(1))).clamp(0.0, 1.0);
        u64::from(self.base_energy_mj)
            + (f64::from(self.max_variable_energy_mj) * ratio).ceil() as u64
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ComponentApi {
    pub base_energy_mj: Option<u32>,
    pub jet: Option<RateControl>,
    pub absorption: Option<RateControl>,
    pub vision: bool,
}

pub fn builtin_component_api(id: &str) -> ComponentApi {
    match id {
        "jet_organ_1" => ComponentApi {
            jet: Some(RateControl {
                max_rate_mg_per_tick: 200,
                subdivisions: 10,
                base_energy_mj: 5,
                max_variable_energy_mj: 10,
            }),
            ..Default::default()
        },
        "absorption_organ_1" => ComponentApi {
            absorption: Some(RateControl {
                max_rate_mg_per_tick: 50,
                subdivisions: 10,
                base_energy_mj: 3,
                max_variable_energy_mj: 5,
            }),
            ..Default::default()
        },
        "photosensor_organ_1" => ComponentApi {
            base_energy_mj: Some(2),
            vision: true,
            ..Default::default()
        },
        "skin_1" => ComponentApi {
            base_energy_mj: Some(2),
            ..Default::default()
        },
        _ => ComponentApi::default(),
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentCommand {
    pub component_id: Uuid,
    pub action: ComponentAction,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentAction {
    Jet {
        material: String,
        rate_mg_per_tick: f64,
    },
    AbsorbAir {
        rate_mg_per_tick: f64,
    },
    AbsorbMatter {
        rate_mg_per_tick: f64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_accepts_rates_between_precision_steps() {
        let api = builtin_component_api("jet_organ_1");
        let rate = api.jet.unwrap();
        for value in [0.0, 9.9, 10.0, 31.0, 200.0] {
            assert_eq!(rate.validate(value), Ok(()));
        }
        for value in [-1.0, 200.1, f64::NAN, f64::INFINITY] {
            assert!(rate.validate(value).is_err());
        }
        let rate = builtin_component_api("absorption_organ_1")
            .absorption
            .unwrap();
        assert_eq!(rate.validate(2.5), Ok(()));
    }
}
