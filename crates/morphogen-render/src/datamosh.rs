//! Controlled Datamosh / Motion-Vector Reuse (MVP): Source A's per-frame optical
//! flow repeatedly advects Source B's *previous output* — the signature
//! "bloom/melt" datamosh look, where a held carrier frame smears under motion
//! that never belonged to it. The pixel transform is the existing, parity-gated
//! flow displace (`flow_displace_cpu` / `flow_displace_metal`); the only new
//! logic lives here — the recursive accumulation + keyframe-refresh policy.
//!
//! This is the deterministic flow-reuse tier (real melt/bloom on decoded RGBA32F
//! frames), in the datamosh *family* but not the authentic macroblock/bitstream
//! artifact. See `docs/DATAMOSH_MILESTONE.md` for the contract.

use crate::cpu_reference::flow_displace_cpu;
use crate::flow::FlowField;
use crate::image_buffer::ImageBufferF32;
use crate::RenderError;

/// Datamosh policy identifier recorded on jobs/manifests. The underlying pixel op
/// is the existing `flow_displace`; this id names the recursive accumulation +
/// keyframe-refresh policy, distinct from every flow / granular / route id.
pub const DATAMOSH_BLOOM_ALGORITHM: &str = "flow_reuse_datamosh_bloom_cpu_v1";

/// Whether output frame `index` is a keyframe ("keep" / I-frame): it snaps back
/// to the carrier `B[index]` instead of advecting the held previous output.
///
/// `keyframe_interval` semantics:
/// - `1` ⇒ every frame is a keyframe ⇒ output is byte-identical to Source B
///   (the natural passthrough / "off").
/// - `N` (small) ⇒ keyframes at `0, N, 2N, …` ⇒ the periodic snap-back "pulse".
/// - `0` ⇒ only frame 0 is a keyframe ⇒ `B[0]` accumulates *all* of A's motion
///   (maximal melt/bloom).
///
/// Frame 0 is always a keyframe (frame-zero behavior: `out[0] = B[0]`).
pub fn is_datamosh_keyframe(index: usize, keyframe_interval: u32) -> bool {
    index == 0 || (keyframe_interval >= 1 && index % keyframe_interval as usize == 0)
}

/// Render one frame of recursive flow-reuse datamosh ("bloom/melt").
///
/// Stateful temporal node:
/// - **Frame-zero / keyframe:** `previous_output: None` *or* `is_keyframe` ⇒ the
///   carrier frame is returned unchanged (`B[index]`). Frame zero is reached via
///   `previous_output: None`.
/// - **Otherwise (P-frame delta):** the *previous output* (RGBA32F, unquantized)
///   is advected by A's optical flow scaled by `amount`. The carrier content is
///   frozen from the last keyframe and is **not** re-sampled here — only the held
///   buffer + the flow are read, which is what produces the melt.
///
/// Prior-frame state consumed: `previous_output`. Checkpoint representation: that
/// same RGBA32F buffer.
pub fn datamosh_bloom_frame_cpu(
    carrier: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    flow: &FlowField,
    is_keyframe: bool,
    amount: f32,
) -> Result<ImageBufferF32, RenderError> {
    match previous_output {
        // Frame zero or a keyframe refresh: the carrier is the output verbatim.
        None => Ok(carrier.clone()),
        Some(_) if is_keyframe => Ok(carrier.clone()),
        Some(previous_output) => {
            if previous_output.width != carrier.width || previous_output.height != carrier.height {
                return Err(RenderError::IncompatibleInputs(format!(
                    "previous output is {}x{}, carrier is {}x{}",
                    previous_output.width,
                    previous_output.height,
                    carrier.width,
                    carrier.height
                )));
            }
            flow_displace_cpu(previous_output, flow, amount)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, color: [f32; 4]) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |_, _| color).expect("buffer")
    }

    #[test]
    fn keyframe_predicate_matches_policy() {
        // interval 1: every frame keeps (passthrough).
        assert!(is_datamosh_keyframe(0, 1));
        assert!(is_datamosh_keyframe(3, 1));
        // interval 0: only frame zero keeps (full melt thereafter).
        assert!(is_datamosh_keyframe(0, 0));
        assert!(!is_datamosh_keyframe(1, 0));
        assert!(!is_datamosh_keyframe(7, 0));
        // interval 3: keep at 0, 3, 6; advect between.
        assert!(is_datamosh_keyframe(0, 3));
        assert!(!is_datamosh_keyframe(1, 3));
        assert!(!is_datamosh_keyframe(2, 3));
        assert!(is_datamosh_keyframe(3, 3));
        assert!(is_datamosh_keyframe(6, 3));
    }

    #[test]
    fn frame_zero_returns_carrier_verbatim() {
        let carrier = solid(2, 2, [0.25, 0.5, 0.75, 1.0]);
        let flow = FlowField::from_fn(2, 2, |_, _| [1.0, 0.0]).expect("flow");
        let out = datamosh_bloom_frame_cpu(&carrier, None, &flow, true, 1.0).expect("frame");
        assert_eq!(out, carrier);
    }

    #[test]
    fn keyframe_refresh_ignores_previous_output() {
        let carrier = solid(2, 2, [0.1, 0.2, 0.3, 1.0]);
        let previous = solid(2, 2, [0.9, 0.8, 0.7, 1.0]);
        let flow = FlowField::from_fn(2, 2, |_, _| [1.0, 0.0]).expect("flow");
        // A keyframe snaps back to the carrier regardless of the held state/flow.
        let out =
            datamosh_bloom_frame_cpu(&carrier, Some(&previous), &flow, true, 1.0).expect("frame");
        assert_eq!(out, carrier);
    }

    #[test]
    fn non_keyframe_advects_previous_output_not_carrier() {
        // Distinct carrier vs previous so we can tell which one is sampled.
        let carrier = solid(4, 1, [0.0, 0.0, 0.0, 1.0]);
        // Previous output: a horizontal ramp in the red channel.
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [x as f32 / 3.0, 0.0, 0.0, 1.0])
            .expect("previous");
        // Flow shifts sampling one pixel to the right (backward-sampling).
        let flow = FlowField::from_fn(4, 1, |_, _| [1.0, 0.0]).expect("flow");
        let out =
            datamosh_bloom_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0).expect("frame");

        // The result must come from advecting `previous`, not the black carrier.
        let direct = flow_displace_cpu(&previous, &flow, 1.0).expect("direct");
        assert_eq!(out, direct);
        // And it is not the carrier (which is all black).
        assert_ne!(out, carrier);
    }

    #[test]
    fn amount_zero_holds_previous_output() {
        let carrier = solid(3, 1, [0.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(3, 1, |x, _| [x as f32, 0.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(3, 1, |_, _| [1.0, 0.0]).expect("flow");
        // amount 0 ⇒ no displacement ⇒ the held buffer passes through unchanged.
        let out =
            datamosh_bloom_frame_cpu(&carrier, Some(&previous), &flow, false, 0.0).expect("frame");
        assert_eq!(out, previous);
    }

    #[test]
    fn mismatched_dimensions_error() {
        let carrier = solid(2, 2, [0.0, 0.0, 0.0, 1.0]);
        let previous = solid(3, 3, [0.0, 0.0, 0.0, 1.0]);
        let flow = FlowField::from_fn(2, 2, |_, _| [0.0, 0.0]).expect("flow");
        let result = datamosh_bloom_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0);
        assert!(result.is_err());
    }
}
