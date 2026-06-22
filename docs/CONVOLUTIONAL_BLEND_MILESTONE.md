# Convolutional Audio/Video Blending Milestone (Image Kernel)

## Goal

The roadmap's **"tiny direct convolution for audio or image kernels"** MVP, built
for the **image** carrier (`docs/EFFECTS_ROADMAP.md` — *Convolutional Audio/Video
Blending*). Source A supplies a small **K×K image kernel** derived from its frame;
Source B's frame is **spatially convolved** with that kernel, so B takes on the
local structure of A (a structure-aware blur / spatial blend). This is the A→B
cross-modal complement to the descriptor-routing effects: there a *scalar*
descriptor modulated B; here a *spatial* kernel does.

The new deterministic logic is two pieces: **kernel extraction** (A's frame → a
normalized K×K weight grid) and a **direct 2D convolution** of B with that kernel.
Both are simple float math, so a parity-gated Metal path follows the same shape as
the video-vocoder kernel (the HQ-tier "Metal spatial kernels").

## The transform (the new logic)

1. **Kernel extraction (Source A).** Box-downsample A's per-pixel Rec.709
   **luminance** into a `K×K` grid (each cell = mean luma over its region of A),
   then **normalize so the weights sum to 1**. A's bright regions become heavy
   taps, its dark regions light ones — the blur takes on A's coarse layout. A
   fully black A (sum ≈ 0) falls back to a **uniform** kernel (`1/K²` each) so the
   result is always a well-defined weighted average. `K` is odd and ≥ 1.
2. **Convolution (Source B).** For each output pixel and each RGB channel,
   accumulate `Σ weights[k] · B[clamp(x+dx), clamp(y+dy)]` over the centered K×K
   window (`r = (K-1)/2`), **clamped border** sampling. Alpha is preserved. Kernel
   taps are applied without flip (correlation-style, the image-processing
   convention for symmetric blur kernels); CPU and Metal apply the identical order
   so parity holds.
3. **Blend.** `out = lerp(B, convolved, amount)`, clamped `[0,1]`. `--amount`
   (default `1.0`) is the global wet/dry; `amount = 0` ⇒ `out = B` exactly.

- `amount = 0` ⇒ output **byte-identical** to Source B (lerp at `t=0` returns the
  carrier untouched). `K = 1` ⇒ identity kernel (single unit tap) ⇒ passthrough at
  any amount.
- Determinism: identical A, B, `K`, `amount` ⇒ identical output frames; CPU and
  Metal byte-identical within `METAL_CPU_PARITY_EPSILON`.

## Initial Scope

- CPU reference in `morphogen-render` (`conv_blend.rs`): `ConvolutionKernel` +
  `analyze_convolution_kernel_cpu` (A → normalized K×K), `ConvolutionBlendSettings`
  (kernel_size, amount) with `validate`, and `convolution_blend_cpu`
  (carrier × kernel × amount → image) with focused synthetic tests.
- Parity-gated Metal kernel `convolution_blend` (carrier texture + weights buffer
  + params), `convolution_blend_metal`, shader-source validation, runtime parity
  test — mirrors the video-vocoder Metal slice.
- `render-convolutional-blend-sequence` CLI: `--modulator-dir` (A PNG seq),
  `--carrier-dir` (B PNG seq), `--output-dir`, `--kernel-size` (odd, default 3),
  `--amount` (default 1.0; 0 = passthrough), `--backend cpu|metal` (parity-gated),
  `--max-frames`. Per frame: extract the kernel from A[i], convolve B[i]. Output
  follows Source B; frame count is the common prefix with the cap.
- Persisted `frame_sequence_convolution_blend` queue task + macOS Render-panel
  section follow once the CPU + CLI + Metal slice is proven (a `frame_sequence`
  video job, like the vocoder / audio-route).

## Algorithm Identifier

- `image_kernel_convolution_blend_cpu_v1` — recorded on the job/manifest. Names
  the A-luma→K×K kernel extraction + direct-convolution blend policy.

No reusable analysis sidecar: the kernel is cheap to recompute per frame.

## Acceptance Criteria

1. **Passthrough identity.** `--amount 0` (or `K = 1`) ⇒ output byte-identical to
   Source B.
2. **Convolution transfer.** A structured A over a high-frequency B ⇒ the output is
   visibly blended/blurred by A's kernel shape; a uniform A ⇒ a plain box blur.
3. **Determinism.** Identical inputs + settings ⇒ identical output frames.
4. **CPU/Metal parity.** `--backend metal` byte-identical to CPU within
   `METAL_CPU_PARITY_EPSILON`, gated frame-by-frame before export.
5. **No `unwrap()` in library code**; errors via `RenderError`/`thiserror`.

## Verification (off-vs-on)

Convolution changes a frame **spatially**, not temporally, so a blur on a static
carrier is invisible to `scripts/frame-delta.py` (a *within-sequence*
consecutive-frame metric — identical input frames stay identical after the same
blur). The honest readout is a **cross-sequence OFF-vs-ON per-frame difference**:
render the same job off (`--amount 0`) vs on (`--amount 1`, K≥3) on a
high-frequency carrier (where a blur visibly changes pixels), Read a frame from
each, and report the mean per-pixel difference between the OFF and ON frame —
off→on ⇒ a nonzero diff that grows with the kernel's spread; `--amount 0` ⇒ 0.

## Deferred (not this slice)

- **Audio impulse convolution** — Source A impulse response × Source B audio
  (convolution-reverb-style), the other half of the roadmap MVP. CPU-only.
- **FFT convolution** (the roadmap HQ tier) — large kernels via frequency-domain
  multiply, vs this direct spatial loop.
- **Per-channel / color kernels**, separable kernels, and Source-A *color*
  (not luma) taps. This MVP routes one luma-derived kernel applied to all channels.
- Queue + SwiftUI exposure land after the CPU + CLI + Metal slice is verified.
