use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RateControl {
    pub max_rate_mg_per_tick: u32,
    /// Volume throughput in mL/tick. Legacy snapshots retain no volume limit.
    #[serde(default)]
    pub max_rate_ml_per_tick: Option<u32>,
    /// Optional gas-specific volume limit; otherwise use the general volume limit.
    #[serde(default)]
    pub max_gas_rate_ml_per_tick: Option<u32>,
    pub subdivisions: u32,
    pub base_energy_mj: u32,
    pub max_variable_energy_mj: u32,
}

impl RateControl {
    pub fn gas_throughput_fraction(
        &self,
        mass_mg: f64,
        volume_ml: f64,
    ) -> Result<f64, &'static str> {
        let mut gas = self.clone();
        gas.max_rate_ml_per_tick = self.max_gas_rate_ml_per_tick.or(self.max_rate_ml_per_tick);
        gas.throughput_fraction(mass_mg, volume_ml)
    }

    /// Scale the entire candidate batch together, preserving mixture proportions.
    pub fn throughput_fraction(&self, mass_mg: f64, volume_ml: f64) -> Result<f64, &'static str> {
        if !mass_mg.is_finite() || mass_mg < 0.0 || !volume_ml.is_finite() || volume_ml < 0.0 {
            return Err("mass and volume must be finite and nonnegative");
        }
        let mut fraction = 1.0_f64;
        if mass_mg > 0.0 {
            fraction = fraction.min(f64::from(self.max_rate_mg_per_tick) / mass_mg);
        }
        if let Some(limit) = self.max_rate_ml_per_tick {
            if volume_ml > 0.0 {
                fraction = fraction.min(f64::from(limit) / volume_ml);
            }
        }
        Ok(fraction)
    }

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
                max_rate_ml_per_tick: Some(2),
                max_gas_rate_ml_per_tick: None,
                subdivisions: 10,
                base_energy_mj: 5,
                max_variable_energy_mj: 10,
            }),
            ..Default::default()
        },
        "absorption_organ_1" => ComponentApi {
            absorption: Some(RateControl {
                max_rate_mg_per_tick: 50,
                max_rate_ml_per_tick: Some(1),
                max_gas_rate_ml_per_tick: Some(5),
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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentAction {
    Jet {
        material: String,
        rate_mg_per_tick: f64,
        angle_deg: f64,
    },
    AbsorbAir {
        rate_ml_per_tick: f64,
    },
    AbsorbMatter {
        rate_mg_per_tick: f64,
    },
}

impl<'de> Deserialize<'de> for ComponentAction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
        enum SavedAction {
            Jet {
                material: String,
                rate_mg_per_tick: f64,
                #[serde(default)]
                angle_deg: f64,
            },
            AbsorbAir {
                rate_ml_per_tick: Option<f64>,
                rate_mg_per_tick: Option<f64>,
            },
            AbsorbMatter {
                rate_mg_per_tick: f64,
            },
        }
        Ok(match SavedAction::deserialize(deserializer)? {
            SavedAction::Jet {
                material,
                rate_mg_per_tick,
                angle_deg,
            } => Self::Jet {
                material,
                rate_mg_per_tick,
                angle_deg,
            },
            SavedAction::AbsorbMatter { rate_mg_per_tick } => {
                Self::AbsorbMatter { rate_mg_per_tick }
            }
            SavedAction::AbsorbAir {
                rate_ml_per_tick,
                rate_mg_per_tick,
            } => {
                let rate = match (rate_ml_per_tick, rate_mg_per_tick) {
                    (Some(rate), None) => rate,
                    // Legacy commands requested air mass at the reference density.
                    (None, Some(rate)) if rate.is_finite() && rate >= 0.0 => {
                        (rate / 1.293).min(5.0)
                    }
                    _ => {
                        return Err(serde::de::Error::custom(
                            "air command requires exactly one rate",
                        ));
                    }
                };
                Self::AbsorbAir {
                    rate_ml_per_tick: rate,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_jet_commands_default_to_zero_deflection() {
        let action: ComponentAction = serde_json::from_value(serde_json::json!({
            "method": "jet", "material": "water", "rate_mg_per_tick": 31
        }))
        .unwrap();
        assert!(matches!(
            action,
            ComponentAction::Jet { angle_deg: 0.0, .. }
        ));
    }

    #[test]
    fn legacy_air_mass_commands_load_with_explicit_conversion() {
        let action: ComponentAction = serde_json::from_value(
            serde_json::json!({"method": "absorb_air", "rate_mg_per_tick": 1.293}),
        )
        .unwrap();
        assert_eq!(
            action,
            ComponentAction::AbsorbAir {
                rate_ml_per_tick: 1.0
            }
        );
        let value = serde_json::to_value(&action).unwrap();
        assert_eq!(value["rate_ml_per_tick"], 1.0);
        assert!(value.get("rate_mg_per_tick").is_none());
    }

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

    #[test]
    fn throughput_stops_at_the_first_mass_or_volume_limit() {
        let spec = builtin_component_api("absorption_organ_1")
            .absorption
            .unwrap();
        for (mass, volume, expected) in [
            (100.0, 0.5, 0.5),
            (10.0, 2.0, 0.5),
            (100.0, 2.0, 0.5),
            (10.0, 0.5, 1.0),
            (0.0, 0.0, 1.0),
        ] {
            let fraction = spec.throughput_fraction(mass, volume).unwrap();
            assert_eq!(fraction, expected);
            assert!(mass * fraction <= f64::from(spec.max_rate_mg_per_tick));
            assert!(volume * fraction <= f64::from(spec.max_rate_ml_per_tick.unwrap()));
        }
        for (mass, volume) in [
            (f64::NAN, 1.0),
            (1.0, f64::INFINITY),
            (-1.0, 1.0),
            (1.0, -1.0),
        ] {
            assert!(spec.throughput_fraction(mass, volume).is_err());
        }
        let mut legacy = serde_json::to_value(&spec).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("max_rate_ml_per_tick");
        legacy
            .as_object_mut()
            .unwrap()
            .remove("max_gas_rate_ml_per_tick");
        let restored: RateControl = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.max_rate_ml_per_tick, None);
        assert_eq!(restored.max_gas_rate_ml_per_tick, None);
        assert_eq!(restored.throughput_fraction(10.0, 2.0), Ok(1.0));
        assert_eq!(restored.gas_throughput_fraction(10.0, 10.0), Ok(1.0));
        assert_eq!(
            serde_json::from_value::<RateControl>(serde_json::to_value(&spec).unwrap()).unwrap(),
            spec
        );
        let jet = builtin_component_api("jet_organ_1").jet.unwrap();
        assert_eq!(jet.throughput_fraction(100.0, 4.0), Ok(0.5));
        assert_eq!(spec.gas_throughput_fraction(10.0, 10.0), Ok(0.5));
        assert_eq!(spec.gas_throughput_fraction(100.0, 1.0), Ok(0.5));
        assert_eq!(spec.gas_throughput_fraction(10.0, 5.0), Ok(1.0));
        assert_eq!(jet.gas_throughput_fraction(100.0, 4.0), Ok(0.5));
    }
}
