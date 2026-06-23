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

/// Codec-simulated ("block") datamosh policy id: identical recursion, but A's flow
/// is quantized to a coarse block grid before each advection so whole macroblocks
/// slide coherently — the chunky "real datamosh" look rather than the smooth
/// per-pixel bloom warp. The pixel op is still the parity-gated `flow_displace`;
/// the only new logic is `quantize_flow_to_blocks`.
pub const DATAMOSH_BLOCK_ALGORITHM: &str = "flow_reuse_datamosh_block_cpu_v1";

/// The datamosh policy id for a given `block_size`: the codec-simulated block id
/// when blocks are ≥ 2px, otherwise the smooth bloom id. A `block_size` of `0` or
/// `1` makes every pixel its own block ⇒ identical output to the bloom path, so it
/// is recorded under the bloom id (the natural "no macroblocking" continuity).
pub fn datamosh_algorithm(block_size: u32) -> &'static str {
    if block_size >= 2 {
        DATAMOSH_BLOCK_ALGORITHM
    } else {
        DATAMOSH_BLOOM_ALGORITHM
    }
}

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

/// Quantize a flow field to a `block_size`×`block_size` grid: every pixel in a
/// block is assigned that block's **mean** motion vector, so the subsequent
/// advection slides whole macroblocks coherently. `block_size` ≤ 1 returns the
/// flow unchanged (each pixel is its own block — the smooth bloom case). Edge
/// blocks average only the pixels they actually cover. Deterministic: fixed
/// iteration order, f64 accumulation, so identical input ⇒ identical output.
pub fn quantize_flow_to_blocks(
    flow: &FlowField,
    block_size: u32,
) -> Result<FlowField, RenderError> {
    if block_size <= 1 {
        return Ok(flow.clone());
    }
    let width = flow.width;
    let height = flow.height;
    let blocks_x = width.div_ceil(block_size);
    let blocks_y = height.div_ceil(block_size);
    let mut means = vec![[0.0f32, 0.0f32]; (blocks_x as usize) * (blocks_y as usize)];
    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let x0 = bx * block_size;
            let y0 = by * block_size;
            let x1 = (x0 + block_size).min(width);
            let y1 = (y0 + block_size).min(height);
            let mut sum = [0.0f64, 0.0f64];
            let mut count = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let vector = flow.vector(x, y).unwrap_or([0.0, 0.0]);
                    sum[0] += vector[0] as f64;
                    sum[1] += vector[1] as f64;
                    count += 1;
                }
            }
            if count > 0 {
                let inverse = 1.0 / count as f64;
                means[(by * blocks_x + bx) as usize] =
                    [(sum[0] * inverse) as f32, (sum[1] * inverse) as f32];
            }
        }
    }
    FlowField::from_fn(width, height, |x, y| {
        let bx = x / block_size;
        let by = y / block_size;
        means[(by * blocks_x + bx) as usize]
    })
}

/// Render one frame of codec-simulated ("block") datamosh. Identical to
/// [`datamosh_bloom_frame_cpu`] except the advecting flow is block-quantized first
/// (`quantize_flow_to_blocks`). `block_size` ≤ 1 makes it byte-identical to the
/// bloom frame. Frame-zero / keyframe behavior is unchanged (carrier verbatim).
pub fn datamosh_block_frame_cpu(
    carrier: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    flow: &FlowField,
    is_keyframe: bool,
    amount: f32,
    block_size: u32,
) -> Result<ImageBufferF32, RenderError> {
    match previous_output {
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
            let quantized = quantize_flow_to_blocks(flow, block_size)?;
            flow_displace_cpu(previous_output, &quantized, amount)
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

    #[test]
    fn algorithm_id_selects_block_only_for_coarse_blocks() {
        // 0/1 ⇒ each pixel its own block ⇒ bloom path (no macroblocking).
        assert_eq!(datamosh_algorithm(0), DATAMOSH_BLOOM_ALGORITHM);
        assert_eq!(datamosh_algorithm(1), DATAMOSH_BLOOM_ALGORITHM);
        // ≥ 2 ⇒ the codec-simulated block id.
        assert_eq!(datamosh_algorithm(2), DATAMOSH_BLOCK_ALGORITHM);
        assert_eq!(datamosh_algorithm(16), DATAMOSH_BLOCK_ALGORITHM);
    }

    #[test]
    fn quantize_block_size_one_or_zero_is_identity() {
        let flow = FlowField::from_fn(4, 3, |x, y| [x as f32, y as f32]).expect("flow");
        assert_eq!(quantize_flow_to_blocks(&flow, 0).expect("q0"), flow);
        assert_eq!(quantize_flow_to_blocks(&flow, 1).expect("q1"), flow);
    }

    #[test]
    fn quantize_assigns_block_mean_to_every_pixel_in_the_block() {
        // 2x2 image, one 2px block ⇒ every pixel gets the mean of all four vectors.
        let flow = FlowField::from_fn(2, 2, |x, y| [x as f32, y as f32]).expect("flow");
        // means: x in {0,1} ⇒ 0.5; y in {0,1} ⇒ 0.5.
        let quantized = quantize_flow_to_blocks(&flow, 2).expect("quantized");
        for y in 0..2 {
            for x in 0..2 {
                assert_eq!(quantized.vector(x, y), Some([0.5, 0.5]));
            }
        }
    }

    #[test]
    fn quantize_edge_block_averages_only_covered_pixels() {
        // 3px wide, block_size 2 ⇒ blocks cover columns {0,1} and {2}. The second
        // block has a single column, so its mean is that column's value exactly.
        let flow = FlowField::from_fn(3, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let quantized = quantize_flow_to_blocks(&flow, 2).expect("quantized");
        // Block 0 (x=0,1) ⇒ mean 0.5; block 1 (x=2) ⇒ 2.0.
        assert_eq!(quantized.vector(0, 0), Some([0.5, 0.0]));
        assert_eq!(quantized.vector(1, 0), Some([0.5, 0.0]));
        assert_eq!(quantized.vector(2, 0), Some([2.0, 0.0]));
    }

    #[test]
    fn block_frame_size_one_equals_bloom_frame() {
        let carrier = solid(4, 1, [0.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [x as f32 / 3.0, 0.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |_, _| [1.0, 0.0]).expect("flow");
        let bloom =
            datamosh_bloom_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0).expect("bloom");
        let block = datamosh_block_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0, 1)
            .expect("block");
        assert_eq!(block, bloom);
    }

    #[test]
    fn block_frame_quantizes_flow_before_advecting() {
        let carrier = solid(4, 1, [0.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [x as f32 / 3.0, 0.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let block = datamosh_block_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0, 2)
            .expect("block");
        // Must equal displacing by the *quantized* flow, not the raw flow.
        let quantized = quantize_flow_to_blocks(&flow, 2).expect("quantized");
        let expected = flow_displace_cpu(&previous, &quantized, 1.0).expect("expected");
        assert_eq!(block, expected);
        let raw = flow_displace_cpu(&previous, &flow, 1.0).expect("raw");
        assert_ne!(block, raw);
    }

    #[test]
    fn block_frame_zero_and_keyframe_return_carrier() {
        let carrier = solid(2, 2, [0.25, 0.5, 0.75, 1.0]);
        let previous = solid(2, 2, [0.9, 0.8, 0.7, 1.0]);
        let flow = FlowField::from_fn(2, 2, |_, _| [1.0, 0.0]).expect("flow");
        // Frame zero (no previous output).
        let zero =
            datamosh_block_frame_cpu(&carrier, None, &flow, true, 1.0, 16).expect("zero");
        assert_eq!(zero, carrier);
        // Keyframe refresh ignores held state + flow.
        let keyframe =
            datamosh_block_frame_cpu(&carrier, Some(&previous), &flow, true, 1.0, 16)
                .expect("keyframe");
        assert_eq!(keyframe, carrier);
    }
}
