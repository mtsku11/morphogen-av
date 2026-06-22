# Audio-to-Video Descriptor Routing Milestone

## Goal

The cross-modal complement to Spectral Audio Cross-Synthesis: there Source A's
audio shaped Source B's **audio**; here Source A's audio shapes Source B's
**video**. Source B's frames stay the material you see; **Source A's RMS
envelope decides how much each frame is displaced** over time. This is the
roadmap's **"RMS controls displacement amount"** MVP for Audio-to-Video
Descriptor Routing (`docs/EFFECTS_ROADMAP.md`).

The pixel transform is the existing, already-parity-gated **flow displace**
(`flow_displace_cpu` / `flow_displace_metal`): each output pixel samples the
carrier at `p + amount · field(p)`. The only new deterministic logic is the
**routing** — turning A's RMS envelope into the per-frame scalar `amount` — so a
Metal path comes essentially for free by feeding that scalar into the existing
displace kernel (parity-gated frame-by-frame, like every other GPU path).

## Routing (the new logic)

1. **Modulator envelope (Source A).** Compute A's RMS envelope (`rms_envelope`,
   `--rms-window` / `--rms-hop`) and **peak-normalize**:
   `a_norm[k] = rms[k] / max(rms)` ∈ `[0,1]` (all zeros if the peak is ~0). A's
   loudest moment maps to full displacement, its silence to none. Same
   peak-normalize + hold-last convention as the cross-synth `gain` mode and the
   granular audio→control routing, so A and B stay independent in rate/length.
2. **Per-frame amount.** For output frame `i` at time `t = i / fps`, hold-last
   `a_norm` at `t` (latest A sample at or before `t`), then
   `amount = base_amount · a_norm`. `base_amount` (`--amount`, default `1.0`)
   is the global scale; `base_amount = 0` ⇒ `amount = 0` for every frame.
3. **Displacement field.** A fixed, procedural **uniform** field:
   `field(x,y) = [shift_x, shift_y]` for all pixels (`--shift-x` default
   `8.0` px, `--shift-y` default `0.0`). The field is the *direction and unit
   magnitude*; `amount` is the *loudness-driven scale*. (Spatially varying
   fields — sine warp, radial — are deferred; the uniform field is the simplest
   falsifiable readout of "amount".)
4. **Apply (per frame, per source frame B[i]).**
   `out[i] = flow_displace(B[i], field, amount)` — backward-sampling, bilinear,
   clamped at the border (the existing displace semantics).

- `amount = 0` (silence, or `--amount 0`) ⇒ each pixel samples `p + 0 = p` ⇒
  output **byte-identical** to Source B.
- Determinism: identical A, B, window/hop, shift, `base_amount`, fps ⇒ identical
  output frames; CPU and Metal byte-identical within `METAL_CPU_PARITY_EPSILON`
  (inherited from the displace kernel's existing parity gate).

## Initial Scope

- CPU reference in `morphogen-render` (`audio_route.rs`): the peak-normalized
  envelope → per-frame `amount` mapping, the uniform field builder, and the
  named frame entry (`rms_displacement_route_frame_cpu`, delegating to
  `flow_displace_cpu`) with focused synthetic tests. No new pixel math — the
  transform is the proven displace.
- `render-audio-video-route-sequence` CLI: `--modulator-wav` (A),
  `--carrier-dir` (B PNG sequence), `--output-dir` (out PNG sequence),
  `--amount` (base, default 1.0; `0` = passthrough), `--shift-x` / `--shift-y`
  (field, default `8.0` / `0.0`), `--rms-window` / `--rms-hop`, `--fps`
  (frame→time mapping for the envelope lookup), `--backend cpu|metal`
  (parity-gated), `--max-frames`. Reuses the `render-video-vocoder-sequence`
  frame-loop idiom.
- Output is a PNG frame sequence following Source B (dimensions, frame count =
  common prefix with the cap).
- Queue task + macOS Render-panel exposure follow once the CPU + CLI + Metal
  slice is proven (a `frame_sequence`-style video job, like the vocoder).

## Algorithm Identifier

- `rms_displacement_route_cpu_v1` — the routing algorithm id recorded on the
  job/manifest. (The underlying pixel op is the existing `flow_displace`; this id
  names the RMS-envelope→amount routing policy.)

No new reusable analysis sidecar: A's RMS is cheap and the existing `cache-rms`
sidecar already covers reuse if wanted later.

## Acceptance Criteria

1. **Passthrough identity.** `--amount 0` (or silent A) ⇒ output byte-identical
   to Source B.
2. **Envelope transfer.** A loud→silent ramp over a static B ⇒ the per-frame
   displacement tracks A (large where A is loud, none where A is silent); a flat
   A ⇒ uniform displacement on every frame.
3. **Determinism.** Identical inputs + settings ⇒ identical output frames.
4. **CPU/Metal parity.** `--backend metal` byte-identical to CPU within
   `METAL_CPU_PARITY_EPSILON`, gated frame-by-frame before export.
5. **No `unwrap()` in library code**; errors via `RenderError`/`thiserror`.

## Verification (off-vs-on)

Render the same job **off** (`--amount 0`, or a silent modulator) vs **on**
(`--amount 1` with a loud modulator) on a readout fixture, Read frames from
both, and report the `scripts/frame-delta.py` number — off ⇒ ~0 delta
(passthrough), on ⇒ a nonzero displacement delta that rises and falls with A's
loudness. A look without a number is unfalsifiable; a number without the pixels
proves nothing.

## Deferred (not this slice)

- **Spatially varying displacement fields** — sine/wave warp, radial, or a
  Source-A-derived flow field (the convergence with Optical-Flow Advection).
- **Other descriptor targets** — centroid→hue/brightness, onset→flash/cut,
  RMS→zoom/scanline. This MVP routes one descriptor (RMS) to one target
  (displacement amount).
- **Sample-accurate descriptor curves** (the roadmap HQ tier): per-sample /
  smoothed / look-ahead envelopes routed into render nodes, vs the per-frame
  hold-last here.
- Queue + SwiftUI exposure land after the CPU + CLI + Metal slice is verified.
