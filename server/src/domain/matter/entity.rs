use super::{ElementMixture, MatterError, MatterStructure};
use crate::domain::world::MapPosition;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 散落在地图上的物质实体。它没有碰撞箱，也不携带物质类别。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MatterEntity {
    pub id: Uuid,
    pub position: MapPosition,
    pub temperature_celsius: f64,
    pub structure: MatterStructure,
    pub elements: ElementMixture,
}

impl MatterEntity {
    pub fn new(
        position: MapPosition,
        temperature_celsius: f64,
        structure: MatterStructure,
        elements: ElementMixture,
    ) -> Result<Self, MatterError> {
        Self::with_id(
            Uuid::new_v4(),
            position,
            temperature_celsius,
            structure,
            elements,
        )
    }

    pub fn with_id(
        id: Uuid,
        position: MapPosition,
        temperature_celsius: f64,
        structure: MatterStructure,
        elements: ElementMixture,
    ) -> Result<Self, MatterError> {
        if !temperature_celsius.is_finite() {
            return Err(MatterError::InvalidTemperature(temperature_celsius));
        }

        Ok(Self {
            id,
            position,
            temperature_celsius,
            structure,
            elements,
        })
    }

    /// 查询该元素均匀分布后，每个被占用像素包含的质量。
    pub fn element_mass_per_occupied_pixel(&self, element_id: &str) -> Option<f64> {
        self.elements
            .element_mass_per_occupied_pixel(element_id, self.structure.occupied_pixel_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::create_default_element_registry;

    #[test]
    fn creates_unclassified_map_matter_with_uniform_elements() {
        let registry = create_default_element_registry().unwrap();
        let elements = ElementMixture::new(&registry, [("water", 8.0)]).unwrap();
        let structure = MatterStructure::new(2, 2, vec![true, false, true, true]).unwrap();
        let matter =
            MatterEntity::new(MapPosition { x: -4, y: 10 }, 20.0, structure, elements).unwrap();

        assert_eq!(
            matter.element_mass_per_occupied_pixel("water"),
            Some(8.0 / 3.0)
        );
    }

    #[test]
    fn rejects_non_finite_temperature() {
        let registry = create_default_element_registry().unwrap();
        let elements = ElementMixture::new(&registry, [("water", 1.0)]).unwrap();
        let structure = MatterStructure::filled(1, 1).unwrap();

        assert!(matches!(
            MatterEntity::new(MapPosition { x: 0, y: 0 }, f64::NAN, structure, elements),
            Err(MatterError::InvalidTemperature(_))
        ));
    }
}
