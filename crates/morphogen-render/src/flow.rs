use serde::{Deserialize, Serialize};

use crate::RenderError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowField {
    pub width: u32,
    pub height: u32,
    pub vectors: Vec<[f32; 2]>,
}

impl FlowField {
    pub fn new(width: u32, height: u32, vectors: Vec<[f32; 2]>) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidFlowField(
                "width and height must be greater than zero".to_string(),
            ));
        }

        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                RenderError::InvalidFlowField("flow dimensions are too large".to_string())
            })?;

        if vectors.len() != expected {
            return Err(RenderError::InvalidFlowField(format!(
                "expected {expected} vectors, got {}",
                vectors.len()
            )));
        }

        Ok(Self {
            width,
            height,
            vectors,
        })
    }

    pub fn from_fn(
        width: u32,
        height: u32,
        mut vector_fn: impl FnMut(u32, u32) -> [f32; 2],
    ) -> Result<Self, RenderError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                RenderError::InvalidFlowField("flow dimensions are too large".to_string())
            })?;
        let mut vectors = Vec::with_capacity(expected);
        for y in 0..height {
            for x in 0..width {
                vectors.push(vector_fn(x, y));
            }
        }
        Self::new(width, height, vectors)
    }

    pub fn vector(&self, x: u32, y: u32) -> Option<[f32; 2]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = y as usize * self.width as usize + x as usize;
        self.vectors.get(index).copied()
    }
}
