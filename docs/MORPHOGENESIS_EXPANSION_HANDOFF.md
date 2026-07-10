# Morphogenesis Expansion — more models + the 3D relief look (Sonnet handoff)

Status: **PLANNED — handoff for Sonnet slice-builds under an Opus
orchestrator.** Written 2026-07-10 at the user's request: *"expand the
morphogenesis feature and build more models like the reaction-diffusion one —
either more RD models or another generative model. I'm also interested in
making this look more 3D."*

Builds on three complete milestones — read their DONE entries first:
[MORPHOGENESIS_MILESTONE.md](MORPHOGENESIS_MILESTONE.md) (Gray-Scott S1–S4),
[MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md](MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md)
(inject/erode/homeostat L-S1–L-S3),
[MORPHOGENESIS_FIELD_VIEW_MILESTONE.md](MORPHOGENESIS_FIELD_VIEW_MILESTONE.md)
(`--output-view field`). Baselines at handoff time: **cargo 733/0, swift
152/0**, clippy clean, `cargo fmt --check` dirty on 8 pre-existing files
(zero new diffs allowed). All landed through `a7d9c8e` on `origin/main`.

## Orchestration ground rules (learned this arc, non-negotiable)

- One Sonnet agent per slice; the orchestrator writes nothing on faith —
  re-runs suites, re-runs the readout numbers, **Reads the frames**, then
  `/checkpoint`s. Baseline → delta reported per slice.
- **Agents die on account session limits** mid-run, usually with 60–95% on
  disk. The orchestrator finishes inline; keep slices small enough that
  finishing inline is cheap. SourceKit diagnostics on fresh cross-module
  symbols are chronically stale — trust `swift build`/`swift test`.
- `main()` runs the CLI on a 64 MiB worker thread (clap debug-assert stack
  overflow at this subcommand count). Never remove it; new flags are safe.
- CPU first; **Metal stays deferred** for all stateful field models (the
  datamosh finding: per-frame parity passes while bytes diverge across
  substeps — don't "fix").
- Presets, not raw parameter numbers, for every model (most of every model's
  parameter space is dead — the #1 look trap, three times confirmed).
- Every readout on **real footage** (the cello fixture recipe below), never
  only synthetics: the S3 param-map trap (opposite-sign segment passed every
  synthetic test, silently killed the field on mostly-dark real footage) and
  the L4 flat-carrier degeneracy both only surfaced on real/textured media.

**Standard fixture recipe:** `ffmpeg -ss 2.0 -i cello.mp4 -vf fps=24
-frames:v 144 <dir>/frame_%06d.png` (640×360; the clips are gitignored,
present in the repo root) + `ffmpeg -ss 2.0 -t 6 -i cello.mp4 -ac 1
cello.wav` for audio routes. Freeze metric: `frame-delta.py` over frames
0–48 vs 95–143 (copy to temp dirs). Recommended live-coupling knobs today:
`--inject 0.1 --erode 0.03 --inject-source motion`.

---

## Track B first — the 3D relief look (build this before new models)

**Why first:** one small slice, transforms every EXISTING output, and every
new model inherits it. The field is a height map already; we're just not
lighting it.

### B1 — gradient-lit relief shading (`--shade`)

Treat V as a height field and light it. All per-pixel, deterministic, no 3D
engine:

```
n  = normalize(-dV/dx * height, -dV/dy * height, 1)      // surface normal
l  = (cos(el)·cos(az), cos(el)·sin(az), sin(el))          // light direction
diffuse  = max(0, n·l)
specular = max(0, reflect(-l, n)·(0,0,1))^shininess
lit = ambient + (1-ambient)·diffuse + spec_strength·specular
```

- ∇V already exists (the displace pass) — reuse it at sim res, upsample the
  *lit* result (or upsample V then shade at carrier res; pick one, declare,
  pin — shading after upsample is smoother and is the recommendation).
- Applies in **both output views**: field view → the B/W becomes an embossed
  membrane (the user's "more 3D" on the look they love); composite view →
  `lit` multiplies/adds onto the pattern layer so growths read as raised
  tissue on the footage.
- **Knobs:** `--shade <0..1>` (blend, 0 = off, the continuity anchor),
  `--shade-height <f32>` (gradient→normal scale), `--shade-azimuth <turns>`,
  `--shade-elevation <0..0.25 turns>`, `--shade-specular <0..1>`,
  `--shade-shininess` (pinned default fine). Defaults chosen empirically on
  the cello field render (probe precedent).
- **Mod targets:** `shade`, `shade_azimuth`, `shade_height` join the
  registry. `shade_azimuth = lfo(saw, 0.1)` = a light orbiting the pattern —
  an instant hero shot; make it the readout.
- **This also closes the dark-footage composite gap** noted in the showcase
  session: specular+diffuse ADD light, so growth finally reads on
  near-black footage (the luma-preserving tint couldn't). Assert it: the
  old cello composite that read as "just a hue change" must show visible
  relief structure on the dark stage with shade on.
- **Anchors:** shade-0 byte-identity; azimuth rotation changes bytes but a
  180° azimuth flip mirrors highlight/shadow sides (spot-assert); off-vs-on
  frame Read + delta; queue/SwiftUI ride-along (three knobs + slots, the
  established template).
- **Checkpoint:** shading is composite-side (does NOT touch field state) but
  changes output → joins the contract like pattern_mix did (changed shade
  refuses resume; legacy checkpoints default 0).
- Effort: small. One slice including queue/SwiftUI.

---

## Track A — new generative models

**Architecture decision (recommendation, orchestrator confirms with the user
if unsure):** extend the EXISTING commands with `--model
<gray-scott|fitzhugh-nagumo|lenia>` rather than new command families.
Everything downstream of the substep is model-agnostic and already built:
seeding-from-B, inject/erode weight fields, homeostat, param-map (per-model
mapping), composite, field view, shading (B1), checkpoint, queue, panel.
Each model = its own algorithm id (`morphogenesis_fhn_cpu_v1`,
`morphogenesis_lenia_cpu_v1`), its own preset list behind the existing
`--preset` flag (preset names imply the model's parameter set), its own
state layout in the RGBA32F checkpoint (channels documented per model).
Per CLAUDE.md "no broad abstractions": do NOT build a FieldModel trait for
the second model — copy the Gray-Scott shape for FHN, and only extract
shared pieces when Lenia (the third) makes the duplication real (rule of
three). Physarum is NOT a grid field — it gets its own command + contract.

### A1 — FitzHugh–Nagumo (excitable media: travelling waves, spirals)

The biggest *temporal* contrast to Gray-Scott: instead of patterns that grow
and settle, FHN is an excitable medium — injection fires **travelling pulse
waves** that propagate, curl into rotating spirals, and annihilate on
collision. With `inject = audio-rms`, every musical hit launches a wavefront
across the frame. This model never freezes by nature.

```
du/dt = Du·∇²u + u − u³/3 − v + I(x,y)     // fast activator (I = inject weight!)
dv/dt = ε·(u + a − b·v)                      // slow recovery
```

- u is signed (≈ [−2, 2]) — the [0,1] clamp is WRONG here; clamp to a
  declared [−3, 3] safety box instead and normalize for display
  (`(u+2)/4`). The live-coupling inject maps naturally onto the I current
  term rather than adding to u directly (declare; it's the physically right
  coupling and reuses the weight field unchanged).
- Presets to pin empirically (start from ε≈0.08, a≈0.7, b≈0.8, Du≈1.0 at
  dt≈0.1 with more substeps; TUNE — the atlas values are a starting point,
  not a contract): `pulse` (excitable, waves die out — pure music-reactive),
  `spiral` (self-sustaining rotors), `labyrinth` (Turing-ish FHN regime).
- Aliveness test per preset = wave speed: a point stimulus must propagate
  a front N cells in M frames (falsifiable, unlike variance — waves MOVE).
- Seed: B-luma threshold fires u (not v); speckle optional per preset.
- Readout: `inject=audio-rms` on the cello — every bow attack visibly
  launches a wave. Field view + shade = rippling metal. This is the
  flagship clip.
- Slices: A1-S1 core+CLI (+field view works free), A1-S2 mod-map +
  queue/SwiftUI ride-along.

### A2 — Lenia (continuous cellular automata: organic "creatures")

Grid-based, gather-only, deterministic — a perfect invariant citizen, and
the most alien look available: smooth gliding blobs, orbiting rings,
breathing membranes.

```
A(t+dt) = clamp01( A + dt · G( (K * A)(x) ) )
K = ring kernel (radius R, gaussian shell profile), normalized
G(u) = 2·exp(−(u−μ)²/(2σ²)) − 1                  // growth mapping
```

- Direct convolution at sim res (R ≈ 13 ⇒ 27×27 taps) is O(W·H·R²) —
  measure first at 320×180; if too slow, separable/FFT is an OPTIMIZATION
  slice, not the MVP (half-res sim exists for a reason).
- One channel (A in checkpoint R; prev-luma stays in B — collision-free).
  inject/erode apply to A directly (they're already the right shape).
- Presets: `orbium` (the classic glider, μ≈0.15 σ≈0.017 R=13 dt=0.1),
  `geminium`, plus a dense `soup` preset for full-frame texture. Pin
  empirically; Lenia creatures are notoriously parameter-fragile — the
  aliveness test is "total mass stays in a band AND the centroid moves"
  (gliders translate; death and explosion both fail it).
- B-coupling: seed from luma blobs; param map shifts μ locally (bright
  regions host different fauna — same segment discipline as S3: probe both
  endpoints alive on REAL footage).
- Readout: creatures swimming over the cellist, `inject=audio-rms` spawning
  new ones on the beats.
- Slices: A2-S1 core+CLI, A2-S2 coupling+queue/SwiftUI.

### A3 — Physarum / slime mold (agent transport networks) — own contract

The wilder sibling, explicitly deferred twice; scope it LAST and write its
own milestone doc before building (this handoff only pins the determinism
recipe so it doesn't get invented wrong):

- N agents (fixed count, e.g. 2²⁰) with (x, y, heading) in f32; per frame:
  **sense** (three trail samples ahead), **rotate** (fixed rule), **move**,
  **deposit** into the trail grid, then trail **diffuse+decay** (the grid
  half reuses the stencil machinery).
- **Determinism recipe:** agents live in one Vec, updated in index order;
  deposits are sequential accumulation in agent order (scatter, but
  FIXED-order scatter — no parallelism in the MVP, no atomics); all
  randomness is splitmix64 of (seed, agent index, frame). Checkpoint =
  agent buffer + trail grid (agent state does NOT fit RGBA32F — it needs a
  raw f32 sidecar file next to the state PNG; extend the checkpoint
  contract accordingly, declared).
- The trail grid is the displayable field → output-view/shade/composite all
  apply; B-coupling: deposit bonus on bright/moving footage (the weight
  field again); the look: glowing root networks crawling over the subject.

### Explicitly out of scope (all tracks)

Multi-species Gray-Scott ecologies; 3-D voxel simulation (the 3D ask is
satisfied by B1's relief shading — true volumetrics is a different app);
Metal ports; GPU Lenia FFT; RD-as-chain-stage (separate existing flag).

---

## Suggested build order

**B1 shading → A1 FHN → A2 Lenia → (A3 Physarum, own contract).**
B1 is one slice and upgrades everything retroactively; FHN delivers the
never-freezes music-reactive look with the least new machinery; Lenia adds
the alien-life look; Physarum is gated behind its own contract. After B1
lands, every A-model readout should include a `--shade` variant — the
orchestrator delivers each model's 6 s audio-muxed clip (field view + shade
is the expected hero) via SendUserFile.

## Acceptance template (every slice)

1. Continuity/identity anchors byte-pinned (off = today's bytes).
2. Model aliveness falsifiable per preset (variance band / wave speed /
   mass+centroid — as specified per model).
3. Checkpoint: interrupt+resume byte-identical; changed knob/model/preset
   refuses; legacy checkpoints unaffected.
4. Queue add→run byte-identical; SwiftUI token tests; no-op arg arrays
   byte-identical.
5. Readout on the cello fixture with numbers (early/late windows where
   freeze is a risk) AND frames Read; deltas may be non-monotonic — pair
   numbers with pixels.
6. No `unwrap()`; clippy clean; zero new fmt diffs; baseline → delta
   reported.
