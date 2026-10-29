use super::ComponentCategory;
use crate::domain::organism::ComponentSize;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Geometry {
    Capsule,
    Ellipse,
    Rectangle,
    Surface,
    HostOutline,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Assembly {
    Free,
    Surface,
    Wrap,
    Interior,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Collision {
    None,
    Geometry,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Appearance {
    pub fill: String,
    pub stroke: String,
    pub stroke_width_px: u32,
    /// Visual offset only; never expands the collision geometry.
    pub protrusion_px: u32,
    pub range_radius_px: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ComponentModel {
    pub geometry: Geometry,
    pub size: ComponentSize,
    /// Reserved for growth: absent bounds mean the current size is fixed.
    pub size_bounds: Option<SizeBounds>,
    pub assembly: Assembly,
    pub collision: Collision,
    pub appearance: Appearance,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SizeBounds {
    pub min: ComponentSize,
    pub max: ComponentSize,
}

impl ComponentModel {
    pub fn for_category(category: ComponentCategory) -> Self {
        let (geometry, assembly, collision, width_px, length_px, fill, stroke) = match category {
            ComponentCategory::Body => (
                Geometry::Capsule,
                Assembly::Free,
                Collision::Geometry,
                40,
                80,
                "#234339",
                "#668e70",
            ),
            ComponentCategory::ExternalOrgan => (
                Geometry::Surface,
                Assembly::Surface,
                Collision::None,
                0,
                15,
                "none",
                "#102724",
            ),
            ComponentCategory::InternalOrgan => (
                Geometry::Ellipse,
                Assembly::Interior,
                Collision::None,
                12,
                12,
                "#40364f",
                "#9b8bbb",
            ),
        };
        Self {
            geometry,
            assembly,
            collision,
            size: ComponentSize {
                width_px,
                length_px,
            },
            size_bounds: None,
            appearance: Appearance {
                fill: fill.into(),
                stroke: stroke.into(),
                stroke_width_px: if width_px == 0 { 2 } else { 1 },
                protrusion_px: if width_px == 0 { 1 } else { 0 },
                range_radius_px: None,
            },
        }
    }

    pub fn accepts_size(&self, size: ComponentSize) -> bool {
        if size.width_px > size.length_px {
            return false;
        }
        if let Some(bounds) = &self.size_bounds {
            size.width_px >= bounds.min.width_px
                && size.width_px <= bounds.max.width_px
                && size.length_px >= bounds.min.length_px
                && size.length_px <= bounds.max.length_px
        } else {
            size == self.size
        }
    }

    pub fn valid_for(&self, category: ComponentCategory) -> bool {
        let category_valid = match category {
            ComponentCategory::Body => {
                self.geometry == Geometry::Capsule
                    && self.assembly == Assembly::Free
                    && self.collision == Collision::Geometry
            }
            ComponentCategory::ExternalOrgan => {
                self.collision == Collision::None
                    && self.size.width_px == 0
                    && matches!(
                        (self.geometry, self.assembly),
                        (Geometry::Surface, Assembly::Surface)
                            | (Geometry::HostOutline, Assembly::Wrap)
                    )
            }
            ComponentCategory::InternalOrgan => {
                self.collision == Collision::None
                    && self.assembly == Assembly::Interior
                    && matches!(self.geometry, Geometry::Ellipse | Geometry::Rectangle)
            }
        };
        let sizes_valid = |size: ComponentSize| {
            size.length_px > 0
                && size.length_px <= 100_000
                && size.width_px <= size.length_px
                && if category == ComponentCategory::ExternalOrgan {
                    size.width_px == 0
                } else {
                    size.width_px > 0
                }
        };
        category_valid
            && sizes_valid(self.size)
            && self.accepts_size(self.size)
            && self.size_bounds.as_ref().is_none_or(|b| {
                sizes_valid(b.min)
                    && sizes_valid(b.max)
                    && b.min.width_px <= b.max.width_px
                    && b.min.length_px <= b.max.length_px
            })
            && self.appearance.stroke_width_px > 0
            && self.appearance.stroke_width_px <= 16
            && self.appearance.protrusion_px <= 16
            && valid_color(&self.appearance.fill)
            && valid_color(&self.appearance.stroke)
            && self
                .appearance
                .range_radius_px
                .is_none_or(|r| r > 0 && r <= 100_000)
    }
}

fn valid_color(value: &str) -> bool {
    value == "none"
        || (value.len() == 7
            && value.starts_with('#')
            && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_collision_thickness_and_growth_bounds() {
        let mut organ = ComponentModel::for_category(ComponentCategory::ExternalOrgan);
        organ.collision = Collision::Geometry;
        assert!(!organ.valid_for(ComponentCategory::ExternalOrgan));
        organ.collision = Collision::None;
        organ.size.width_px = 2;
        assert!(!organ.valid_for(ComponentCategory::ExternalOrgan));
        let mut body = ComponentModel::for_category(ComponentCategory::Body);
        body.size_bounds = Some(SizeBounds {
            min: ComponentSize {
                width_px: 20,
                length_px: 40,
            },
            max: ComponentSize {
                width_px: 80,
                length_px: 160,
            },
        });
        assert!(body.valid_for(ComponentCategory::Body));
        assert!(body.accepts_size(ComponentSize {
            width_px: 60,
            length_px: 120
        }));
        assert!(!body.accepts_size(ComponentSize {
            width_px: 80,
            length_px: 40
        }));
        assert!(!body.accepts_size(ComponentSize {
            width_px: 81,
            length_px: 120
        }));
    }
}
