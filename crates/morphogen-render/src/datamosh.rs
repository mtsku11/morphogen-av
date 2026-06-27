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
use crate::sampler::sample_bilinear_clamped;
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

/// Block-residual datamosh policy id: the block recursion, but the intra-block
/// motion discarded by `quantize_flow_to_blocks` is **accumulated** in a per-pixel
/// residual buffer and re-injected (lagged) into the advecting flow — macroblocks
/// slide coherently *and* shed a trailing haze of the fine motion the coarse grid
/// couldn't represent. Still a pure flow→flow transform feeding the parity-gated
/// `flow_displace`; the new logic is the residual accumulation. A separate id (not
/// a descriptor dim on the block id), so it does not bump the block id.
pub const DATAMOSH_BLOCK_RESIDUAL_ALGORITHM: &str = "flow_reuse_datamosh_block_residual_cpu_v1";

/// Per-block keep/drop ("pseudo-keyframe") datamosh policy id: the block recursion,
/// but after the advect each macroblock whose **mean motion** falls below a
/// threshold "keeps" — it snaps back to the carrier `B[i]` (an intra/I-block
/// refresh) — while busier blocks are denied refresh and keep rotting. The patchy
/// "some macroblocks refresh, some rot" half of the aesthetic, content-driven like
/// a codec's intra-block map rather than injected noise. The refresh is a per-block
/// pixel composite over the *output* of the parity-gated `flow_displace`, so Metal
/// stays free. A separate id (not a descriptor dim on the block/residual id); it
/// names the most-specific active policy and takes precedence over residual in the
/// recorded label (the `residual_gain`/`residual_decay`/`block_refresh_threshold`
/// knobs are recorded separately and carry the rest).
pub const DATAMOSH_BLOCK_REFRESH_ALGORITHM: &str = "flow_reuse_datamosh_block_refresh_cpu_v1";

/// Vector-remix datamosh policy id: the per-block motion-vector grid (the FFglitch
/// "vector" unit — the same block-mean grid the block tier quantizes to) is
/// **remixed** (sorted by magnitude or seeded-shuffled across blocks) before the
/// advection, so motion is reorganized between macroblocks — the deterministic
/// "family look" of FFglitch's MV sort/shuffle, on the optical-flow field rather
/// than the codec bitstream. Still a pure flow→flow transform feeding the
/// parity-gated `flow_displace`, so Metal stays free. A separate id; the
/// `vector_remix` mode is recorded alongside.
pub const DATAMOSH_VECTOR_REMIX_ALGORITHM: &str = "flow_reuse_datamosh_vector_remix_cpu_v1";

/// Scanline-smear datamosh policy id: the recursive block mosh is followed by a
/// deterministic horizontal tear/debris pass driven by the same flow field. This
/// is the "glitch art postcard" tier: long lateral bands, retained hard edges,
/// and sparse chroma/black codec debris. It remains decoded-frame deterministic;
/// real bitstream corruption stays in the separate experimental command.
pub const DATAMOSH_SCANLINE_SMEAR_ALGORITHM: &str = "flow_reuse_datamosh_scanline_smear_cpu_v1";

/// Codec-engrave datamosh policy id: scanline smear plus an edge-aware detail
/// pass that engraves luma-edge hatching, block stepping, and RGB-channel offsets
/// back into the subject. This targets the dense internal detail seen in glitch
/// stills where the object remains readable but its surface is shredded.
pub const DATAMOSH_CODEC_ENGRAVE_ALGORITHM: &str = "flow_reuse_datamosh_codec_engrave_cpu_v1";

/// How the per-block motion-vector grid is remixed before advection (the FFglitch
/// "vector" operation, on the flow field). `None` (default) ⇒ no remix ⇒ the
/// block-quantized flow unchanged (byte-identical off path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VectorRemixMode {
    /// No remix — the block-quantized flow is used unchanged.
    #[default]
    None,
    /// Reassign block MVs in descending-magnitude order along the raster scan, so
    /// motion pools coherently across the frame (the "fluid sort" look).
    Sort,
    /// Apply a deterministic seeded permutation, so motion scrambles between blocks.
    Shuffle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanlineSmearSettings {
    pub line_height: u32,
    pub max_shift: f32,
    pub motion_gain: f32,
    pub wave_amplitude: f32,
    pub wave_frequency: f32,
    pub smear_mix: f32,
    pub structure_protect: f32,
    pub chroma_burst_rate: f32,
    pub chroma_burst_size: u32,
    pub seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodecEngraveSettings {
    pub block_size: u32,
    pub edge_gain: f32,
    pub hatch_strength: f32,
    pub hatch_frequency: f32,
    pub chroma_offset: f32,
    pub block_step: f32,
    pub foreground_boost: f32,
    pub micro_contrast: f32,
    pub seed: u64,
}

impl Default for CodecEngraveSettings {
    fn default() -> Self {
        Self {
            block_size: 4,
            edge_gain: 10.0,
            hatch_strength: 0.86,
            hatch_frequency: 1.06,
            chroma_offset: 2.8,
            block_step: 0.3,
            foreground_boost: 0.12,
            micro_contrast: 1.1,
            seed: 0,
        }
    }
}

impl Default for ScanlineSmearSettings {
    fn default() -> Self {
        Self {
            line_height: 2,
            max_shift: 96.0,
            motion_gain: 18.0,
            wave_amplitude: 10.0,
            wave_frequency: 0.37,
            smear_mix: 0.92,
            structure_protect: 0.72,
            chroma_burst_rate: 0.018,
            chroma_burst_size: 18,
            seed: 0,
        }
    }
}

/// The datamosh policy id for a given `block_size`, `residual_gain`,
/// `refresh_threshold`, and `remix_mode`. Precedence (most-specific active policy
/// wins):
/// - blocks ≤ 1 (each pixel its own block) ⇒ the smooth bloom id (block quantize,
///   residual, refresh, and remix are all no-ops without macroblocks);
/// - blocks ≥ 2px **and** `remix_mode != None` ⇒ the vector-remix id;
/// - blocks ≥ 2px **and** `refresh_threshold > 0` ⇒ the per-block refresh id;
/// - blocks ≥ 2px and `residual_gain > 0` ⇒ the block-residual id;
/// - blocks ≥ 2px otherwise ⇒ the codec-simulated block id.
pub fn datamosh_algorithm(
    block_size: u32,
    residual_gain: f32,
    refresh_threshold: f32,
    remix_mode: VectorRemixMode,
) -> &'static str {
    if block_size < 2 {
        DATAMOSH_BLOOM_ALGORITHM
    } else if remix_mode != VectorRemixMode::None {
        DATAMOSH_VECTOR_REMIX_ALGORITHM
    } else if refresh_threshold > 0.0 {
        DATAMOSH_BLOCK_REFRESH_ALGORITHM
    } else if residual_gain > 0.0 {
        DATAMOSH_BLOCK_RESIDUAL_ALGORITHM
    } else {
        DATAMOSH_BLOCK_ALGORITHM
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
                    previous_output.width, previous_output.height, carrier.width, carrier.height
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
    let (blocks_x, _blocks_y, means) = block_mean_grid(flow, block_size);
    FlowField::from_fn(flow.width, flow.height, |x, y| {
        let bx = x / block_size;
        let by = y / block_size;
        means[(by * blocks_x + bx) as usize]
    })
}

/// The per-block **mean** motion-vector grid: `(blocks_x, blocks_y, means)` where
/// `means[by*blocks_x + bx]` is the f64-accumulated mean MV of that block (edge
/// blocks average only the pixels they cover). Shared by `quantize_flow_to_blocks`
/// and `remix_block_vectors`; deterministic (fixed iteration order).
fn block_mean_grid(flow: &FlowField, block_size: u32) -> (u32, u32, Vec<[f32; 2]>) {
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
    (blocks_x, blocks_y, means)
}

/// One step of a deterministic splitmix64 PRNG — the seeded source for the shuffle
/// permutation (so a given `seed` always yields the same remix).
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Remix the per-block motion-vector grid (the FFglitch "vector" unit), then expand
/// it back to a full flow field — the parity-gated displace that follows is
/// untouched (Metal stays free, exactly like `quantize_flow_to_blocks`).
///
/// `mode`:
/// - [`VectorRemixMode::None`] (or `block_size ≤ 1`) ⇒ the block-quantized flow
///   unchanged (byte-identical off path — block quantize without macroblocks is a
///   no-op too).
/// - [`VectorRemixMode::Sort`] ⇒ reassign block MVs in **descending-magnitude**
///   order along the raster scan (top-left blocks take the strongest motion), so
///   motion pools coherently across the frame.
/// - [`VectorRemixMode::Shuffle`] ⇒ a deterministic seeded Fisher–Yates permutation
///   of the block MVs, so motion scrambles between blocks.
///
/// Both remix modes are pure **permutations** of the existing block MVs (no new
/// magnitudes are invented), so total motion energy is preserved — only its spatial
/// assignment changes.
pub fn remix_block_vectors(
    flow: &FlowField,
    block_size: u32,
    mode: VectorRemixMode,
    seed: u64,
) -> Result<FlowField, RenderError> {
    if mode == VectorRemixMode::None || block_size <= 1 {
        return quantize_flow_to_blocks(flow, block_size);
    }
    let (blocks_x, _blocks_y, means) = block_mean_grid(flow, block_size);
    let n = means.len();

    // `order[i]` = which source block supplies the MV for raster block `i`.
    let order: Vec<usize> = match mode {
        VectorRemixMode::Sort => {
            let mut idx: Vec<usize> = (0..n).collect();
            let mag2 = |v: [f32; 2]| (v[0] as f64) * (v[0] as f64) + (v[1] as f64) * (v[1] as f64);
            // Descending magnitude; ties keep raster order for determinism.
            idx.sort_by(|&a, &b| mag2(means[b]).total_cmp(&mag2(means[a])).then(a.cmp(&b)));
            idx
        }
        VectorRemixMode::Shuffle => {
            let mut idx: Vec<usize> = (0..n).collect();
            let mut state = seed;
            for i in (1..n).rev() {
                let j = (splitmix64(&mut state) % (i as u64 + 1)) as usize;
                idx.swap(i, j);
            }
            idx
        }
        VectorRemixMode::None => unreachable!("None handled above"),
    };

    let remixed: Vec<[f32; 2]> = (0..n).map(|i| means[order[i]]).collect();
    FlowField::from_fn(flow.width, flow.height, |x, y| {
        let bx = x / block_size;
        let by = y / block_size;
        remixed[(by * blocks_x + bx) as usize]
    })
}

/// Apply flow-driven horizontal scanline tearing plus sparse codec debris to an
/// already-rendered datamosh frame. Hard local luma edges reduce the smear mix so
/// silhouettes survive while flatter regions tear into lateral bands.
pub fn datamosh_scanline_smear_frame_cpu(
    image: &ImageBufferF32,
    flow: &FlowField,
    frame_index: u32,
    settings: ScanlineSmearSettings,
) -> Result<ImageBufferF32, RenderError> {
    validate_scanline_smear_settings(settings)?;
    if image.width != flow.width || image.height != flow.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "scanline smear image is {}x{}, flow is {}x{}",
            image.width, image.height, flow.width, flow.height
        )));
    }

    let bands = image.height.div_ceil(settings.line_height);
    let mut shifts = Vec::with_capacity(bands as usize);
    for band in 0..bands {
        shifts.push(scanline_band_shift(flow, band, frame_index, settings));
    }

    ImageBufferF32::from_fn(image.width, image.height, |x, y| {
        let band = y / settings.line_height;
        let shift = shifts.get(band as usize).copied().unwrap_or(0.0);
        let source = sample_bilinear_clamped(image, x as f32 - shift, y as f32);
        let original = image.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let edge = local_luma_edge(image, x, y);
        let protect = (edge * settings.structure_protect * 9.0).clamp(0.0, 0.85);
        let mix = settings.smear_mix * (1.0 - protect);
        let mut out = mix_rgba(original, source, mix);
        if let Some(debris) = scanline_debris_pixel(x, y, frame_index, edge, settings) {
            out = mix_rgba(out, debris, 0.88);
        }
        out
    })
}

/// Engrave compressed motion detail into a datamosh frame using the current
/// carrier's edges as a structure mask. The carrier contributes edge direction
/// and high-frequency luma only; the visible colour remains dominated by the
/// datamosh state, with deterministic RGB offsets and block stepping near edges.
pub fn datamosh_codec_engrave_frame_cpu(
    image: &ImageBufferF32,
    carrier: &ImageBufferF32,
    flow: &FlowField,
    frame_index: u32,
    settings: CodecEngraveSettings,
) -> Result<ImageBufferF32, RenderError> {
    validate_codec_engrave_settings(settings)?;
    if image.width != carrier.width || image.height != carrier.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "codec engrave image is {}x{}, carrier is {}x{}",
            image.width, image.height, carrier.width, carrier.height
        )));
    }
    if image.width != flow.width || image.height != flow.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "codec engrave image is {}x{}, flow is {}x{}",
            image.width, image.height, flow.width, flow.height
        )));
    }

    ImageBufferF32::from_fn(image.width, image.height, |x, y| {
        let carrier_pixel = carrier.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let [dx, dy] = luma_gradient(carrier, x, y);
        let edge = (dx * dx + dy * dy).sqrt();
        let texture = local_luma_texture(carrier, x, y);
        let flow_vector = flow.vector(x, y).unwrap_or([0.0, 0.0]);
        let motion = (flow_vector[0] * flow_vector[0] + flow_vector[1] * flow_vector[1]).sqrt();
        let detail = ((edge * settings.edge_gain + texture * settings.edge_gain * 0.6 - 0.08)
            * 1.2)
            .clamp(0.0, 1.0);
        let motion_gate = (0.45 + motion * 0.22).clamp(0.45, 1.0);
        let subject_presence = (detail + motion_gate * 0.3).clamp(0.0, 1.0);
        let foreground_gate = 1.0 + subject_presence * settings.foreground_boost;
        let gate = (detail * motion_gate * foreground_gate).clamp(0.0, 1.0);
        let stepped = block_step_pixel(image, flow_vector, x, y, frame_index, settings);
        let chroma = chroma_offset_pixel(image, x, y, flow_vector, settings.chroma_offset);
        let mut out = mix_rgba(stepped, chroma, gate * 0.62);

        let hatch = edge_hatch(dx, dy, carrier_pixel, x, y, frame_index, settings);
        let hatch_mix = gate * settings.hatch_strength;
        out = mix_rgba(out, hatch, hatch_mix);

        let contrast = 1.0 + gate * settings.micro_contrast;
        out = [
            ((out[0] - 0.5) * contrast + 0.5).clamp(0.0, 1.0),
            ((out[1] - 0.5) * contrast + 0.5).clamp(0.0, 1.0),
            ((out[2] - 0.5) * contrast + 0.5).clamp(0.0, 1.0),
            out[3].clamp(0.0, 1.0),
        ];
        mix_rgba(image.pixel(x, y).unwrap_or(out), out, gate)
    })
}

fn validate_scanline_smear_settings(settings: ScanlineSmearSettings) -> Result<(), RenderError> {
    if settings.line_height == 0 {
        return Err(RenderError::InvalidDatamoshSettings(
            "line_height must be greater than zero".to_string(),
        ));
    }
    if settings.chroma_burst_size == 0 {
        return Err(RenderError::InvalidDatamoshSettings(
            "chroma_burst_size must be greater than zero".to_string(),
        ));
    }
    for (name, value) in [
        ("max_shift", settings.max_shift),
        ("motion_gain", settings.motion_gain),
        ("wave_amplitude", settings.wave_amplitude),
        ("wave_frequency", settings.wave_frequency),
        ("smear_mix", settings.smear_mix),
        ("structure_protect", settings.structure_protect),
        ("chroma_burst_rate", settings.chroma_burst_rate),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(RenderError::InvalidDatamoshSettings(format!(
                "{name} must be finite and non-negative"
            )));
        }
    }
    if settings.smear_mix > 1.0 {
        return Err(RenderError::InvalidDatamoshSettings(
            "smear_mix must be in [0, 1]".to_string(),
        ));
    }
    if settings.structure_protect > 1.0 {
        return Err(RenderError::InvalidDatamoshSettings(
            "structure_protect must be in [0, 1]".to_string(),
        ));
    }
    if settings.chroma_burst_rate > 1.0 {
        return Err(RenderError::InvalidDatamoshSettings(
            "chroma_burst_rate must be in [0, 1]".to_string(),
        ));
    }
    Ok(())
}

fn validate_codec_engrave_settings(settings: CodecEngraveSettings) -> Result<(), RenderError> {
    if settings.block_size == 0 {
        return Err(RenderError::InvalidDatamoshSettings(
            "codec engrave block_size must be greater than zero".to_string(),
        ));
    }
    for (name, value) in [
        ("edge_gain", settings.edge_gain),
        ("hatch_strength", settings.hatch_strength),
        ("hatch_frequency", settings.hatch_frequency),
        ("chroma_offset", settings.chroma_offset),
        ("block_step", settings.block_step),
        ("foreground_boost", settings.foreground_boost),
        ("micro_contrast", settings.micro_contrast),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(RenderError::InvalidDatamoshSettings(format!(
                "{name} must be finite and non-negative"
            )));
        }
    }
    if settings.hatch_strength > 1.0 {
        return Err(RenderError::InvalidDatamoshSettings(
            "hatch_strength must be in [0, 1]".to_string(),
        ));
    }
    if settings.block_step > 1.0 {
        return Err(RenderError::InvalidDatamoshSettings(
            "block_step must be in [0, 1]".to_string(),
        ));
    }
    Ok(())
}

fn block_step_pixel(
    image: &ImageBufferF32,
    flow_vector: [f32; 2],
    x: u32,
    y: u32,
    frame_index: u32,
    settings: CodecEngraveSettings,
) -> [f32; 4] {
    let bx = x / settings.block_size;
    let by = y / settings.block_size;
    let x0 = bx * settings.block_size;
    let y0 = by * settings.block_size;
    let noise = signed_noise(
        settings.seed ^ 0xC0DE_EE11_6EA7_0001,
        frame_index,
        bx.wrapping_mul(97).wrapping_add(by),
    );
    let jitter = noise * settings.block_step * settings.block_size as f32;
    let step_x = x0 as f32 + flow_vector[0] * settings.block_step + jitter;
    let step_y = y0 as f32 + flow_vector[1].signum() * settings.block_step * 1.25;
    sample_bilinear_clamped(
        image,
        mix_scalar(x as f32, step_x, settings.block_step),
        mix_scalar(y as f32, step_y, settings.block_step * 0.55),
    )
}

fn chroma_offset_pixel(
    image: &ImageBufferF32,
    x: u32,
    y: u32,
    flow_vector: [f32; 2],
    amount: f32,
) -> [f32; 4] {
    let direction = if flow_vector[0].abs() > 0.01 {
        flow_vector[0].signum()
    } else {
        1.0
    };
    let red = sample_bilinear_clamped(image, x as f32 + amount * direction, y as f32)[0];
    let green = sample_bilinear_clamped(image, x as f32, y as f32 + flow_vector[1] * 0.4)[1];
    let blue = sample_bilinear_clamped(image, x as f32 - amount * direction, y as f32)[2];
    [red, green, blue, image.pixel(x, y).map_or(1.0, |p| p[3])]
}

fn edge_hatch(
    dx: f32,
    dy: f32,
    carrier_pixel: [f32; 4],
    x: u32,
    y: u32,
    frame_index: u32,
    settings: CodecEngraveSettings,
) -> [f32; 4] {
    let length = (dx * dx + dy * dy).sqrt();
    let tangent = if length > 1e-5 {
        [-dy / length, dx / length]
    } else {
        [1.0, 0.0]
    };
    let phase_noise = signed_noise(
        settings.seed ^ 0x51A7_1C00_D37A_1101,
        frame_index,
        (x / settings.block_size).wrapping_add((y / settings.block_size) * 8191),
    );
    let coordinate = x as f32 * tangent[0] + y as f32 * tangent[1];
    let stripe =
        (coordinate * settings.hatch_frequency + frame_index as f32 * 0.31 + phase_noise).sin();
    let base_luma = luma(carrier_pixel);
    let engraved = if stripe >= 0.0 {
        (base_luma + 0.38).clamp(0.0, 1.0)
    } else {
        (base_luma - 0.42).clamp(0.0, 1.0)
    };
    let tint = if stripe >= 0.0 {
        [0.92, 1.0, 0.78]
    } else {
        [0.02, 0.02, 0.025]
    };
    [
        (carrier_pixel[0] * 0.32 + engraved * tint[0] * 0.68).clamp(0.0, 1.0),
        (carrier_pixel[1] * 0.32 + engraved * tint[1] * 0.68).clamp(0.0, 1.0),
        (carrier_pixel[2] * 0.32 + engraved * tint[2] * 0.68).clamp(0.0, 1.0),
        carrier_pixel[3],
    ]
}

fn scanline_band_shift(
    flow: &FlowField,
    band: u32,
    frame_index: u32,
    settings: ScanlineSmearSettings,
) -> f32 {
    let y0 = band * settings.line_height;
    let y1 = (y0 + settings.line_height).min(flow.height);
    let mut sum = [0.0f64, 0.0f64];
    let mut count = 0u64;
    for y in y0..y1 {
        for x in 0..flow.width {
            let vector = flow.vector(x, y).unwrap_or([0.0, 0.0]);
            sum[0] += vector[0] as f64;
            sum[1] += vector[1] as f64;
            count += 1;
        }
    }
    let inverse = if count == 0 { 0.0 } else { 1.0 / count as f64 };
    let mean_x = (sum[0] * inverse) as f32;
    let mean_y = (sum[1] * inverse) as f32;
    let motion = mean_x + mean_y.abs() * signed_noise(settings.seed, frame_index, band);
    let wave = ((band as f32 * settings.wave_frequency) + frame_index as f32 * 0.23).sin()
        * settings.wave_amplitude;
    (motion * settings.motion_gain + wave).clamp(-settings.max_shift, settings.max_shift)
}

fn scanline_debris_pixel(
    x: u32,
    y: u32,
    frame_index: u32,
    edge: f32,
    settings: ScanlineSmearSettings,
) -> Option<[f32; 4]> {
    if edge > 0.22 || settings.chroma_burst_rate <= 0.0 {
        return None;
    }
    let bx = x / settings.chroma_burst_size;
    let by = y / settings.line_height;
    let state = settings.seed
        ^ ((frame_index as u64) << 32)
        ^ ((bx as u64).wrapping_mul(0xA24B_AED4_963E_E407))
        ^ ((by as u64).wrapping_mul(0x9FB2_1C65_1E98_DF25));
    if unit_noise_from_state(state) >= settings.chroma_burst_rate {
        return None;
    }
    let palette_index = (unit_noise_from_state(state ^ 0xD1B5_4A32_D192_ED03) * 8.0) as u32;
    Some(match palette_index {
        0 => [0.0, 0.92, 0.95, 1.0],
        1 => [1.0, 0.0, 0.84, 1.0],
        2 => [0.68, 1.0, 0.0, 1.0],
        3 => [0.08, 0.08, 1.0, 1.0],
        4 => [1.0, 1.0, 1.0, 1.0],
        5 => [0.0, 0.0, 0.0, 1.0],
        6 => [1.0, 0.13, 0.0, 1.0],
        _ => [0.4, 0.1, 1.0, 1.0],
    })
}

fn luma_gradient(image: &ImageBufferF32, x: u32, y: u32) -> [f32; 2] {
    let left = image
        .pixel(x.saturating_sub(1), y)
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let right = image
        .pixel((x + 1).min(image.width - 1), y)
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let up = image
        .pixel(x, y.saturating_sub(1))
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let down = image
        .pixel(x, (y + 1).min(image.height - 1))
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    [luma(right) - luma(left), luma(down) - luma(up)]
}

fn local_luma_texture(image: &ImageBufferF32, x: u32, y: u32) -> f32 {
    let center = image.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let center_luma = luma(center);
    let mut sum = 0.0;
    let mut count = 0.0;
    let x0 = x.saturating_sub(1);
    let y0 = y.saturating_sub(1);
    let x1 = (x + 1).min(image.width - 1);
    let y1 = (y + 1).min(image.height - 1);
    for yy in y0..=y1 {
        for xx in x0..=x1 {
            if xx == x && yy == y {
                continue;
            }
            let neighbor = image.pixel(xx, yy).unwrap_or(center);
            sum += (luma(neighbor) - center_luma).abs();
            count += 1.0;
        }
    }
    if count == 0.0 {
        0.0
    } else {
        sum / count
    }
}

fn mix_scalar(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn signed_noise(seed: u64, frame_index: u32, band: u32) -> f32 {
    unit_noise_from_state(
        seed ^ ((frame_index as u64) << 32) ^ (band as u64).wrapping_mul(0x632B_E59B_D9B4_E019),
    ) * 2.0
        - 1.0
}

fn unit_noise_from_state(mut state: u64) -> f32 {
    let value = splitmix64(&mut state);
    ((value >> 40) as f32) / ((1u64 << 24) as f32)
}

fn local_luma_edge(image: &ImageBufferF32, x: u32, y: u32) -> f32 {
    let left = image
        .pixel(x.saturating_sub(1), y)
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let right = image
        .pixel((x + 1).min(image.width - 1), y)
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let up = image
        .pixel(x, y.saturating_sub(1))
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let down = image
        .pixel(x, (y + 1).min(image.height - 1))
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    (luma(right) - luma(left)).abs() + (luma(down) - luma(up)).abs()
}

fn luma(pixel: [f32; 4]) -> f32 {
    pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722
}

fn mix_rgba(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
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
                    previous_output.width, previous_output.height, carrier.width, carrier.height
                )));
            }
            let quantized = quantize_flow_to_blocks(flow, block_size)?;
            flow_displace_cpu(previous_output, &quantized, amount)
        }
    }
}

/// A zero-valued (no-motion) flow field — the reset state for the residual
/// accumulator at frame zero and every keyframe.
pub fn zero_flow(width: u32, height: u32) -> Result<FlowField, RenderError> {
    FlowField::from_fn(width, height, |_, _| [0.0, 0.0])
}

/// The block-residual **flow transform** for one P-frame: quantize `flow` to a
/// block grid, accumulate the discarded intra-block residual (`flow − block_mean`)
/// into the per-pixel state buffer with `residual_decay`, and return the
/// `(effective_flow, new_accumulator)` where
/// `effective = block_mean + accumulator·residual_gain`.
///
/// This is the *pure flow→flow* core — no advection — so the recursive render loop
/// can feed `effective_flow` to the parity-gated displace on **either** backend
/// (Metal stays free, exactly as the block tier). `block_size ≤ 1` ⇒ `block_mean =
/// flow` ⇒ residual `0`; with a zero accumulator that yields `effective = flow`
/// (the smooth bloom warp).
pub fn datamosh_residual_flow(
    flow: &FlowField,
    accumulated_residual: &FlowField,
    block_size: u32,
    residual_gain: f32,
    residual_decay: f32,
) -> Result<(FlowField, FlowField), RenderError> {
    let quantized = quantize_flow_to_blocks(flow, block_size)?;
    // accum[p] = accum[p]·decay + (flow[p] − block_mean[p])
    let new_accum = FlowField::from_fn(flow.width, flow.height, |x, y| {
        let f = flow.vector(x, y).unwrap_or([0.0, 0.0]);
        let q = quantized.vector(x, y).unwrap_or([0.0, 0.0]);
        let a = accumulated_residual.vector(x, y).unwrap_or([0.0, 0.0]);
        [
            a[0] * residual_decay + (f[0] - q[0]),
            a[1] * residual_decay + (f[1] - q[1]),
        ]
    })?;
    // effective[p] = block_mean[p] + accum[p]·gain
    let effective = FlowField::from_fn(flow.width, flow.height, |x, y| {
        let q = quantized.vector(x, y).unwrap_or([0.0, 0.0]);
        let a = new_accum.vector(x, y).unwrap_or([0.0, 0.0]);
        [q[0] + a[0] * residual_gain, q[1] + a[1] * residual_gain]
    })?;
    Ok((effective, new_accum))
}

/// Render one frame of **block-residual** datamosh and return the updated residual
/// accumulator alongside the output frame.
///
/// Extends [`datamosh_block_frame_cpu`]: the intra-block motion discarded by
/// `quantize_flow_to_blocks` (`resid = flow − block_mean`) is accumulated in a
/// per-pixel residual flow buffer and re-injected (lagged) into the advecting flow
/// (`effective = block_mean + accum·residual_gain`). The advecting pixel op is
/// still the parity-gated `flow_displace`.
///
/// State (second stateful channel alongside `previous_output`):
/// - `accumulated_residual` — the prior-frame residual buffer (2-channel
///   `FlowField`, carrier dims). Returned updated as the second tuple element.
/// - **Frame-zero (`previous_output: None`) and every keyframe reset it to zero**
///   (an I-frame refresh clears accumulated residual), returning the carrier
///   verbatim plus a zeroed accumulator.
///
/// Continuity: `residual_gain == 0` short-circuits to the block path
/// (byte-identical, zeroed accumulator returned). `block_size ≤ 1` ⇒ `resid = 0`
/// ⇒ the accumulator stays zero ⇒ byte-identical to the bloom path.
#[allow(clippy::too_many_arguments)]
pub fn datamosh_residual_frame_cpu(
    carrier: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    accumulated_residual: &FlowField,
    flow: &FlowField,
    is_keyframe: bool,
    amount: f32,
    block_size: u32,
    residual_gain: f32,
    residual_decay: f32,
) -> Result<(ImageBufferF32, FlowField), RenderError> {
    // Gain 0 ⇒ no residual is ever re-injected ⇒ exactly the block path. Short-
    // circuit so the byte-identity continuity is guaranteed, not float-incidental.
    if residual_gain == 0.0 {
        let out = datamosh_block_frame_cpu(
            carrier,
            previous_output,
            flow,
            is_keyframe,
            amount,
            block_size,
        )?;
        return Ok((out, zero_flow(carrier.width, carrier.height)?));
    }

    match previous_output {
        // Frame zero or a keyframe refresh: carrier verbatim, accumulator cleared.
        None => Ok((carrier.clone(), zero_flow(carrier.width, carrier.height)?)),
        Some(_) if is_keyframe => Ok((carrier.clone(), zero_flow(carrier.width, carrier.height)?)),
        Some(previous_output) => {
            if previous_output.width != carrier.width || previous_output.height != carrier.height {
                return Err(RenderError::IncompatibleInputs(format!(
                    "previous output is {}x{}, carrier is {}x{}",
                    previous_output.width, previous_output.height, carrier.width, carrier.height
                )));
            }
            let (effective, new_accum) = datamosh_residual_flow(
                flow,
                accumulated_residual,
                block_size,
                residual_gain,
                residual_decay,
            )?;
            let out = flow_displace_cpu(previous_output, &effective, amount)?;
            Ok((out, new_accum))
        }
    }
}

/// Per-block keep/drop refresh decision: a macroblock "keeps" (snaps back to the
/// carrier `B[i]`, an intra/I-block refresh) when its **mean motion magnitude** is
/// *below* `refresh_threshold` — calm regions stay crisp while busy regions are
/// denied refresh and keep rotting under the reused flow (the controlled analogue
/// of a codec's intra-block map). `refresh_threshold <= 0` ⇒ no block ever
/// refreshes (the plain block/residual path); a threshold above the largest block
/// motion ⇒ every block refreshes (the carrier verbatim).
pub fn block_motion_refreshes(block_mean: [f32; 2], refresh_threshold: f32) -> bool {
    if refresh_threshold <= 0.0 {
        return false;
    }
    let magnitude = (block_mean[0] * block_mean[0] + block_mean[1] * block_mean[1]).sqrt();
    magnitude < refresh_threshold
}

/// Composite a per-block keep/drop refresh into an advected datamosh frame: pixels
/// whose block "keeps" (`block_motion_refreshes`) take the carrier `B[i]`; the rest
/// keep the advected (rotted) content. `block_means` is the quantized flow
/// (`quantize_flow_to_blocks`), so every pixel reads its own block's mean motion.
///
/// A pure CPU post-step over the *output* of the parity-gated displace, so it is
/// identical regardless of which backend produced `advected` — Metal stays free,
/// exactly as the block / residual tiers.
pub fn datamosh_block_refresh_composite(
    advected: &ImageBufferF32,
    carrier: &ImageBufferF32,
    block_means: &FlowField,
    refresh_threshold: f32,
) -> Result<ImageBufferF32, RenderError> {
    if advected.width != carrier.width || advected.height != carrier.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "advected frame is {}x{}, carrier is {}x{}",
            advected.width, advected.height, carrier.width, carrier.height
        )));
    }
    ImageBufferF32::from_fn(advected.width, advected.height, |x, y| {
        let mean = block_means.vector(x, y).unwrap_or([0.0, 0.0]);
        let source = if block_motion_refreshes(mean, refresh_threshold) {
            carrier
        } else {
            advected
        };
        source.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 0.0])
    })
}

/// Clear the residual accumulator in every block that "keeps" (refreshes to the
/// carrier): an intra-block refresh discards that block's accumulated prediction
/// state, matching the whole-frame keyframe reset. Blocks that keep rotting retain
/// their accumulator. Used only on the residual path; the pure block path has no
/// accumulator to reset.
pub fn reset_residual_in_refreshed_blocks(
    accumulated_residual: &FlowField,
    block_means: &FlowField,
    refresh_threshold: f32,
) -> Result<FlowField, RenderError> {
    FlowField::from_fn(
        accumulated_residual.width,
        accumulated_residual.height,
        |x, y| {
            let mean = block_means.vector(x, y).unwrap_or([0.0, 0.0]);
            if block_motion_refreshes(mean, refresh_threshold) {
                [0.0, 0.0]
            } else {
                accumulated_residual.vector(x, y).unwrap_or([0.0, 0.0])
            }
        },
    )
}

/// Render one frame of **per-block keep/drop** datamosh and return the updated
/// residual accumulator. Extends [`datamosh_residual_frame_cpu`]: after the advect,
/// each macroblock whose mean motion is below `refresh_threshold` snaps back to the
/// carrier `B[i]` (an intra-block refresh) while busier blocks keep rotting;
/// refreshed blocks also clear their residual accumulator.
///
/// Continuity:
/// - `refresh_threshold <= 0` ⇒ byte-identical to [`datamosh_residual_frame_cpu`]
///   (no block refreshes);
/// - a threshold above the largest block motion ⇒ every block refreshes ⇒ the
///   carrier verbatim with a cleared accumulator (byte-identical to a whole-frame
///   keyframe);
/// - `block_size <= 1` ⇒ the bloom path (refresh ignored, like residual).
///
/// Frame-zero / keyframe return the carrier verbatim with a zeroed accumulator,
/// unchanged (refresh only acts on a P-frame).
#[allow(clippy::too_many_arguments)]
pub fn datamosh_refresh_frame_cpu(
    carrier: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    accumulated_residual: &FlowField,
    flow: &FlowField,
    is_keyframe: bool,
    amount: f32,
    block_size: u32,
    residual_gain: f32,
    residual_decay: f32,
    refresh_threshold: f32,
) -> Result<(ImageBufferF32, FlowField), RenderError> {
    let (advected, new_accum) = datamosh_residual_frame_cpu(
        carrier,
        previous_output,
        accumulated_residual,
        flow,
        is_keyframe,
        amount,
        block_size,
        residual_gain,
        residual_decay,
    )?;
    // Refresh only acts on a P-frame over coarse blocks; frame-zero / keyframe are
    // already the carrier verbatim (every pixel "refreshed") with a zero accum.
    let is_p_frame = previous_output.is_some() && !is_keyframe;
    if !is_p_frame || refresh_threshold <= 0.0 || block_size < 2 {
        return Ok((advected, new_accum));
    }
    let block_means = quantize_flow_to_blocks(flow, block_size)?;
    let out =
        datamosh_block_refresh_composite(&advected, carrier, &block_means, refresh_threshold)?;
    let reset_accum =
        reset_residual_in_refreshed_blocks(&new_accum, &block_means, refresh_threshold)?;
    Ok((out, reset_accum))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, color: [f32; 4]) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |_, _| color).expect("buffer")
    }

    #[test]
    fn scanline_smear_is_deterministic_and_moves_horizontal_content() {
        let image = ImageBufferF32::from_fn(6, 2, |x, y| [x as f32 / 5.0, y as f32, 0.0, 1.0])
            .expect("image");
        let flow = FlowField::from_fn(6, 2, |_, _| [2.0, 0.0]).expect("flow");
        let settings = ScanlineSmearSettings {
            line_height: 1,
            max_shift: 4.0,
            motion_gain: 1.0,
            wave_amplitude: 0.0,
            smear_mix: 1.0,
            structure_protect: 0.0,
            chroma_burst_rate: 0.0,
            ..ScanlineSmearSettings::default()
        };

        let first =
            datamosh_scanline_smear_frame_cpu(&image, &flow, 3, settings).expect("first render");
        let second =
            datamosh_scanline_smear_frame_cpu(&image, &flow, 3, settings).expect("second render");

        assert_eq!(first, second);
        assert_eq!(first.width, image.width);
        assert_eq!(first.height, image.height);
        assert_ne!(first, image);
        assert!(
            first.pixel(3, 0).expect("pixel")[0] < image.pixel(3, 0).expect("source pixel")[0],
            "positive row shift samples from the left side of the ramp"
        );
    }

    #[test]
    fn scanline_smear_rejects_mismatched_flow_dimensions() {
        let image = solid(2, 2, [0.2, 0.3, 0.4, 1.0]);
        let flow = FlowField::from_fn(3, 2, |_, _| [0.0, 0.0]).expect("flow");
        let result =
            datamosh_scanline_smear_frame_cpu(&image, &flow, 0, ScanlineSmearSettings::default());

        assert!(matches!(result, Err(RenderError::IncompatibleInputs(_))));
    }

    #[test]
    fn scanline_smear_debris_is_seeded_and_sparse_controlled() {
        let image = solid(4, 2, [0.1, 0.1, 0.1, 1.0]);
        let flow = FlowField::from_fn(4, 2, |_, _| [0.0, 0.0]).expect("flow");
        let settings = ScanlineSmearSettings {
            line_height: 1,
            max_shift: 0.0,
            motion_gain: 0.0,
            wave_amplitude: 0.0,
            smear_mix: 0.0,
            structure_protect: 0.0,
            chroma_burst_rate: 1.0,
            chroma_burst_size: 2,
            seed: 7,
            ..ScanlineSmearSettings::default()
        };

        let first =
            datamosh_scanline_smear_frame_cpu(&image, &flow, 2, settings).expect("first render");
        let second =
            datamosh_scanline_smear_frame_cpu(&image, &flow, 2, settings).expect("second render");

        assert_eq!(first, second);
        assert_ne!(first, image);
    }

    #[test]
    fn codec_engrave_is_deterministic_and_changes_edge_rich_regions() {
        let image = ImageBufferF32::from_fn(6, 3, |x, y| {
            [0.25 + x as f32 * 0.04, 0.35 + y as f32 * 0.05, 0.4, 1.0]
        })
        .expect("image");
        let carrier = ImageBufferF32::from_fn(6, 3, |x, _| {
            if x < 3 {
                [0.05, 0.05, 0.05, 1.0]
            } else {
                [0.95, 0.95, 0.95, 1.0]
            }
        })
        .expect("carrier");
        let flow = FlowField::from_fn(6, 3, |_, _| [1.5, 0.0]).expect("flow");
        let settings = CodecEngraveSettings {
            block_size: 2,
            chroma_offset: 1.0,
            seed: 11,
            ..CodecEngraveSettings::default()
        };

        let first =
            datamosh_codec_engrave_frame_cpu(&image, &carrier, &flow, 5, settings).expect("first");
        let second =
            datamosh_codec_engrave_frame_cpu(&image, &carrier, &flow, 5, settings).expect("second");

        assert_eq!(first, second);
        assert_ne!(first, image);
        assert!(
            first
                .max_channel_difference(&image)
                .expect("same dimensions")
                > 0.1,
            "edge hatching should visibly modify the datamosh frame"
        );
    }

    #[test]
    fn codec_engrave_rejects_mismatched_carrier_dimensions() {
        let image = solid(2, 2, [0.2, 0.3, 0.4, 1.0]);
        let carrier = solid(3, 2, [0.2, 0.3, 0.4, 1.0]);
        let flow = FlowField::from_fn(2, 2, |_, _| [0.0, 0.0]).expect("flow");
        let result = datamosh_codec_engrave_frame_cpu(
            &image,
            &carrier,
            &flow,
            0,
            CodecEngraveSettings::default(),
        );

        assert!(matches!(result, Err(RenderError::IncompatibleInputs(_))));
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
        let previous =
            ImageBufferF32::from_fn(3, 1, |x, _| [x as f32, 0.0, 0.0, 1.0]).expect("previous");
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
        let none = VectorRemixMode::None;
        // 0/1 ⇒ each pixel its own block ⇒ bloom path (no macroblocking).
        assert_eq!(
            datamosh_algorithm(0, 0.0, 0.0, none),
            DATAMOSH_BLOOM_ALGORITHM
        );
        assert_eq!(
            datamosh_algorithm(1, 0.0, 0.0, none),
            DATAMOSH_BLOOM_ALGORITHM
        );
        // ≥ 2 with no residual ⇒ the codec-simulated block id.
        assert_eq!(
            datamosh_algorithm(2, 0.0, 0.0, none),
            DATAMOSH_BLOCK_ALGORITHM
        );
        assert_eq!(
            datamosh_algorithm(16, 0.0, 0.0, none),
            DATAMOSH_BLOCK_ALGORITHM
        );
    }

    #[test]
    fn algorithm_id_selects_residual_only_when_active() {
        let none = VectorRemixMode::None;
        // Residual id requires BOTH coarse blocks and a positive gain.
        assert_eq!(
            datamosh_algorithm(16, 0.5, 0.0, none),
            DATAMOSH_BLOCK_RESIDUAL_ALGORITHM
        );
        // Gain 0 ⇒ block id even with coarse blocks.
        assert_eq!(
            datamosh_algorithm(16, 0.0, 0.0, none),
            DATAMOSH_BLOCK_ALGORITHM
        );
        // Residual is a no-op without quantization ⇒ bloom id regardless of gain.
        assert_eq!(
            datamosh_algorithm(1, 0.5, 0.0, none),
            DATAMOSH_BLOOM_ALGORITHM
        );
        assert_eq!(
            datamosh_algorithm(0, 0.9, 0.0, none),
            DATAMOSH_BLOOM_ALGORITHM
        );
    }

    #[test]
    fn algorithm_id_selects_refresh_when_active() {
        let none = VectorRemixMode::None;
        // Refresh id requires coarse blocks AND a positive threshold; it takes
        // precedence over residual in the recorded label.
        assert_eq!(
            datamosh_algorithm(16, 0.0, 0.5, none),
            DATAMOSH_BLOCK_REFRESH_ALGORITHM
        );
        assert_eq!(
            datamosh_algorithm(16, 0.5, 0.5, none),
            DATAMOSH_BLOCK_REFRESH_ALGORITHM
        );
        // Threshold 0 ⇒ falls back to residual / block as before.
        assert_eq!(
            datamosh_algorithm(16, 0.5, 0.0, none),
            DATAMOSH_BLOCK_RESIDUAL_ALGORITHM
        );
        assert_eq!(
            datamosh_algorithm(16, 0.0, 0.0, none),
            DATAMOSH_BLOCK_ALGORITHM
        );
        // Refresh is a no-op without quantization ⇒ bloom id regardless of threshold.
        assert_eq!(
            datamosh_algorithm(1, 0.0, 0.5, none),
            DATAMOSH_BLOOM_ALGORITHM
        );
    }

    #[test]
    fn algorithm_id_selects_vector_remix_when_active() {
        // Remix id requires coarse blocks AND a non-None mode; it takes precedence
        // over refresh/residual in the recorded label.
        assert_eq!(
            datamosh_algorithm(16, 0.0, 0.0, VectorRemixMode::Sort),
            DATAMOSH_VECTOR_REMIX_ALGORITHM
        );
        assert_eq!(
            datamosh_algorithm(16, 0.5, 0.5, VectorRemixMode::Shuffle),
            DATAMOSH_VECTOR_REMIX_ALGORITHM
        );
        // None ⇒ falls back to the prior precedence.
        assert_eq!(
            datamosh_algorithm(16, 0.0, 0.0, VectorRemixMode::None),
            DATAMOSH_BLOCK_ALGORITHM
        );
        // Remix is a no-op without quantization ⇒ bloom id regardless of mode.
        assert_eq!(
            datamosh_algorithm(1, 0.0, 0.0, VectorRemixMode::Sort),
            DATAMOSH_BLOOM_ALGORITHM
        );
    }

    #[test]
    fn refresh_threshold_zero_equals_residual_frame() {
        let carrier = solid(4, 1, [0.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [x as f32 / 3.0, 0.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let accum = zero_flow(4, 1).expect("accum");
        // Threshold 0 ⇒ no block refreshes ⇒ byte-identical to the residual frame.
        let (out, new_accum) = datamosh_refresh_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            2,
            0.5,
            0.9,
            0.0,
        )
        .expect("refresh");
        let (residual_out, residual_accum) = datamosh_residual_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            2,
            0.5,
            0.9,
        )
        .expect("residual");
        assert_eq!(out, residual_out);
        for x in 0..4 {
            assert_eq!(new_accum.vector(x, 0), residual_accum.vector(x, 0));
        }
    }

    #[test]
    fn refresh_high_threshold_equals_carrier_keyframe() {
        let carrier = solid(4, 1, [0.2, 0.4, 0.6, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [0.0, x as f32 / 3.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let accum = zero_flow(4, 1).expect("accum");
        // A threshold above any block's motion ⇒ every block refreshes ⇒ the
        // carrier verbatim with a fully-cleared accumulator (≡ a whole-frame
        // keyframe), even with residual active.
        let (out, new_accum) = datamosh_refresh_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            2,
            0.5,
            0.9,
            1000.0,
        )
        .expect("refresh");
        assert_eq!(out, carrier);
        for x in 0..4 {
            assert_eq!(new_accum.vector(x, 0), Some([0.0, 0.0]));
        }
    }

    #[test]
    fn refresh_keeps_calm_blocks_and_rots_busy_blocks() {
        let carrier = solid(4, 1, [1.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [0.0, x as f32 / 3.0, 0.0, 1.0])
            .expect("previous");
        // Block 0 (x=0,1): zero mean motion (calm). Block 1 (x=2,3): large motion (busy).
        let flow = FlowField::from_fn(4, 1, |x, _| if x < 2 { [0.0, 0.0] } else { [10.0, 0.0] })
            .expect("flow");
        let accum = zero_flow(4, 1).expect("accum");
        let (out, _) = datamosh_refresh_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            2,
            0.0,
            0.9,
            5.0,
        )
        .expect("refresh");
        // Reference advect (no refresh) for the busy block.
        let advected = datamosh_block_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0, 2)
            .expect("advected");
        // Calm block keeps ⇒ carrier.
        assert_eq!(out.pixel(0, 0), carrier.pixel(0, 0));
        assert_eq!(out.pixel(1, 0), carrier.pixel(1, 0));
        // Busy block rots ⇒ advected; and is distinct from the carrier.
        assert_eq!(out.pixel(2, 0), advected.pixel(2, 0));
        assert_eq!(out.pixel(3, 0), advected.pixel(3, 0));
        assert_ne!(out.pixel(2, 0), carrier.pixel(2, 0));
    }

    #[test]
    fn refresh_block_size_one_ignored() {
        let carrier = solid(4, 1, [1.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [0.0, x as f32 / 3.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |_, _| [0.0, 0.0]).expect("flow");
        let accum = zero_flow(4, 1).expect("accum");
        // block_size 1 ⇒ refresh is a no-op (the bloom path), even with a low
        // threshold that would otherwise refresh the zero-motion field.
        let (out, _) = datamosh_refresh_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            1,
            0.0,
            0.9,
            5.0,
        )
        .expect("refresh");
        let bloom =
            datamosh_bloom_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0).expect("bloom");
        assert_eq!(out, bloom);
    }

    #[test]
    fn refresh_resets_accumulator_in_refreshed_blocks() {
        let carrier = solid(4, 1, [1.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [0.0, x as f32 / 3.0, 0.0, 1.0])
            .expect("previous");
        // Block 0 mean [0,0] (calm, refreshes) but with non-zero intra-block detail
        // (±3) so its residual would be non-zero; block 1 mean [10,0] (busy, rots).
        let flow = FlowField::from_fn(4, 1, |x, _| match x {
            0 => [-3.0, 0.0],
            1 => [3.0, 0.0],
            2 => [8.0, 0.0],
            _ => [12.0, 0.0],
        })
        .expect("flow");
        let accum = zero_flow(4, 1).expect("accum");
        let (_out, new_accum) = datamosh_refresh_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            2,
            0.5,
            0.9,
            5.0,
        )
        .expect("refresh");
        // Calm refreshed block ⇒ accumulator cleared despite non-zero residual.
        assert_eq!(new_accum.vector(0, 0), Some([0.0, 0.0]));
        assert_eq!(new_accum.vector(1, 0), Some([0.0, 0.0]));
        // Busy rotting block ⇒ residual retained (= f − block_mean = ±2).
        assert_eq!(new_accum.vector(2, 0), Some([-2.0, 0.0]));
        assert_eq!(new_accum.vector(3, 0), Some([2.0, 0.0]));
    }

    #[test]
    fn residual_gain_zero_equals_block_frame() {
        let carrier = solid(4, 1, [0.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [x as f32 / 3.0, 0.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |x, _| [x as f32, 0.0]).expect("flow");
        // A deliberately non-zero prior accumulator must NOT leak into the output
        // when gain is 0 (the short-circuit ignores it).
        let accum = FlowField::from_fn(4, 1, |_, _| [5.0, -5.0]).expect("accum");
        let (out, new_accum) = datamosh_residual_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            2,
            0.0,
            0.9,
        )
        .expect("residual");
        let block = datamosh_block_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0, 2)
            .expect("block");
        assert_eq!(out, block);
        // The returned accumulator is zeroed at gain 0.
        for x in 0..4 {
            assert_eq!(new_accum.vector(x, 0), Some([0.0, 0.0]));
        }
    }

    #[test]
    fn residual_block_size_one_equals_bloom() {
        let carrier = solid(4, 1, [0.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [x as f32 / 3.0, 0.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let accum = zero_flow(4, 1).expect("accum");
        // block_size 1 ⇒ resid = 0 ⇒ accum stays zero ⇒ exactly the bloom warp,
        // regardless of a positive gain.
        let (out, new_accum) = datamosh_residual_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            1,
            0.75,
            0.9,
        )
        .expect("residual");
        let bloom =
            datamosh_bloom_frame_cpu(&carrier, Some(&previous), &flow, false, 1.0).expect("bloom");
        assert_eq!(out, bloom);
        for x in 0..4 {
            assert_eq!(new_accum.vector(x, 0), Some([0.0, 0.0]));
        }
    }

    #[test]
    fn residual_gain_one_first_p_frame_equals_raw_flow_displace() {
        let carrier = solid(4, 1, [0.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [x as f32 / 3.0, 0.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let accum = zero_flow(4, 1).expect("accum");
        // gain 1, first P-frame (accum zero): effective = q + (f − q) = f exactly,
        // so the output equals displacing by A's RAW flow (the smooth bloom warp).
        let (out, new_accum) = datamosh_residual_frame_cpu(
            &carrier,
            Some(&previous),
            &accum,
            &flow,
            false,
            1.0,
            2,
            1.0,
            0.9,
        )
        .expect("residual");
        let raw = flow_displace_cpu(&previous, &flow, 1.0).expect("raw");
        assert_eq!(out, raw);
        // And the accumulator now holds exactly the discarded residual f − q.
        let quantized = quantize_flow_to_blocks(&flow, 2).expect("quantized");
        for x in 0..4 {
            let f = flow.vector(x, 0).unwrap();
            let q = quantized.vector(x, 0).unwrap();
            assert_eq!(new_accum.vector(x, 0), Some([f[0] - q[0], f[1] - q[1]]));
        }
    }

    #[test]
    fn residual_accumulates_with_decay_across_p_frames() {
        let carrier = solid(4, 1, [0.0, 0.0, 0.0, 1.0]);
        let previous = ImageBufferF32::from_fn(4, 1, |x, _| [x as f32 / 3.0, 0.0, 0.0, 1.0])
            .expect("previous");
        let flow = FlowField::from_fn(4, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let quantized = quantize_flow_to_blocks(&flow, 2).expect("quantized");
        let decay = 0.5f32;

        // Frame 1: accum starts zero ⇒ accum1 = f − q.
        let accum0 = zero_flow(4, 1).expect("accum0");
        let (_out1, accum1) = datamosh_residual_frame_cpu(
            &carrier,
            Some(&previous),
            &accum0,
            &flow,
            false,
            1.0,
            2,
            0.5,
            decay,
        )
        .expect("frame1");
        // Frame 2: same flow ⇒ accum2 = accum1·decay + (f − q).
        let (_out2, accum2) = datamosh_residual_frame_cpu(
            &carrier,
            Some(&previous),
            &accum1,
            &flow,
            false,
            1.0,
            2,
            0.5,
            decay,
        )
        .expect("frame2");
        for x in 0..4 {
            let f = flow.vector(x, 0).unwrap();
            let q = quantized.vector(x, 0).unwrap();
            let r0 = f[0] - q[0];
            let r1 = f[1] - q[1];
            let expected = [r0 * decay + r0, r1 * decay + r1];
            assert_eq!(accum2.vector(x, 0), Some(expected));
        }
    }

    #[test]
    fn residual_keyframe_and_frame_zero_reset_accumulator() {
        let carrier = solid(2, 2, [0.25, 0.5, 0.75, 1.0]);
        let previous = solid(2, 2, [0.9, 0.8, 0.7, 1.0]);
        let flow = FlowField::from_fn(2, 2, |_, _| [3.0, -2.0]).expect("flow");
        let dirty = FlowField::from_fn(2, 2, |_, _| [9.0, 9.0]).expect("dirty");

        // Frame zero (no previous output): carrier verbatim, accumulator cleared.
        let (zero_out, zero_accum) =
            datamosh_residual_frame_cpu(&carrier, None, &dirty, &flow, true, 1.0, 16, 0.5, 0.9)
                .expect("zero");
        assert_eq!(zero_out, carrier);
        for y in 0..2 {
            for x in 0..2 {
                assert_eq!(zero_accum.vector(x, y), Some([0.0, 0.0]));
            }
        }

        // Keyframe refresh ignores held state + flow and clears the accumulator.
        let (key_out, key_accum) = datamosh_residual_frame_cpu(
            &carrier,
            Some(&previous),
            &dirty,
            &flow,
            true,
            1.0,
            16,
            0.5,
            0.9,
        )
        .expect("keyframe");
        assert_eq!(key_out, carrier);
        for y in 0..2 {
            for x in 0..2 {
                assert_eq!(key_accum.vector(x, y), Some([0.0, 0.0]));
            }
        }
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
        let zero = datamosh_block_frame_cpu(&carrier, None, &flow, true, 1.0, 16).expect("zero");
        assert_eq!(zero, carrier);
        // Keyframe refresh ignores held state + flow.
        let keyframe = datamosh_block_frame_cpu(&carrier, Some(&previous), &flow, true, 1.0, 16)
            .expect("keyframe");
        assert_eq!(keyframe, carrier);
    }

    #[test]
    fn remix_none_is_identical_to_block_quantize() {
        let flow = FlowField::from_fn(4, 4, |x, y| [x as f32, y as f32]).expect("flow");
        let quantized = quantize_flow_to_blocks(&flow, 2).expect("quantize");
        let remixed = remix_block_vectors(&flow, 2, VectorRemixMode::None, 0).expect("remix");
        assert_eq!(remixed, quantized);
    }

    #[test]
    fn remix_is_no_op_without_macroblocks() {
        // block_size <= 1 ⇒ the block grid is per-pixel ⇒ remix returns the flow
        // unchanged regardless of mode (the bloom path).
        let flow = FlowField::from_fn(3, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let remixed = remix_block_vectors(&flow, 1, VectorRemixMode::Shuffle, 7).expect("remix");
        assert_eq!(remixed, flow);
    }

    #[test]
    fn remix_sort_pools_strongest_motion_into_the_first_block() {
        // Four 1x1 blocks (block_size 1 would be a no-op, so use a 4x1 flow with
        // block_size 1? no — need >=2). Use a 4x1 flow, block_size 2 ⇒ two blocks:
        // block 0 = mean of x∈{0,1} = 0.5, block 1 = mean of x∈{2,3} = 2.5.
        let flow = FlowField::from_fn(4, 1, |x, _| [x as f32, 0.0]).expect("flow");
        let sorted = remix_block_vectors(&flow, 2, VectorRemixMode::Sort, 0).expect("sort");
        // Descending magnitude ⇒ the stronger block (2.5) is reassigned to block 0
        // (x∈{0,1}), the weaker (0.5) to block 1 (x∈{2,3}).
        assert_eq!(sorted.vector(0, 0).unwrap(), [2.5, 0.0]);
        assert_eq!(sorted.vector(1, 0).unwrap(), [2.5, 0.0]);
        assert_eq!(sorted.vector(2, 0).unwrap(), [0.5, 0.0]);
        assert_eq!(sorted.vector(3, 0).unwrap(), [0.5, 0.0]);
    }

    #[test]
    fn remix_shuffle_is_a_deterministic_permutation_of_block_mvs() {
        let flow = FlowField::from_fn(8, 1, |x, _| [x as f32, 0.0]).expect("flow");
        // block_size 2 ⇒ 4 blocks with means 0.5, 2.5, 4.5, 6.5.
        let a = remix_block_vectors(&flow, 2, VectorRemixMode::Shuffle, 42).expect("a");
        let b = remix_block_vectors(&flow, 2, VectorRemixMode::Shuffle, 42).expect("b");
        // Same seed ⇒ byte-identical (deterministic).
        assert_eq!(a, b);
        // A permutation reuses exactly the original block MVs (multiset preserved).
        let block_mv = |field: &FlowField, block: u32| field.vector(block * 2, 0).unwrap()[0];
        let mut got: Vec<f32> = (0..4).map(|blk| block_mv(&a, blk)).collect();
        got.sort_by(|x, y| x.total_cmp(y));
        assert_eq!(got, vec![0.5, 2.5, 4.5, 6.5]);
        // A different seed yields a different assignment (with 4! permutations the
        // chance of collision is low; these two seeds differ in practice).
        let c = remix_block_vectors(&flow, 2, VectorRemixMode::Shuffle, 7).expect("c");
        assert_ne!(a, c);
    }
}
