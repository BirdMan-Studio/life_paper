use super::ElementRegistry;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

/// Supported atoms, independent of registered substances such as nitrogen (N2).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Atom {
    C,
    H,
    O,
    N,
}

impl Atom {
    pub const fn relative_mass(self) -> u32 {
        match self {
            Self::C => 12,
            Self::H => 1,
            Self::O => 16,
            Self::N => 14,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChemicalProperties {
    /// Atom counts in a molecule, empirical unit, or polymer repeat unit.
    pub atoms: BTreeMap<Atom, u32>,
    /// Signed game energy relative to elemental substances, not nutritional yield.
    pub chemical_energy_mj_per_mg: f64,
}

impl ChemicalProperties {
    pub fn new(atoms: &[(Atom, u32)], chemical_energy_mj_per_mg: f64) -> Self {
        Self {
            atoms: atoms.iter().copied().collect(),
            chemical_energy_mj_per_mg,
        }
    }

    pub fn relative_mass(&self) -> f64 {
        self.atoms
            .iter()
            .map(|(atom, count)| f64::from(atom.relative_mass()) * f64::from(*count))
            .sum()
    }

    pub fn is_valid(&self) -> bool {
        !self.atoms.is_empty()
            && self.atoms.values().all(|n| *n > 0)
            && self.chemical_energy_mj_per_mg.is_finite()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReactionAmount {
    pub substance: String,
    pub mass_mg: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReactionEnergy {
    pub input_energy_mj: f64,
    pub output_energy_mj: f64,
    /// Negative releases energy to the organism; positive requires energy.
    pub reaction_energy_mj: f64,
    pub organism_energy_delta_mj: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChemistryError {
    EmptySide,
    InvalidAmount(String),
    UnknownSubstance(String),
    MissingChemistry(String),
    InvalidChemistry(String),
    NumericOverflow,
    Unbalanced(Atom),
}

impl fmt::Display for ChemistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ChemistryError {}

struct Totals {
    atoms: BTreeMap<Atom, f64>,
    energy: f64,
}

fn totals(
    registry: &ElementRegistry,
    amounts: &[ReactionAmount],
) -> Result<Totals, ChemistryError> {
    if amounts.is_empty() {
        return Err(ChemistryError::EmptySide);
    }
    let mut result = Totals {
        atoms: BTreeMap::new(),
        energy: 0.0,
    };
    for amount in amounts {
        if !amount.mass_mg.is_finite() || amount.mass_mg <= 0.0 {
            return Err(ChemistryError::InvalidAmount(amount.substance.clone()));
        }
        let definition = registry
            .get(&amount.substance)
            .ok_or_else(|| ChemistryError::UnknownSubstance(amount.substance.clone()))?;
        let properties = definition
            .chemistry
            .as_ref()
            .ok_or_else(|| ChemistryError::MissingChemistry(amount.substance.clone()))?;
        if !properties.is_valid() {
            return Err(ChemistryError::InvalidChemistry(amount.substance.clone()));
        }
        // mass in mg / relative molecular mass gives millimoles of formula units.
        let units = amount.mass_mg / properties.relative_mass();
        for (atom, count) in &properties.atoms {
            *result.atoms.entry(*atom).or_default() += units * f64::from(*count);
        }
        result.energy += amount.mass_mg * properties.chemical_energy_mj_per_mg;
        if !result.energy.is_finite() || result.atoms.values().any(|n| !n.is_finite()) {
            return Err(ChemistryError::NumericOverflow);
        }
    }
    Ok(result)
}

/// Evaluate only: no inventory, capacity, processing budget, or stored energy is changed.
pub fn evaluate_reaction(
    registry: &ElementRegistry,
    inputs: &[ReactionAmount],
    outputs: &[ReactionAmount],
) -> Result<ReactionEnergy, ChemistryError> {
    let input = totals(registry, inputs)?;
    let output = totals(registry, outputs)?;
    for atom in [Atom::C, Atom::H, Atom::O, Atom::N] {
        let a = input.atoms.get(&atom).copied().unwrap_or(0.0);
        let b = output.atoms.get(&atom).copied().unwrap_or(0.0);
        // Relative tolerance only: very small batches must still conserve atoms.
        if (a - b).abs() > a.abs().max(b.abs()) * 1e-9 {
            return Err(ChemistryError::Unbalanced(atom));
        }
    }
    let delta = output.energy - input.energy;
    if !delta.is_finite() {
        return Err(ChemistryError::NumericOverflow);
    }
    Ok(ReactionEnergy {
        input_energy_mj: input.energy,
        output_energy_mj: output.energy,
        reaction_energy_mj: delta,
        organism_energy_delta_mj: -delta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::element::create_default_element_registry;

    fn amounts(values: &[(&str, f64)]) -> Vec<ReactionAmount> {
        values
            .iter()
            .map(|(id, mass)| ReactionAmount {
                substance: (*id).into(),
                mass_mg: *mass,
            })
            .collect()
    }

    #[test]
    fn energy_sign_reverse_and_cycle_are_consistent() {
        let registry = create_default_element_registry().unwrap();
        let input = amounts(&[("hydrogen", 2.0), ("oxygen", 16.0)]);
        let output = amounts(&[("water", 18.0)]);
        let forward = evaluate_reaction(&registry, &input, &output).unwrap();
        let reverse = evaluate_reaction(&registry, &output, &input).unwrap();
        assert!(forward.reaction_energy_mj < 0.0);
        assert_eq!(forward.organism_energy_delta_mj, 576.0);
        assert_eq!(forward.reaction_energy_mj + reverse.reaction_energy_mj, 0.0);
    }

    #[test]
    fn sugar_and_nitrogen_are_balanced_by_atoms_not_display_strings() {
        let registry = create_default_element_registry().unwrap();
        assert!(
            evaluate_reaction(
                &registry,
                &amounts(&[("disaccharide", 342.0), ("water", 18.0)]),
                &amounts(&[("monosaccharide", 360.0)])
            )
            .is_ok()
        );
        assert!(
            evaluate_reaction(
                &registry,
                &amounts(&[("monosaccharide", 30.0), ("oxygen", 32.0)]),
                &amounts(&[("carbon_dioxide", 44.0), ("water", 18.0)])
            )
            .unwrap()
            .organism_energy_delta_mj
                > 0.0
        );
        assert!(
            evaluate_reaction(
                &registry,
                &amounts(&[("nitrogen", 28.0), ("hydrogen", 6.0)]),
                &amounts(&[("ammonia", 34.0)])
            )
            .is_ok()
        );
        let same = evaluate_reaction(
            &registry,
            &amounts(&[("cellulose", 162.0)]),
            &amounts(&[("polysaccharide", 162.0)]),
        )
        .unwrap();
        assert_eq!(same.reaction_energy_mj, 0.0);
        assert!(
            evaluate_reaction(
                &registry,
                &amounts(&[("carbon", 18.0)]),
                &amounts(&[("water", 18.0)])
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_data_and_tiny_unbalanced_batches_are_rejected() {
        let registry = create_default_element_registry().unwrap();
        for mass in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                evaluate_reaction(
                    &registry,
                    &amounts(&[("water", mass)]),
                    &amounts(&[("water", 1.0)])
                )
                .is_err()
            );
        }
        assert!(evaluate_reaction(&registry, &[], &amounts(&[("water", 1.0)])).is_err());
        assert!(
            evaluate_reaction(
                &registry,
                &amounts(&[("missing", 1.0)]),
                &amounts(&[("water", 1.0)])
            )
            .is_err()
        );
        assert!(
            evaluate_reaction(
                &registry,
                &amounts(&[("water", 1e-20)]),
                &amounts(&[("water", 2e-20)])
            )
            .is_err()
        );
        let mut legacy = serde_json::to_value(registry.get("water").unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("chemistry");
        let restored: super::super::ElementDefinition = serde_json::from_value(legacy).unwrap();
        assert!(restored.chemistry.is_none());
    }

    #[test]
    fn lipid_oxidation_releases_more_energy_per_mass_than_sugar() {
        let registry = create_default_element_registry().unwrap();
        let input = amounts(&[("lipid", 284.0), ("oxygen", 832.0)]);
        let output = amounts(&[("carbon_dioxide", 792.0), ("water", 324.0)]);
        let forward = evaluate_reaction(&registry, &input, &output).unwrap();
        let reverse = evaluate_reaction(&registry, &output, &input).unwrap();
        let sugar = evaluate_reaction(
            &registry,
            &amounts(&[("monosaccharide", 30.0), ("oxygen", 32.0)]),
            &amounts(&[("carbon_dioxide", 44.0), ("water", 18.0)]),
        )
        .unwrap();
        assert!(forward.organism_energy_delta_mj / 284.0 > sugar.organism_energy_delta_mj / 30.0);
        assert!((forward.organism_energy_delta_mj - 22749.6).abs() < 1e-8);
        assert!((sugar.organism_energy_delta_mj - 948.0).abs() < 1e-8);
        assert_eq!(forward.reaction_energy_mj, -reverse.reaction_energy_mj);
    }

    #[test]
    fn oxidation_yields_share_the_same_energy_reference() {
        let registry = create_default_element_registry().unwrap();
        for (fuel, mass, oxygen, carbon_dioxide, water, expected) in [
            ("carbon", 12.0, 32.0, 44.0, 0.0, 792.0),
            ("disaccharide", 342.0, 384.0, 528.0, 198.0, 11394.0),
            ("polysaccharide", 162.0, 192.0, 264.0, 90.0, 5850.0),
            ("cellulose", 162.0, 192.0, 264.0, 90.0, 5850.0),
        ] {
            let input = amounts(&[(fuel, mass), ("oxygen", oxygen)]);
            let mut output = amounts(&[("carbon_dioxide", carbon_dioxide)]);
            if water > 0.0 {
                output.extend(amounts(&[("water", water)]));
            }
            let forward = evaluate_reaction(&registry, &input, &output).unwrap();
            let reverse = evaluate_reaction(&registry, &output, &input).unwrap();
            assert!((forward.organism_energy_delta_mj - expected).abs() < 1e-8);
            assert_eq!(forward.reaction_energy_mj, -reverse.reaction_energy_mj);
        }
    }

    #[test]
    fn five_ml_of_reference_air_preserves_the_target_sugar_energy_yield() {
        let registry = create_default_element_registry().unwrap();
        // Balance reference only: air transfer and volumetric density are not simulated yet.
        let oxygen_mg = 5.0 * 1.429 * 0.21;
        let progress = oxygen_mg / 32.0;
        let reaction = evaluate_reaction(
            &registry,
            &amounts(&[("monosaccharide", 30.0 * progress), ("oxygen", oxygen_mg)]),
            &amounts(&[
                ("carbon_dioxide", 44.0 * progress),
                ("water", 18.0 * progress),
            ]),
        )
        .unwrap();
        assert!((reaction.organism_energy_delta_mj - 44.45083125).abs() < 1e-8);
    }

    #[test]
    fn chemistry_registration_rejects_invalid_properties_without_mutating() {
        let mut registry = create_default_element_registry().unwrap();
        let before = registry.get("water").unwrap().clone();
        for invalid in [
            ChemicalProperties::new(&[], 0.0),
            ChemicalProperties::new(&[(Atom::H, 0)], 0.0),
            ChemicalProperties::new(&[(Atom::H, 1)], f64::NAN),
            ChemicalProperties::new(&[(Atom::H, 1)], f64::INFINITY),
        ] {
            assert!(registry.set_chemistry("water", invalid).is_err());
            assert_eq!(registry.get("water").unwrap(), &before);
        }
        assert!(
            registry
                .set_chemistry("missing", ChemicalProperties::new(&[(Atom::H, 1)], 0.0))
                .is_err()
        );
        let mut legacy = serde_json::to_value(&registry).unwrap();
        legacy["definitions"]["water"]
            .as_object_mut()
            .unwrap()
            .remove("chemistry");
        let restored: ElementRegistry = serde_json::from_value(legacy).unwrap();
        assert!(matches!(
            evaluate_reaction(
                &restored,
                &amounts(&[("water", 1.0)]),
                &amounts(&[("water", 1.0)])
            ),
            Err(ChemistryError::MissingChemistry(_))
        ));
    }

    #[test]
    fn energy_is_additive_and_numeric_overflow_is_rejected() {
        let mut registry = create_default_element_registry().unwrap();
        let input = amounts(&[("nitrogen", 28.0), ("hydrogen", 6.0)]);
        let output = amounts(&[("ammonia", 34.0)]);
        let a = evaluate_reaction(&registry, &input, &output).unwrap();
        let b = evaluate_reaction(
            &registry,
            &amounts(&[("nitrogen", 14.0), ("nitrogen", 14.0), ("hydrogen", 6.0)]),
            &output,
        )
        .unwrap();
        assert_eq!(a, b);
        registry
            .set_chemistry(
                "water",
                ChemicalProperties::new(&[(Atom::H, 2), (Atom::O, 1)], f64::MAX),
            )
            .unwrap();
        assert_eq!(
            evaluate_reaction(
                &registry,
                &amounts(&[("water", 2.0)]),
                &amounts(&[("water", 2.0)])
            ),
            Err(ChemistryError::NumericOverflow)
        );
    }
}
