use crate::{sample_bilinear_clamped, FlowField, ImageBufferF32, RenderError};

pub fn luminance_gradient_flow_cpu(
    modulator: &ImageBufferF32,
    width: u32,
    height: u32,
) -> Result<FlowField, RenderError> {
    FlowField::from_fn(width, height, |x, y| {
        let source_x = map_axis(x, width, modulator.width);
        let source_y = map_axis(y, height, modulator.height);

        let left = luminance(sample_bilinear_clamped(modulator, source_x - 1.0, source_y));
        let right = luminance(sample_bilinear_clamped(modulator, source_x + 1.0, source_y));
        let up = luminance(sample_bilinear_clamped(modulator, source_x, source_y - 1.0));
        let down = luminance(sample_bilinear_clamped(modulator, source_x, source_y + 1.0));

        [right - left, down - up]
    })
}

fn map_axis(value: u32, target_extent: u32, source_extent: u32) -> f32 {
    if target_extent <= 1 || source_extent <= 1 {
        return 0.0;
    }

    value as f32 / (target_extent - 1) as f32 * (source_extent - 1) as f32
}

fn luminance(pixel: [f32; 4]) -> f32 {
    pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luminance_gradient_flow_points_toward_brighter_pixels() {
        let modulator = ImageBufferF32::new(
            3,
            1,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ],
        )
        .expect("valid modulator");

        let flow = luminance_gradient_flow_cpu(&modulator, 3, 1).expect("flow");

        assert!(flow.vector(0, 0).expect("vector")[0] > 0.99);
        assert!(flow.vector(2, 0).expect("vector")[0].abs() < 0.000_001);
    }

    #[test]
    fn luminance_gradient_flow_resizes_to_carrier_dimensions() {
        let modulator = ImageBufferF32::new(
            2,
            2,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ],
        )
        .expect("valid modulator");

        let flow = luminance_gradient_flow_cpu(&modulator, 4, 4).expect("flow");

        assert_eq!(flow.width, 4);
        assert_eq!(flow.height, 4);
        assert!(flow.vector(0, 0).expect("vector")[0] > 0.99);
    }
}
