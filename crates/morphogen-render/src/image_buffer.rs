use serde::{Deserialize, Serialize};

use crate::RenderError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageBufferF32 {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
}

impl ImageBufferF32 {
    pub fn new(width: u32, height: u32, pixels: Vec<[f32; 4]>) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidImageBuffer(
                "width and height must be greater than zero".to_string(),
            ));
        }

        let expected = pixel_count(width, height)?;
        if pixels.len() != expected {
            return Err(RenderError::InvalidImageBuffer(format!(
                "expected {expected} pixels, got {}",
                pixels.len()
            )));
        }

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn from_fn(
        width: u32,
        height: u32,
        mut pixel_fn: impl FnMut(u32, u32) -> [f32; 4],
    ) -> Result<Self, RenderError> {
        let count = pixel_count(width, height)?;
        let mut pixels = Vec::with_capacity(count);
        for y in 0..height {
            for x in 0..width {
                pixels.push(pixel_fn(x, y));
            }
        }
        Self::new(width, height, pixels)
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<[f32; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.pixels.get(index).copied()
    }

    /// Largest absolute per-channel difference against another buffer of the same
    /// dimensions. Returns `None` when the two buffers are not directly comparable.
    /// Used to gate the Metal render backend against the CPU reference.
    pub fn max_channel_difference(&self, other: &Self) -> Option<f32> {
        if self.width != other.width || self.height != other.height {
            return None;
        }

        let mut max_difference = 0.0_f32;
        for (lhs, rhs) in self.pixels.iter().zip(&other.pixels) {
            for channel in 0..4 {
                let difference = (lhs[channel] - rhs[channel]).abs();
                if difference > max_difference {
                    max_difference = difference;
                }
            }
        }
        Some(max_difference)
    }
}

fn pixel_count(width: u32, height: u32) -> Result<usize, RenderError> {
    if width == 0 || height == 0 {
        return Err(RenderError::InvalidImageBuffer(
            "width and height must be greater than zero".to_string(),
        ));
    }

    (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| {
            RenderError::InvalidImageBuffer("image dimensions are too large".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_channel_difference_reports_largest_channel_delta() {
        let a = ImageBufferF32::new(1, 2, vec![[0.0, 0.0, 0.0, 1.0], [0.25, 0.5, 0.0, 1.0]])
            .expect("buffer a");
        let b = ImageBufferF32::new(1, 2, vec![[0.0, 0.0, 0.0, 1.0], [0.25, 0.9, 0.0, 1.0]])
            .expect("buffer b");

        let difference = a.max_channel_difference(&b).expect("comparable buffers");
        assert!((difference - 0.4).abs() < 1e-6);
    }

    #[test]
    fn max_channel_difference_rejects_mismatched_dimensions() {
        let a = ImageBufferF32::new(1, 1, vec![[0.0, 0.0, 0.0, 1.0]]).expect("buffer a");
        let b = ImageBufferF32::new(2, 1, vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]])
            .expect("buffer b");

        assert_eq!(a.max_channel_difference(&b), None);
    }
}
