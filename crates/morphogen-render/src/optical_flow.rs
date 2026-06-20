use crate::{sample_bilinear_clamped, FlowField, ImageBufferF32, RenderError};

/// Default half-window for the dense Lucas-Kanade least-squares neighborhood.
pub const LUCAS_KANADE_WINDOW_RADIUS: i32 = 3;

/// Structure-tensor determinants below this threshold are treated as
/// unsolvable (flat regions and the aperture problem) and yield zero flow.
const LUCAS_KANADE_DETERMINANT_EPSILON: f32 = 1e-6;

/// Estimates dense temporal optical flow between two consecutive modulator
/// frames with windowed Lucas-Kanade least squares.
///
/// The returned field is in source-pixel motion units (before amount scaling)
/// and is resampled to the requested output dimensions, mirroring the
/// resampling convention of [`crate::luminance_gradient_flow_cpu`]. Vectors
/// describe the apparent motion that carries `previous` brightness onto
/// `current`.
pub fn lucas_kanade_flow_cpu(
    previous: &ImageBufferF32,
    current: &ImageBufferF32,
    width: u32,
    height: u32,
    window_radius: i32,
) -> Result<FlowField, RenderError> {
    if previous.width != current.width || previous.height != current.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "previous frame is {}x{}, current frame is {}x{}",
            previous.width, previous.height, current.width, current.height
        )));
    }
    let radius = window_radius.max(0);

    FlowField::from_fn(width, height, |x, y| {
        let source_x = map_axis(x, width, current.width);
        let source_y = map_axis(y, height, current.height);

        let mut sxx = 0.0_f32;
        let mut sxy = 0.0_f32;
        let mut syy = 0.0_f32;
        let mut sxt = 0.0_f32;
        let mut syt = 0.0_f32;

        for window_y in -radius..=radius {
            for window_x in -radius..=radius {
                let px = source_x + window_x as f32;
                let py = source_y + window_y as f32;

                let left = luminance(sample_bilinear_clamped(current, px - 1.0, py));
                let right = luminance(sample_bilinear_clamped(current, px + 1.0, py));
                let up = luminance(sample_bilinear_clamped(current, px, py - 1.0));
                let down = luminance(sample_bilinear_clamped(current, px, py + 1.0));
                let ix = 0.5 * (right - left);
                let iy = 0.5 * (down - up);
                let it = luminance(sample_bilinear_clamped(current, px, py))
                    - luminance(sample_bilinear_clamped(previous, px, py));

                sxx += ix * ix;
                sxy += ix * iy;
                syy += iy * iy;
                sxt += ix * it;
                syt += iy * it;
            }
        }

        let determinant = sxx * syy - sxy * sxy;
        if determinant.abs() <= LUCAS_KANADE_DETERMINANT_EPSILON {
            return [0.0, 0.0];
        }

        // Solve A [u; v] = -[sxt; syt] with A = [[sxx, sxy], [sxy, syy]].
        let u = (-syy * sxt + sxy * syt) / determinant;
        let v = (sxy * sxt - sxx * syt) / determinant;
        [u, v]
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

    fn textured_frame(width: u32, height: u32, shift_x: f32, shift_y: f32) -> ImageBufferF32 {
        // A 2D sinusoidal texture has gradient that varies in direction across
        // any window, so the structure tensor is full rank and the flow is
        // recoverable. Shifting the phase translates the content.
        ImageBufferF32::from_fn(width, height, |x, y| {
            let fx = x as f32 - shift_x;
            let fy = y as f32 - shift_y;
            let value =
                0.5 + 0.2 * (0.6 * fx).sin() + 0.2 * (0.7 * fy).sin() + 0.1 * (0.5 * (fx + fy)).sin();
            [value, value, value, 1.0]
        })
        .expect("valid frame")
    }

    #[test]
    fn static_frames_produce_near_zero_flow() {
        let frame = textured_frame(16, 16, 0.0, 0.0);
        let flow = lucas_kanade_flow_cpu(&frame, &frame, 16, 16, LUCAS_KANADE_WINDOW_RADIUS)
            .expect("flow");

        let vector = flow.vector(8, 8).expect("vector");
        assert!(vector[0].abs() < 1e-4, "u was {}", vector[0]);
        assert!(vector[1].abs() < 1e-4, "v was {}", vector[1]);
    }

    #[test]
    fn horizontal_translation_is_recovered() {
        let previous = textured_frame(20, 20, 0.0, 0.0);
        let current = textured_frame(20, 20, 1.0, 0.0);
        let flow =
            lucas_kanade_flow_cpu(&previous, &current, 20, 20, LUCAS_KANADE_WINDOW_RADIUS)
                .expect("flow");

        let vector = flow.vector(10, 10).expect("vector");
        assert!(vector[0] > 0.6 && vector[0] < 1.4, "u was {}", vector[0]);
        assert!(vector[1].abs() < 0.3, "v was {}", vector[1]);
    }

    #[test]
    fn mismatched_dimensions_are_rejected() {
        let previous = textured_frame(8, 8, 0.0, 0.0);
        let current = textured_frame(8, 4, 0.0, 0.0);
        assert!(lucas_kanade_flow_cpu(&previous, &current, 8, 8, 2).is_err());
    }
}
