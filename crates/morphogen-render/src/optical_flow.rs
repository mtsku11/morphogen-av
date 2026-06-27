use rayon::prelude::*;

use crate::{FlowField, ImageBufferF32, RenderError};

/// Default half-window for dense Lucas-Kanade least-squares neighborhoods.
pub const LUCAS_KANADE_WINDOW_RADIUS: i32 = 3;
/// The deterministic reference uses at most this many pyramid levels.
pub const PYRAMIDAL_LUCAS_KANADE_MAX_LEVELS: usize = 4;
/// Number of incremental warp refinements at each pyramid level.
pub const PYRAMIDAL_LUCAS_KANADE_WARP_ITERATIONS: usize = 4;

const LUCAS_KANADE_DETERMINANT_EPSILON: f32 = 1e-6;
const MIN_RELIABLE_CONFIDENCE: f32 = 0.05;
const MIN_PYRAMID_DIMENSION: u32 = 16;

/// A scalar reliability field aligned with an output flow field. Values are
/// normalized to the inclusive range `0.0..=1.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowConfidenceMap {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

impl FlowConfidenceMap {
    pub fn new(width: u32, height: u32, values: Vec<f32>) -> Result<Self, RenderError> {
        let expected = checked_pixel_count(width, height)?;
        if values.len() != expected {
            return Err(RenderError::InvalidFlowField(format!(
                "expected {expected} confidence values, got {}",
                values.len()
            )));
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(RenderError::InvalidFlowField(
                "confidence values must be finite".to_string(),
            ));
        }

        Ok(Self {
            width,
            height,
            values,
        })
    }

    pub fn value(&self, x: u32, y: u32) -> Option<f32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.values
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }
}

/// Optical-flow result for the pyramidal reference estimator. `flow` is ready
/// for renderer consumption; the two confidence maps remain available to later
/// analysis, masking, and diagnostic nodes without changing the v2 flow-cache
/// payload contract.
#[derive(Debug, Clone, PartialEq)]
pub struct PyramidalLucasKanadeEstimate {
    pub flow: FlowField,
    pub forward_confidence: FlowConfidenceMap,
    pub backward_confidence: FlowConfidenceMap,
}

/// Estimates dense temporal optical flow through a coarse-to-fine Lucas-Kanade
/// pyramid with iterative warped refinement and forward/backward consistency.
///
/// The returned field uses [`crate::FlowField`]'s backward-sampling convention:
/// adding a vector to a current output coordinate selects the corresponding
/// previous-frame coordinate. It is expressed in requested output pixels before
/// render-node amount scaling, so it can be applied directly to a carrier with
/// different dimensions than the modulator.
pub fn pyramidal_lucas_kanade_flow_cpu(
    previous: &ImageBufferF32,
    current: &ImageBufferF32,
    width: u32,
    height: u32,
    window_radius: i32,
) -> Result<PyramidalLucasKanadeEstimate, RenderError> {
    pyramidal_lucas_kanade_flow_with_refiner(
        previous,
        current,
        width,
        height,
        window_radius,
        refine_level_cpu,
    )
}

/// Signature of the per-level Lucas-Kanade refinement step: given the previous
/// and current pyramid level as flat luminance buffers (`width * height`, row
/// major), the current `flow` estimate to refine in place, the window radius and
/// the warp-iteration count, it returns the per-pixel structure confidence.
///
/// The refinement is the dense inner loop of the estimator and the only step a
/// GPU backend needs to override; expressing it as a plain-slice callback keeps
/// the pyramid build, upsample, forward/backward filter and resample steps shared
/// (and bitwise identical) across backends while the parity surface stays a
/// single kernel.
pub trait LucasKanadeLevelRefiner:
    Fn(&[f32], &[f32], u32, u32, &mut [[f32; 2]], i32, usize) -> Result<Vec<f32>, RenderError>
{
}

impl<T> LucasKanadeLevelRefiner for T where
    T: Fn(&[f32], &[f32], u32, u32, &mut [[f32; 2]], i32, usize) -> Result<Vec<f32>, RenderError>
{
}

/// Backend-generic entry point for the pyramidal Lucas-Kanade estimator. The CPU
/// reference is [`pyramidal_lucas_kanade_flow_cpu`] (this with [`refine_level_cpu`]);
/// a Metal backend passes a refiner that dispatches the level-refine kernel while
/// reusing every surrounding (cheap, exactly-parity) CPU step.
pub fn pyramidal_lucas_kanade_flow_with_refiner<R>(
    previous: &ImageBufferF32,
    current: &ImageBufferF32,
    width: u32,
    height: u32,
    window_radius: i32,
    refiner: R,
) -> Result<PyramidalLucasKanadeEstimate, RenderError>
where
    R: LucasKanadeLevelRefiner,
{
    if previous.width != current.width || previous.height != current.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "previous frame is {}x{}, current frame is {}x{}",
            previous.width, previous.height, current.width, current.height
        )));
    }

    let radius = window_radius.max(0);
    let previous_pyramid = build_luminance_pyramid(previous);
    let current_pyramid = build_luminance_pyramid(current);
    let (forward, forward_structure) = estimate_forward_flow(
        &previous_pyramid,
        &current_pyramid,
        radius,
        PYRAMIDAL_LUCAS_KANADE_WARP_ITERATIONS,
        &refiner,
    )?;
    let (backward, backward_structure) = estimate_forward_flow(
        &current_pyramid,
        &previous_pyramid,
        radius,
        PYRAMIDAL_LUCAS_KANADE_WARP_ITERATIONS,
        &refiner,
    )?;

    let source = current_pyramid.first().ok_or_else(|| {
        RenderError::InvalidFlowField("optical-flow pyramid is empty".to_string())
    })?;
    let filtered = forward_backward_filter(
        &forward,
        &backward,
        &forward_structure,
        &backward_structure,
        source,
    )?;

    let output_per_source_x = axis_scale(width, source.width);
    let output_per_source_y = axis_scale(height, source.height);
    let output_flow = FlowField::from_fn(width, height, |x, y| {
        let source_x = map_axis(x, width, source.width);
        let source_y = map_axis(y, height, source.height);
        let forward = sample_vector_clamped(
            &filtered.vectors,
            source.width,
            source.height,
            source_x,
            source_y,
        );
        [
            -forward[0] * output_per_source_x,
            -forward[1] * output_per_source_y,
        ]
    })?;

    Ok(PyramidalLucasKanadeEstimate {
        flow: output_flow,
        forward_confidence: resample_confidence(
            &filtered.forward_confidence,
            source.width,
            source.height,
            width,
            height,
        )?,
        backward_confidence: resample_confidence(
            &filtered.backward_confidence,
            source.width,
            source.height,
            width,
            height,
        )?,
    })
}

/// Compatibility entry point for callers that need a flow field only. New
/// feedback work should use [`pyramidal_lucas_kanade_flow_cpu`] so confidence
/// data remains available to the graph.
pub fn lucas_kanade_flow_cpu(
    previous: &ImageBufferF32,
    current: &ImageBufferF32,
    width: u32,
    height: u32,
    window_radius: i32,
) -> Result<FlowField, RenderError> {
    Ok(pyramidal_lucas_kanade_flow_cpu(previous, current, width, height, window_radius)?.flow)
}

#[derive(Debug, Clone)]
struct LuminanceImage {
    width: u32,
    height: u32,
    values: Vec<f32>,
}

impl LuminanceImage {
    fn from_rgba(image: &ImageBufferF32) -> Self {
        Self {
            width: image.width,
            height: image.height,
            values: image.pixels.iter().map(|pixel| luminance(*pixel)).collect(),
        }
    }

    fn sample(&self, x: f32, y: f32) -> f32 {
        sample_scalar_clamped(&self.values, self.width, self.height, x, y)
    }
}

fn build_luminance_pyramid(image: &ImageBufferF32) -> Vec<LuminanceImage> {
    let mut pyramid = vec![LuminanceImage::from_rgba(image)];
    while pyramid.len() < PYRAMIDAL_LUCAS_KANADE_MAX_LEVELS {
        let Some(previous) = pyramid.last() else {
            break;
        };
        if previous.width <= MIN_PYRAMID_DIMENSION && previous.height <= MIN_PYRAMID_DIMENSION {
            break;
        }

        let width = previous.width.div_ceil(2);
        let height = previous.height.div_ceil(2);
        let mut values = Vec::with_capacity(width as usize * height as usize);
        for y in 0..height {
            for x in 0..width {
                let source_x = x as f32 * 2.0;
                let source_y = y as f32 * 2.0;
                let average = (previous.sample(source_x, source_y)
                    + previous.sample(source_x + 1.0, source_y)
                    + previous.sample(source_x, source_y + 1.0)
                    + previous.sample(source_x + 1.0, source_y + 1.0))
                    * 0.25;
                values.push(average);
            }
        }
        pyramid.push(LuminanceImage {
            width,
            height,
            values,
        });
    }
    pyramid
}

fn estimate_forward_flow<R>(
    previous_pyramid: &[LuminanceImage],
    current_pyramid: &[LuminanceImage],
    radius: i32,
    iterations: usize,
    refiner: &R,
) -> Result<(Vec<[f32; 2]>, Vec<f32>), RenderError>
where
    R: LucasKanadeLevelRefiner,
{
    if previous_pyramid.len() != current_pyramid.len() || previous_pyramid.is_empty() {
        return Err(RenderError::IncompatibleInputs(
            "optical-flow pyramids must have matching nonzero levels".to_string(),
        ));
    }

    let mut flow = Vec::new();
    let mut confidence = Vec::new();
    for level_index in (0..previous_pyramid.len()).rev() {
        let previous = &previous_pyramid[level_index];
        let current = &current_pyramid[level_index];
        if previous.width != current.width || previous.height != current.height {
            return Err(RenderError::IncompatibleInputs(
                "optical-flow pyramid levels have incompatible dimensions".to_string(),
            ));
        }

        flow = if flow.is_empty() {
            vec![[0.0, 0.0]; checked_pixel_count(current.width, current.height)?]
        } else {
            upsample_flow(
                &flow,
                previous_pyramid[level_index + 1].width,
                previous_pyramid[level_index + 1].height,
                current.width,
                current.height,
            )
        };
        confidence = refiner(
            &previous.values,
            &current.values,
            current.width,
            current.height,
            &mut flow,
            radius,
            iterations,
        )?;
    }

    Ok((flow, confidence))
}

/// CPU reference implementation of the per-level Lucas-Kanade refinement. See
/// [`LucasKanadeLevelRefiner`] for the contract; this is the refiner used by
/// [`pyramidal_lucas_kanade_flow_cpu`] and the ground truth a GPU port is gated
/// against.
pub fn refine_level_cpu(
    previous: &[f32],
    current: &[f32],
    level_width: u32,
    level_height: u32,
    flow: &mut [[f32; 2]],
    radius: i32,
    iterations: usize,
) -> Result<Vec<f32>, RenderError> {
    let expected = checked_pixel_count(level_width, level_height)?;
    if flow.len() != expected || previous.len() != expected || current.len() != expected {
        return Err(RenderError::InvalidFlowField(
            "pyramid flow level has incompatible dimensions".to_string(),
        ));
    }

    let level_width_u = level_width;
    let level_height_u = level_height;
    let width = level_width as usize;
    let mut confidence = vec![0.0; expected];
    // Each pixel's update reads only its own flow estimate plus the (immutable)
    // images, so the per-pixel work is independent within an iteration. Running
    // it in parallel is bitwise-identical to the sequential pass.
    for _ in 0..iterations.max(1) {
        flow.par_iter_mut()
            .zip(confidence.par_iter_mut())
            .enumerate()
            .for_each(|(index, (flow_vector, confidence_value))| {
                let x = (index % width) as f32;
                let y = (index / width) as f32;
                let estimate = *flow_vector;
                let mut sxx = 0.0_f32;
                let mut sxy = 0.0_f32;
                let mut syy = 0.0_f32;
                let mut sxt = 0.0_f32;
                let mut syt = 0.0_f32;

                for window_y in -radius..=radius {
                    for window_x in -radius..=radius {
                        let current_x = x + window_x as f32;
                        let current_y = y + window_y as f32;
                        let previous_x = current_x - estimate[0];
                        let previous_y = current_y - estimate[1];
                        let ix = 0.5
                            * (sample_scalar_clamped(
                                previous,
                                level_width_u,
                                level_height_u,
                                previous_x + 1.0,
                                previous_y,
                            ) - sample_scalar_clamped(
                                previous,
                                level_width_u,
                                level_height_u,
                                previous_x - 1.0,
                                previous_y,
                            ));
                        let iy = 0.5
                            * (sample_scalar_clamped(
                                previous,
                                level_width_u,
                                level_height_u,
                                previous_x,
                                previous_y + 1.0,
                            ) - sample_scalar_clamped(
                                previous,
                                level_width_u,
                                level_height_u,
                                previous_x,
                                previous_y - 1.0,
                            ));
                        let it = sample_scalar_clamped(
                            current,
                            level_width_u,
                            level_height_u,
                            current_x,
                            current_y,
                        ) - sample_scalar_clamped(
                            previous,
                            level_width_u,
                            level_height_u,
                            previous_x,
                            previous_y,
                        );

                        sxx += ix * ix;
                        sxy += ix * iy;
                        syy += iy * iy;
                        sxt += ix * it;
                        syt += iy * it;
                    }
                }

                let determinant = sxx * syy - sxy * sxy;
                *confidence_value = structure_confidence(sxx, sxy, syy, determinant);
                if determinant.abs() <= LUCAS_KANADE_DETERMINANT_EPSILON {
                    return;
                }

                let delta_x = (-syy * sxt + sxy * syt) / determinant;
                let delta_y = (sxy * sxt - sxx * syt) / determinant;
                if delta_x.is_finite() && delta_y.is_finite() {
                    flow_vector[0] += delta_x;
                    flow_vector[1] += delta_y;
                }
            });
    }
    Ok(confidence)
}

struct FilteredFlow {
    vectors: Vec<[f32; 2]>,
    forward_confidence: Vec<f32>,
    backward_confidence: Vec<f32>,
}

fn forward_backward_filter(
    forward: &[[f32; 2]],
    backward: &[[f32; 2]],
    forward_structure: &[f32],
    backward_structure: &[f32],
    image: &LuminanceImage,
) -> Result<FilteredFlow, RenderError> {
    let expected = checked_pixel_count(image.width, image.height)?;
    if forward.len() != expected
        || backward.len() != expected
        || forward_structure.len() != expected
        || backward_structure.len() != expected
    {
        return Err(RenderError::InvalidFlowField(
            "forward/backward flow confidence dimensions do not match".to_string(),
        ));
    }

    let mut filtered = Vec::with_capacity(expected);
    let mut forward_confidence = Vec::with_capacity(expected);
    let mut backward_confidence = Vec::with_capacity(expected);
    for y in 0..image.height {
        for x in 0..image.width {
            let index = y as usize * image.width as usize + x as usize;
            let vector = forward[index];
            let previous_x = x as f32 - vector[0];
            let previous_y = y as f32 - vector[1];
            let reverse =
                sample_vector_clamped(backward, image.width, image.height, previous_x, previous_y);
            let reverse_structure = sample_scalar_clamped(
                backward_structure,
                image.width,
                image.height,
                previous_x,
                previous_y,
            );
            let disagreement =
                ((vector[0] + reverse[0]).powi(2) + (vector[1] + reverse[1]).powi(2)).sqrt();
            let consistency = 1.0 / (1.0 + disagreement);
            let forward_value = (forward_structure[index] * consistency).clamp(0.0, 1.0);
            let backward_value = (reverse_structure * consistency).clamp(0.0, 1.0);
            if forward_value.min(backward_value) >= MIN_RELIABLE_CONFIDENCE {
                filtered.push(vector);
            } else {
                filtered.push([0.0, 0.0]);
            }
            forward_confidence.push(forward_value);
            backward_confidence.push(backward_value);
        }
    }

    Ok(FilteredFlow {
        vectors: filtered,
        forward_confidence,
        backward_confidence,
    })
}

fn structure_confidence(sxx: f32, sxy: f32, syy: f32, determinant: f32) -> f32 {
    if determinant <= LUCAS_KANADE_DETERMINANT_EPSILON {
        return 0.0;
    }
    let trace = sxx + syy;
    let discriminant = ((sxx - syy).powi(2) + 4.0 * sxy * sxy).sqrt();
    let minimum = ((trace - discriminant) * 0.5).max(0.0);
    let maximum = ((trace + discriminant) * 0.5).max(LUCAS_KANADE_DETERMINANT_EPSILON);
    (minimum / maximum).clamp(0.0, 1.0)
}

fn upsample_flow(
    source: &[[f32; 2]],
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> Vec<[f32; 2]> {
    let scale_x = axis_scale(width, source_width);
    let scale_y = axis_scale(height, source_height);
    let mut result = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            let source_x = map_axis(x, width, source_width);
            let source_y = map_axis(y, height, source_height);
            let vector =
                sample_vector_clamped(source, source_width, source_height, source_x, source_y);
            result.push([vector[0] * scale_x, vector[1] * scale_y]);
        }
    }
    result
}

fn resample_confidence(
    source: &[f32],
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> Result<FlowConfidenceMap, RenderError> {
    let mut values = Vec::with_capacity(checked_pixel_count(width, height)?);
    for y in 0..height {
        for x in 0..width {
            values.push(sample_scalar_clamped(
                source,
                source_width,
                source_height,
                map_axis(x, width, source_width),
                map_axis(y, height, source_height),
            ));
        }
    }
    FlowConfidenceMap::new(width, height, values)
}

fn sample_vector_clamped(
    vectors: &[[f32; 2]],
    width: u32,
    height: u32,
    x: f32,
    y: f32,
) -> [f32; 2] {
    let horizontal = sample_scalar_components(vectors, width, height, x, y, 0);
    let vertical = sample_scalar_components(vectors, width, height, x, y, 1);
    [horizontal, vertical]
}

fn sample_scalar_components(
    vectors: &[[f32; 2]],
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    component: usize,
) -> f32 {
    sample_bilinear(width, height, x, y, |index| {
        vectors.get(index).map_or(0.0, |vector| vector[component])
    })
}

fn sample_scalar_clamped(values: &[f32], width: u32, height: u32, x: f32, y: f32) -> f32 {
    sample_bilinear(width, height, x, y, |index| {
        values.get(index).map_or(0.0, |value| *value)
    })
}

fn sample_bilinear(
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    value_at: impl Fn(usize) -> f32,
) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let clamped_x = x.clamp(0.0, (width - 1) as f32);
    let clamped_y = y.clamp(0.0, (height - 1) as f32);
    let x0 = clamped_x.floor() as u32;
    let y0 = clamped_y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = clamped_x - x0 as f32;
    let ty = clamped_y - y0 as f32;
    let c00 = value_at(y0 as usize * width as usize + x0 as usize);
    let c10 = value_at(y0 as usize * width as usize + x1 as usize);
    let c01 = value_at(y1 as usize * width as usize + x0 as usize);
    let c11 = value_at(y1 as usize * width as usize + x1 as usize);
    let top = c00 + (c10 - c00) * tx;
    let bottom = c01 + (c11 - c01) * tx;
    top + (bottom - top) * ty
}

fn map_axis(value: u32, target_extent: u32, source_extent: u32) -> f32 {
    if target_extent <= 1 || source_extent <= 1 {
        return 0.0;
    }
    value as f32 / (target_extent - 1) as f32 * (source_extent - 1) as f32
}

fn axis_scale(target_extent: u32, source_extent: u32) -> f32 {
    if target_extent <= 1 || source_extent <= 1 {
        return 0.0;
    }
    (target_extent - 1) as f32 / (source_extent - 1) as f32
}

fn checked_pixel_count(width: u32, height: u32) -> Result<usize, RenderError> {
    if width == 0 || height == 0 {
        return Err(RenderError::InvalidFlowField(
            "flow dimensions must be greater than zero".to_string(),
        ));
    }
    (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| RenderError::InvalidFlowField("flow dimensions are too large".to_string()))
}

fn luminance(pixel: [f32; 4]) -> f32 {
    pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722
}

#[cfg(test)]
mod tests {
    use super::*;

    fn textured_frame(width: u32, height: u32, shift_x: f32, shift_y: f32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |x, y| {
            let fx = x as f32 - shift_x;
            let fy = y as f32 - shift_y;
            let value = 0.5
                + 0.2 * (0.31 * fx).sin()
                + 0.2 * (0.37 * fy).sin()
                + 0.1 * (0.23 * (fx + fy)).sin();
            [value, value, value, 1.0]
        })
        .expect("valid frame")
    }

    #[test]
    fn static_frames_produce_near_zero_flow() {
        let frame = textured_frame(32, 32, 0.0, 0.0);
        let estimate =
            pyramidal_lucas_kanade_flow_cpu(&frame, &frame, 32, 32, LUCAS_KANADE_WINDOW_RADIUS)
                .expect("flow");

        let vector = estimate.flow.vector(16, 16).expect("vector");
        assert!(vector[0].abs() < 1e-4, "u was {}", vector[0]);
        assert!(vector[1].abs() < 1e-4, "v was {}", vector[1]);
    }

    #[test]
    fn pyramid_recovers_large_translation_in_backward_sampling_coordinates() {
        let previous = textured_frame(64, 48, 0.0, 0.0);
        let current = textured_frame(64, 48, 6.0, 4.0);
        let estimate = pyramidal_lucas_kanade_flow_cpu(
            &previous,
            &current,
            64,
            48,
            LUCAS_KANADE_WINDOW_RADIUS,
        )
        .expect("flow");

        let vector = estimate.flow.vector(32, 24).expect("vector");
        assert!(vector[0] < -4.5 && vector[0] > -7.5, "u was {}", vector[0]);
        assert!(vector[1] < -2.5 && vector[1] > -5.5, "v was {}", vector[1]);
        assert!(
            estimate
                .forward_confidence
                .value(32, 24)
                .expect("forward confidence")
                > 0.05
        );
        assert!(
            estimate
                .backward_confidence
                .value(32, 24)
                .expect("backward confidence")
                > 0.05
        );
    }

    #[test]
    fn flow_is_scaled_to_the_output_coordinate_system() {
        let previous = textured_frame(32, 32, 0.0, 0.0);
        let current = textured_frame(32, 32, 2.0, 0.0);
        let estimate = pyramidal_lucas_kanade_flow_cpu(
            &previous,
            &current,
            63,
            32,
            LUCAS_KANADE_WINDOW_RADIUS,
        )
        .expect("flow");

        let vector = estimate.flow.vector(31, 16).expect("vector");
        assert!(vector[0] < -2.6 && vector[0] > -5.4, "u was {}", vector[0]);
        assert!(vector[1].abs() < 0.8, "v was {}", vector[1]);
    }

    #[test]
    fn flat_regions_have_zero_confidence_and_zero_flow() {
        let frame =
            ImageBufferF32::from_fn(24, 24, |_, _| [0.5, 0.5, 0.5, 1.0]).expect("flat frame");
        let estimate =
            pyramidal_lucas_kanade_flow_cpu(&frame, &frame, 24, 24, LUCAS_KANADE_WINDOW_RADIUS)
                .expect("flow");

        assert_eq!(estimate.flow.vector(12, 12), Some([0.0, 0.0]));
        assert_eq!(estimate.forward_confidence.value(12, 12), Some(0.0));
        assert_eq!(estimate.backward_confidence.value(12, 12), Some(0.0));
    }

    #[test]
    fn mismatched_dimensions_are_rejected() {
        let previous = textured_frame(8, 8, 0.0, 0.0);
        let current = textured_frame(8, 4, 0.0, 0.0);
        assert!(pyramidal_lucas_kanade_flow_cpu(&previous, &current, 8, 8, 2).is_err());
    }
}
