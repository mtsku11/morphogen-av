use serde::{Deserialize, Serialize};

use crate::{sample_bilinear_clamped, FlowField, ImageBufferF32, RenderError};

/// Selects how the carrier's high-frequency detail is re-injected when
/// `structure_mix != 0`.
///
/// The serde default is [`StructureMode::SingleScale`] so settings serialized
/// before multiscale existed keep their exact meaning, and the Metal parity
/// path (which only implements single-scale) is unaffected.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StructureMode {
    /// One full-resolution high-pass band (carrier minus a radius-2 binomial
    /// low-pass), re-injected uniformly. This is the original behavior and the
    /// only mode with a Metal parity implementation.
    #[default]
    SingleScale,
    /// Three full-resolution detail bands, each gated by a structure mask
    /// derived from the morphed (advected) frame so re-seeded detail follows
    /// the evolving geometry rather than the static carrier grid. CPU-only.
    Multiscale,
}

/// Lower bound on the structure mask so flat morphed regions still regenerate
/// some detail instead of washing fully to fog.
const STRUCTURE_MASK_FLOOR: f32 = 0.25;
/// Maps morphed-frame gradient magnitude into the `[0, 1]` mask response before
/// the floor is applied.
const STRUCTURE_MASK_GAIN: f32 = 6.0;

/// Parameters for the first temporal feedback contract. The MVP deliberately
/// supports one history-advection pass; later multi-iteration behavior must be
/// introduced with explicit CPU and Metal semantics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct FlowFeedbackSettings {
    pub carrier_amount: f32,
    pub feedback_amount: f32,
    pub feedback_mix: f32,
    pub decay: f32,
    pub iterations: u32,
    /// Structure-preserving morph: amount of the carrier's high-frequency band
    /// (detail/edges) re-injected into the feedback result every frame. The
    /// accumulated optical-flow displacement owns the layout while this keeps
    /// detail regenerating, so the frame goes beyond recognition without
    /// collapsing to flat fog. `0.0` reproduces the original additive-feedback
    /// behavior exactly; it defaults to zero for legacy queue files.
    #[serde(default)]
    pub structure_mix: f32,
    /// How the re-injected detail is decomposed and masked. Defaults to
    /// [`StructureMode::SingleScale`] (the original, Metal-backed behavior);
    /// [`StructureMode::Multiscale`] is the CPU-only high-quality path.
    #[serde(default)]
    pub structure_mode: StructureMode,
}

impl FlowFeedbackSettings {
    pub fn validate(self) -> Result<(), RenderError> {
        for (name, value) in [
            ("carrier_amount", self.carrier_amount),
            ("feedback_amount", self.feedback_amount),
            ("feedback_mix", self.feedback_mix),
            ("decay", self.decay),
            ("structure_mix", self.structure_mix),
        ] {
            if !value.is_finite() {
                return Err(RenderError::InvalidFlowFeedbackSettings(format!(
                    "{name} must be finite"
                )));
            }
        }
        if !(0.0..=1.0).contains(&self.feedback_mix) {
            return Err(RenderError::InvalidFlowFeedbackSettings(
                "feedback_mix must be between zero and one".to_string(),
            ));
        }
        if self.decay < 0.0 {
            return Err(RenderError::InvalidFlowFeedbackSettings(
                "decay must be greater than or equal to zero".to_string(),
            ));
        }
        if self.structure_mix < 0.0 {
            return Err(RenderError::InvalidFlowFeedbackSettings(
                "structure_mix must be greater than or equal to zero".to_string(),
            ));
        }
        if self.iterations != 1 {
            return Err(RenderError::InvalidFlowFeedbackSettings(
                "the first flow-feedback renderer supports exactly one iteration".to_string(),
            ));
        }
        Ok(())
    }
}

pub fn flow_displace_cpu(
    carrier: &ImageBufferF32,
    flow: &FlowField,
    amount: f32,
) -> Result<ImageBufferF32, RenderError> {
    if carrier.width != flow.width || carrier.height != flow.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "carrier is {}x{}, flow is {}x{}",
            carrier.width, carrier.height, flow.width, flow.height
        )));
    }

    ImageBufferF32::from_fn(carrier.width, carrier.height, |x, y| {
        let vector = flow.vector(x, y).unwrap_or([0.0, 0.0]);
        let sample_x = x as f32 + vector[0] * amount;
        let sample_y = y as f32 + vector[1] * amount;
        sample_bilinear_clamped(carrier, sample_x, sample_y)
    })
}

/// Renders one frame of temporal flow feedback. Frame zero is represented by
/// `previous_output: None` and therefore contains only the displaced carrier.
pub fn flow_feedback_frame_cpu(
    carrier: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    flow: &FlowField,
    settings: FlowFeedbackSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let displaced_carrier = flow_displace_cpu(carrier, flow, settings.carrier_amount)?;

    let Some(previous_output) = previous_output else {
        return Ok(displaced_carrier);
    };

    if previous_output.width != carrier.width || previous_output.height != carrier.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "previous output is {}x{}, carrier is {}x{}",
            previous_output.width, previous_output.height, carrier.width, carrier.height
        )));
    }

    let advected_history = flow_displace_cpu(previous_output, flow, settings.feedback_amount)?;

    // Structure-preserving morph: re-inject the carrier's high-frequency detail
    // on top of the additive feedback result so detail keeps regenerating
    // instead of washing to fog at high feedback_mix. The re-injection field has
    // near-zero mean, so it restores edges/grain without re-asserting the
    // carrier's flat composition or pulling the frame back to its layout.
    // `structure_mix == 0.0` skips it entirely and is bitwise-identical to the
    // original additive-feedback output.
    let reinjection = if settings.structure_mix != 0.0 {
        Some(match settings.structure_mode {
            StructureMode::SingleScale => single_scale_structure_field(&displaced_carrier)?,
            StructureMode::Multiscale => {
                multiscale_structure_field(&displaced_carrier, &advected_history)?
            }
        })
    } else {
        None
    };

    ImageBufferF32::from_fn(carrier.width, carrier.height, |x, y| {
        let carrier_pixel = displaced_carrier.pixel(x, y).unwrap_or([0.0; 4]);
        let history_pixel = advected_history.pixel(x, y).unwrap_or([0.0; 4]);
        let base = mix_rgba(
            carrier_pixel,
            scale_rgba(history_pixel, settings.decay),
            settings.feedback_mix,
        );
        match &reinjection {
            None => base,
            Some(field) => {
                let detail = field.pixel(x, y).unwrap_or([0.0; 4]);
                [
                    base[0] + settings.structure_mix * detail[0],
                    base[1] + settings.structure_mix * detail[1],
                    base[2] + settings.structure_mix * detail[2],
                    base[3] + settings.structure_mix * detail[3],
                ]
            }
        }
    })
}

/// Single-scale re-injection field: the carrier's high-frequency band, defined
/// as the displaced carrier minus its radius-2 binomial low-pass. Computing the
/// difference once here is bitwise-identical to the original inline form.
fn single_scale_structure_field(
    displaced_carrier: &ImageBufferF32,
) -> Result<ImageBufferF32, RenderError> {
    let low_pass = low_pass_blur(displaced_carrier)?;
    ImageBufferF32::from_fn(displaced_carrier.width, displaced_carrier.height, |x, y| {
        let carrier_pixel = displaced_carrier.pixel(x, y).unwrap_or([0.0; 4]);
        let low_pixel = low_pass.pixel(x, y).unwrap_or([0.0; 4]);
        [
            carrier_pixel[0] - low_pixel[0],
            carrier_pixel[1] - low_pixel[1],
            carrier_pixel[2] - low_pixel[2],
            carrier_pixel[3] - low_pixel[3],
        ]
    })
}

/// Multiscale re-injection field: three full-resolution detail bands of the
/// displaced carrier, each gated by a structure mask taken from the morphed
/// (advected) frame. The bands are a non-decimated Burt-Adelson stack —
/// repeated binomial blurs, differenced into finest/mid/coarse bands — so detail
/// stays at carrier resolution with no upsampling artifacts. Each band is
/// modulated by a mask at the matching scale (sharp for fine detail, blurred for
/// coarse), so re-seeded detail concentrates along the evolving geometry rather
/// than on the static carrier grid.
fn multiscale_structure_field(
    displaced_carrier: &ImageBufferF32,
    advected_history: &ImageBufferF32,
) -> Result<ImageBufferF32, RenderError> {
    let blur0 = low_pass_blur(displaced_carrier)?;
    let blur1 = low_pass_blur(&blur0)?;
    let blur2 = low_pass_blur(&blur1)?;

    // One mask per band scale, derived from the morphed geometry.
    let mask_fine = structure_edge_mask(advected_history)?;
    let mask_mid = low_pass_blur(&mask_fine)?;
    let mask_coarse = low_pass_blur(&mask_mid)?;

    ImageBufferF32::from_fn(displaced_carrier.width, displaced_carrier.height, |x, y| {
        let carrier_pixel = displaced_carrier.pixel(x, y).unwrap_or([0.0; 4]);
        let blur0_pixel = blur0.pixel(x, y).unwrap_or([0.0; 4]);
        let blur1_pixel = blur1.pixel(x, y).unwrap_or([0.0; 4]);
        let blur2_pixel = blur2.pixel(x, y).unwrap_or([0.0; 4]);
        let fine = mask_fine.pixel(x, y).unwrap_or([0.0; 4]);
        let mid = mask_mid.pixel(x, y).unwrap_or([0.0; 4]);
        let coarse = mask_coarse.pixel(x, y).unwrap_or([0.0; 4]);

        let mut detail = [0.0f32; 4];
        for channel in 0..4 {
            let band_fine = carrier_pixel[channel] - blur0_pixel[channel];
            let band_mid = blur0_pixel[channel] - blur1_pixel[channel];
            let band_coarse = blur1_pixel[channel] - blur2_pixel[channel];
            detail[channel] =
                band_fine * fine[channel] + band_mid * mid[channel] + band_coarse * coarse[channel];
        }
        detail
    })
}

/// Per-pixel structure mask in `[STRUCTURE_MASK_FLOOR, 1]`, broadcast to RGBA.
/// The response follows the luminance gradient magnitude of the morphed frame,
/// lifted by a floor so flat regions keep regenerating some detail, then
/// smoothed by one binomial blur to suppress single-pixel speckle.
fn structure_edge_mask(image: &ImageBufferF32) -> Result<ImageBufferF32, RenderError> {
    let width = image.width as i32;
    let height = image.height as i32;
    let luminance = |x: i32, y: i32| {
        let px = x.clamp(0, width - 1) as u32;
        let py = y.clamp(0, height - 1) as u32;
        let pixel = image.pixel(px, py).unwrap_or([0.0; 4]);
        0.299 * pixel[0] + 0.587 * pixel[1] + 0.114 * pixel[2]
    };

    let raw = ImageBufferF32::from_fn(image.width, image.height, |x, y| {
        let xi = x as i32;
        let yi = y as i32;
        let gradient_x = luminance(xi + 1, yi) - luminance(xi - 1, yi);
        let gradient_y = luminance(xi, yi + 1) - luminance(xi, yi - 1);
        let magnitude = (gradient_x * gradient_x + gradient_y * gradient_y).sqrt();
        let edge = (magnitude * STRUCTURE_MASK_GAIN).clamp(0.0, 1.0);
        let mask = STRUCTURE_MASK_FLOOR + (1.0 - STRUCTURE_MASK_FLOOR) * edge;
        [mask, mask, mask, mask]
    })?;

    low_pass_blur(&raw)
}

/// Deterministic separable binomial blur (radius 2, weights [1,4,6,4,1]/16)
/// with clamped edges. Used to extract the carrier's low-frequency band for
/// structure-preserving morph; the high-frequency band is `image - low_pass`.
fn low_pass_blur(image: &ImageBufferF32) -> Result<ImageBufferF32, RenderError> {
    const WEIGHTS: [f32; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];
    const RADIUS: i32 = 2;
    const INV_SUM: f32 = 1.0 / 16.0;
    let width = image.width as i32;
    let height = image.height as i32;

    // Horizontal pass.
    let horizontal = ImageBufferF32::from_fn(image.width, image.height, |x, y| {
        let mut accumulated = [0.0f32; 4];
        for (tap_index, weight) in WEIGHTS.iter().enumerate() {
            let sample_x = (x as i32 + tap_index as i32 - RADIUS).clamp(0, width - 1) as u32;
            let sample = image.pixel(sample_x, y).unwrap_or([0.0; 4]);
            for channel in 0..4 {
                accumulated[channel] += sample[channel] * weight;
            }
        }
        accumulated.map(|value| value * INV_SUM)
    })?;

    // Vertical pass.
    ImageBufferF32::from_fn(image.width, image.height, |x, y| {
        let mut accumulated = [0.0f32; 4];
        for (tap_index, weight) in WEIGHTS.iter().enumerate() {
            let sample_y = (y as i32 + tap_index as i32 - RADIUS).clamp(0, height - 1) as u32;
            let sample = horizontal.pixel(x, sample_y).unwrap_or([0.0; 4]);
            for channel in 0..4 {
                accumulated[channel] += sample[channel] * weight;
            }
        }
        accumulated.map(|value| value * INV_SUM)
    })
}

/// Applies centered flow-guided temporal integration to an exported frame
/// without changing the feedback state that produced it. `samples == 1`
/// returns the original float frame exactly, preserving the checkpoint path.
pub fn flow_temporal_supersample_cpu(
    image: &ImageBufferF32,
    flow: &FlowField,
    amount: f32,
    samples: u32,
) -> Result<ImageBufferF32, RenderError> {
    if image.width != flow.width || image.height != flow.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "image is {}x{}, flow is {}x{}",
            image.width, image.height, flow.width, flow.height
        )));
    }
    if !amount.is_finite() {
        return Err(RenderError::InvalidFlowFeedbackSettings(
            "temporal supersampling amount must be finite".to_string(),
        ));
    }
    if samples == 0 {
        return Err(RenderError::InvalidFlowFeedbackSettings(
            "temporal supersampling must use at least one sample".to_string(),
        ));
    }
    if samples == 1 {
        return Ok(image.clone());
    }

    ImageBufferF32::from_fn(image.width, image.height, |x, y| {
        let vector = flow.vector(x, y).unwrap_or([0.0, 0.0]);
        let mut accumulated = [0.0; 4];
        for sample_index in 0..samples {
            let shutter_offset = (sample_index as f32 + 0.5) / samples as f32 - 0.5;
            let sample = sample_bilinear_clamped(
                image,
                x as f32 + vector[0] * amount * shutter_offset,
                y as f32 + vector[1] * amount * shutter_offset,
            );
            for channel in 0..4 {
                accumulated[channel] += sample[channel];
            }
        }
        let inverse_sample_count = 1.0 / samples as f32;
        accumulated.map(|value| value * inverse_sample_count)
    })
}

fn mix_rgba(a: [f32; 4], b: [f32; 4], amount: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * amount,
        a[1] + (b[1] - a[1]) * amount,
        a[2] + (b[2] - a[2]) * amount,
        a[3] + (b[3] - a[3]) * amount,
    ]
}

fn scale_rgba(pixel: [f32; 4], scale: f32) -> [f32; 4] {
    [
        pixel[0] * scale,
        pixel[1] * scale,
        pixel[2] * scale,
        pixel[3] * scale,
    ]
}
