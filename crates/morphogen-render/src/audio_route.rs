//! Audio-to-Video Descriptor Routing (MVP): Source A's RMS envelope drives the
//! per-frame displacement *amount* applied to Source B's frames. The pixel
//! transform is the existing, parity-gated flow displace (`flow_displace_cpu` /
//! `flow_displace_metal`); the only new deterministic logic lives here — turning
//! a peak-normalized RMS envelope into the scalar `amount` fed to that op.
//!
//! See `docs/AUDIO_VIDEO_ROUTE_MILESTONE.md` for the contract.

use crate::flow::FlowField;
use crate::RenderError;

/// Routing algorithm identifier recorded on jobs/manifests. The underlying pixel
/// op is the existing `flow_displace`; this id names the RMS-envelope→amount
/// routing policy, distinct from every flow / granular / vocoder id.
pub const RMS_DISPLACEMENT_ROUTE_ALGORITHM: &str = "rms_displacement_route_cpu_v1";

/// A peak-normalized RMS envelope, sampled by frame time with hold-last lookup.
///
/// Built from raw `(time_seconds, rms)` samples (time-ascending) so the render
/// crate stays decoupled from the audio crate. The loudest input RMS maps to a
/// normalized gain of `1.0`, silence to `0.0`.
#[derive(Debug, Clone)]
pub struct RmsDisplacementEnvelope {
    /// `(time_seconds, normalized_gain)` in time-ascending order; gain ∈ `[0,1]`.
    samples: Vec<(f64, f32)>,
}

impl RmsDisplacementEnvelope {
    /// Peak-normalize raw `(time, rms)` samples by their maximum RMS. Returns
    /// all-zero gains when the peak is ~0 (a silent modulator ⇒ no displacement).
    /// Samples are expected time-ascending (as `rms_envelope` produces them).
    pub fn from_rms_samples(samples: &[(f64, f32)]) -> Self {
        let peak = samples.iter().map(|(_, rms)| *rms).fold(0.0_f32, f32::max);
        let normalized = if peak <= 0.0 {
            samples.iter().map(|(time, _)| (*time, 0.0)).collect()
        } else {
            samples
                .iter()
                .map(|(time, rms)| (*time, (rms / peak).clamp(0.0, 1.0)))
                .collect()
        };
        Self {
            samples: normalized,
        }
    }

    /// Hold-last normalized gain at `time_seconds`: the latest sample at or
    /// before `time_seconds`. Returns `0.0` before the first sample (or when the
    /// envelope is empty) — the same convention the granular audio routing uses.
    pub fn gain_at(&self, time_seconds: f64) -> f32 {
        let count = self
            .samples
            .partition_point(|(time, _)| *time <= time_seconds);
        count
            .checked_sub(1)
            .and_then(|index| self.samples.get(index))
            .map(|(_, gain)| *gain)
            .unwrap_or(0.0)
    }
}

/// Build the fixed, procedural **uniform** displacement field: every pixel
/// carries the same `[shift_x, shift_y]` vector. The field is the displacement
/// *direction and unit magnitude*; the per-frame `amount` (from the RMS
/// envelope) is the loudness-driven scale applied by `flow_displace`.
pub fn uniform_displacement_field(
    width: u32,
    height: u32,
    shift_x: f32,
    shift_y: f32,
) -> Result<FlowField, RenderError> {
    FlowField::from_fn(width, height, |_, _| [shift_x, shift_y])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{flow_displace_cpu, ImageBufferF32};

    #[test]
    fn peak_normalizes_and_holds_last() {
        let envelope =
            RmsDisplacementEnvelope::from_rms_samples(&[(0.0, 0.5), (1.0, 1.0), (2.0, 0.0)]);

        // Normalized by peak (1.0): gains are [0.5, 1.0, 0.0].
        assert_eq!(envelope.gain_at(-0.1), 0.0); // before the first sample
        assert_eq!(envelope.gain_at(0.0), 0.5); // exactly at the first sample
        assert_eq!(envelope.gain_at(0.5), 0.5); // hold-last from t=0
        assert_eq!(envelope.gain_at(1.5), 1.0); // hold-last from the loud sample
        assert_eq!(envelope.gain_at(2.5), 0.0); // hold-last from the silent sample
    }

    #[test]
    fn peak_below_one_normalizes_to_full_gain() {
        // A quiet modulator (peak 0.25) still reaches gain 1.0 at its loudest.
        let envelope = RmsDisplacementEnvelope::from_rms_samples(&[(0.0, 0.1), (1.0, 0.25)]);
        assert!((envelope.gain_at(0.0) - 0.4).abs() < 1e-6);
        assert!((envelope.gain_at(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn silent_envelope_is_all_zero() {
        let envelope = RmsDisplacementEnvelope::from_rms_samples(&[(0.0, 0.0), (1.0, 0.0)]);
        assert_eq!(envelope.gain_at(0.0), 0.0);
        assert_eq!(envelope.gain_at(0.9), 0.0);
    }

    #[test]
    fn empty_envelope_yields_zero_gain() {
        let envelope = RmsDisplacementEnvelope::from_rms_samples(&[]);
        assert_eq!(envelope.gain_at(0.0), 0.0);
    }

    #[test]
    fn uniform_field_has_constant_vectors() {
        let field = uniform_displacement_field(3, 2, 8.0, -2.0).expect("valid field");
        assert_eq!(field.width, 3);
        assert_eq!(field.height, 2);
        for y in 0..2 {
            for x in 0..3 {
                assert_eq!(field.vector(x, y), Some([8.0, -2.0]));
            }
        }
    }

    #[test]
    fn zero_amount_is_passthrough() {
        let carrier = ImageBufferF32::new(
            2,
            1,
            vec![[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]],
        )
        .expect("valid carrier");
        let field = uniform_displacement_field(2, 1, 8.0, 0.0).expect("valid field");

        // Silence ⇒ gain 0 ⇒ amount 0 ⇒ each pixel samples itself ⇒ identity.
        let amount = 0.0;
        let rendered = flow_displace_cpu(&carrier, &field, amount).expect("render");
        assert_eq!(rendered.pixels, carrier.pixels);
    }

    #[test]
    fn loud_frame_displaces_by_amount() {
        // Left pixel red, right pixel blue. A unit +x shift at amount 1.0 makes
        // the left output sample the right input ⇒ left pixel turns blue.
        let carrier = ImageBufferF32::new(
            2,
            1,
            vec![[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]],
        )
        .expect("valid carrier");
        let field = uniform_displacement_field(2, 1, 1.0, 0.0).expect("valid field");

        let rendered = flow_displace_cpu(&carrier, &field, 1.0).expect("render");
        assert_eq!(rendered.pixels[0], [0.0, 0.0, 1.0, 1.0]); // sampled x=1 (blue)
        // The right pixel clamps at the border (x=2 → x=1), staying blue.
        assert_eq!(rendered.pixels[1], [0.0, 0.0, 1.0, 1.0]);
    }
}
