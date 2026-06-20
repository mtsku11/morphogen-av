use serde::{Deserialize, Serialize};

use crate::{sample_bilinear_clamped, FlowField, ImageBufferF32, RenderError};

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
}

impl FlowFeedbackSettings {
    pub fn validate(self) -> Result<(), RenderError> {
        for (name, value) in [
            ("carrier_amount", self.carrier_amount),
            ("feedback_amount", self.feedback_amount),
            ("feedback_mix", self.feedback_mix),
            ("decay", self.decay),
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
    ImageBufferF32::from_fn(carrier.width, carrier.height, |x, y| {
        let carrier_pixel = displaced_carrier.pixel(x, y).unwrap_or([0.0; 4]);
        let history_pixel = advected_history.pixel(x, y).unwrap_or([0.0; 4]);
        mix_rgba(
            carrier_pixel,
            scale_rgba(history_pixel, settings.decay),
            settings.feedback_mix,
        )
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
