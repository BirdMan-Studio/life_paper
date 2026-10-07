use super::MatterError;
use serde::{Deserialize, Serialize};

/// 物质在二维地图中占用的像素结构。
///
/// `occupied_pixels` 按从左到右、从上到下排列。元素在所有被占用的像素中均匀混合。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MatterStructure {
    size_x_px: u32,
    size_y_px: u32,
    occupied_pixels: Vec<bool>,
}

impl MatterStructure {
    pub fn new(
        size_x_px: u32,
        size_y_px: u32,
        occupied_pixels: Vec<bool>,
    ) -> Result<Self, MatterError> {
        if size_x_px == 0 || size_y_px == 0 {
            return Err(MatterError::InvalidStructureDimensions {
                size_x_px,
                size_y_px,
            });
        }

        let expected = size_x_px
            .checked_mul(size_y_px)
            .and_then(|count| usize::try_from(count).ok())
            .ok_or(MatterError::StructurePixelCountOverflow {
                size_x_px,
                size_y_px,
            })?;

        if occupied_pixels.len() != expected {
            return Err(MatterError::StructurePixelCountMismatch {
                expected,
                actual: occupied_pixels.len(),
            });
        }
        if !occupied_pixels.iter().any(|occupied| *occupied) {
            return Err(MatterError::EmptyStructure);
        }

        Ok(Self {
            size_x_px,
            size_y_px,
            occupied_pixels,
        })
    }

    /// 创建一个所有像素均被占用的矩形结构。
    pub fn filled(size_x_px: u32, size_y_px: u32) -> Result<Self, MatterError> {
        if size_x_px == 0 || size_y_px == 0 {
            return Err(MatterError::InvalidStructureDimensions {
                size_x_px,
                size_y_px,
            });
        }
        let count = size_x_px
            .checked_mul(size_y_px)
            .and_then(|count| usize::try_from(count).ok())
            .ok_or(MatterError::StructurePixelCountOverflow {
                size_x_px,
                size_y_px,
            })?;
        Self::new(size_x_px, size_y_px, vec![true; count])
    }

    pub const fn size_x_px(&self) -> u32 {
        self.size_x_px
    }

    pub const fn size_y_px(&self) -> u32 {
        self.size_y_px
    }

    pub fn occupied_pixel_count(&self) -> usize {
        self.occupied_pixels
            .iter()
            .filter(|occupied| **occupied)
            .count()
    }

    pub fn is_occupied(&self, x: u32, y: u32) -> bool {
        if x >= self.size_x_px || y >= self.size_y_px {
            return false;
        }

        let index = (y * self.size_x_px + x) as usize;
        self.occupied_pixels[index]
    }

    pub fn occupied_pixels(&self) -> &[bool] {
        &self.occupied_pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_minimum_one_pixel_structure() {
        let structure = MatterStructure::new(1, 1, vec![true]).unwrap();
        assert_eq!(structure.occupied_pixel_count(), 1);
        assert!(structure.is_occupied(0, 0));
    }

    #[test]
    fn validates_dimensions_mask_size_and_occupied_area() {
        assert!(matches!(
            MatterStructure::new(0, 1, vec![]),
            Err(MatterError::InvalidStructureDimensions { .. })
        ));
        assert!(matches!(
            MatterStructure::new(2, 2, vec![true]),
            Err(MatterError::StructurePixelCountMismatch { .. })
        ));
        assert!(matches!(
            MatterStructure::new(2, 2, vec![false; 4]),
            Err(MatterError::EmptyStructure)
        ));
    }
}
