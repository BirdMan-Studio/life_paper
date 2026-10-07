use super::MatterError;
use crate::domain::element::{ElementId, ElementRegistry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 构成一个物质实体的元素质量，质量单位为克。
///
/// 组成不携带物质类别；受体可直接根据这里的元素及其质量进行识别。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ElementMixture {
    masses_grams: BTreeMap<ElementId, f64>,
}

impl ElementMixture {
    /// 根据元素注册表创建混合物。重复元素会自动合并质量。
    pub fn new<I, S>(registry: &ElementRegistry, masses: I) -> Result<Self, MatterError>
    where
        I: IntoIterator<Item = (S, f64)>,
        S: AsRef<str>,
    {
        let mut masses_grams = BTreeMap::new();

        for (element_id, mass_grams) in masses {
            let element_id = element_id.as_ref();
            let definition = registry
                .get(element_id)
                .ok_or_else(|| MatterError::UnknownElement(element_id.to_owned()))?;

            if !mass_grams.is_finite() || mass_grams <= 0.0 {
                return Err(MatterError::InvalidElementMass {
                    element_id: element_id.to_owned(),
                    mass_grams,
                });
            }

            let accumulated_mass = masses_grams.entry(definition.id.clone()).or_insert(0.0);
            *accumulated_mass += mass_grams;
            if !accumulated_mass.is_finite() {
                return Err(MatterError::InvalidElementMass {
                    element_id: element_id.to_owned(),
                    mass_grams: *accumulated_mass,
                });
            }
        }

        if masses_grams.is_empty() {
            return Err(MatterError::EmptyComposition);
        }

        Ok(Self { masses_grams })
    }

    pub fn mass_grams(&self, element_id: &str) -> Option<f64> {
        self.masses_grams.get(element_id).copied()
    }

    pub fn total_mass_grams(&self) -> f64 {
        self.masses_grams.values().sum()
    }

    pub fn element_mass_per_occupied_pixel(
        &self,
        element_id: &str,
        occupied_pixel_count: usize,
    ) -> Option<f64> {
        if occupied_pixel_count == 0 {
            return None;
        }

        self.mass_grams(element_id)
            .map(|mass| mass / occupied_pixel_count as f64)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ElementId, f64)> {
        self.masses_grams
            .iter()
            .map(|(element_id, mass)| (element_id, *mass))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::create_default_element_registry;

    #[test]
    fn validates_and_merges_registered_element_masses() {
        let registry = create_default_element_registry().unwrap();
        let mixture =
            ElementMixture::new(&registry, [("water", 2.0), ("oxygen", 1.0), ("water", 3.0)])
                .unwrap();

        assert_eq!(mixture.mass_grams("water"), Some(5.0));
        assert_eq!(mixture.total_mass_grams(), 6.0);
        assert_eq!(
            mixture.element_mass_per_occupied_pixel("water", 5),
            Some(1.0)
        );
    }

    #[test]
    fn rejects_empty_unknown_and_invalid_masses() {
        let registry = create_default_element_registry().unwrap();

        assert!(matches!(
            ElementMixture::new::<[(&str, f64); 0], &str>(&registry, []),
            Err(MatterError::EmptyComposition)
        ));
        assert!(matches!(
            ElementMixture::new(&registry, [("missing", 1.0)]),
            Err(MatterError::UnknownElement(_))
        ));

        for invalid_mass in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                ElementMixture::new(&registry, [("water", invalid_mass)]),
                Err(MatterError::InvalidElementMass { .. })
            ));
        }
    }
}
