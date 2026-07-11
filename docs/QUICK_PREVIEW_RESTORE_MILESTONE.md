# Quick Preview + Performance Capture — Restore

## Problem

The sidebar+detail redesign (`docs/UI_REDESIGN_MILESTONE.md`, merged) deleted
`WorkflowPanelView.swift` in full, including its "Preview" band: a looping,
downscaled preview render of the selected effect (`PreviewPlayerModel`) with
a performance-capture strip layered on top (`GestureRecorder`) for recording
a `[0,1]` gesture onto an armed Rutt-Etra modulation slot
(`docs/PERFORMANCE_CAPTURE_MILESTONE.md`). It was correctly left out rather
than guessed at, because it was hardwired to the old 8-case `WorkflowEffect`
enum that no longer exists. This milestone rebuilds it generically across
the new per-effect detail-view structure.

**Nothing about the underlying mechanism is missing or broken** — this is a
pure view-layer gap, same class of change as the redesign itself:

- `Models/PreviewPlayerModel.swift` (looping playback, frame-index math) —
  untouched, has its own passing tests.
- `Models/GestureRecorder.swift` (knot recording/decimation) — untouched,
  has its own passing tests.
- `AppState.swift`'s preview/capture plumbing — untouched, all present:
  `beginEffectPreview(requiresModulator:) -> Bool`, `previewFrames`,
  `previewScale`, `previewSeconds`, `previewPlaybackFps`, `isRenderingPreview`,
  `previewSummary`, `isExtractingProxies`, `ruttEtraArmedCaptureTargets`,
  `captureTargetSelection`, `captureSlider`, `isCapturing`,
  `beginCaptureTake(loopDuration:)`, `ingestCaptureSample(t:v:)`,
  `endCaptureTake()`, `ruttEtraCapturedTakes`.

## Non-negotiable constraint: VIEW-ONLY, same as the redesign

Do not touch `AppState.swift`, `PreviewPlayerModel.swift`, or
`GestureRecorder.swift`. Every property/function above already works exactly
as it did in the old `WorkflowPanelView` — the job is to re-assemble them
into a new shared view component and wire it into the right effect detail
views, not to change what they do.

## What to build: `QuickPreviewBand`

New file `Views/Effects/QuickPreviewBand.swift`, a shared component every
eligible effect's detail view instantiates at the bottom of its body (after
the Run button and summary `Text`, matching the existing per-effect layout
convention visible in `Views/Effects/DisplacementEffectViews.swift` etc.).

```swift
struct QuickPreviewBand: View {
  @ObservedObject var state: AppState
  let requiresModulator: Bool
  let runEffect: () -> Void
  @StateObject private var previewPlayer = PreviewPlayerModel()
  ...
}
```

Port the following from the old `WorkflowPanelView.previewBand` +
`previewBand`'s embedded capture strip (`/tmp/old-workflow-panel.swift` in
git history at commit `c8f6cb8` if you need the literal source —
`git show c8f6cb8:apps/macos/Sources/MorphogenMacApp/Views/WorkflowPanelView.swift`
— the `previewBand` property, lines ~823-975 of that historical file, plus
`captureTakeLabel`, lines ~977-988) with **one behavior change**, described
next.

### Behavior change: reset on appear (new, not in the old code)

The old `previewBand` lived inside one persistent `WorkflowPanelView` that
never went away, so `state.previewFrames` always belonged to whatever
`selectedEffect` was current. In the new structure, switching the sidebar
selection destroys and recreates a *different* per-effect view — but
`previewFrames` lives on shared `AppState`, so without a reset, switching
from effect X (with a rendered preview) to effect Y would show X's stale
frames labeled as Y's. Add:

```swift
.onAppear {
  state.previewFrames = []
  previewPlayer.stop()
}
```

This makes every effect's Quick Preview start empty when you navigate to
it — press "Quick Preview" again to render for the effect you're now
viewing. Simple, correct, and avoids a misleading stale frame.

### Everything else: port as-is

- Scale (`Full/1/2/1/4/1/8` → `state.previewScale`) and seconds
  (`state.previewSeconds`) controls.
- "Quick Preview" button: `state.beginEffectPreview(requiresModulator:)` →
  if `true`, call `runEffect()`. Disabled while
  `state.isRenderingPreview || state.isExtractingProxies`.
- Empty state hint text; non-empty state shows the current frame
  (`state.previewFrames[previewPlayer.currentIndex]`), play/pause via
  `previewPlayer.togglePlayPause()`, frame counter, horizontal filmstrip.
- `.onChange(of: state.previewFrames.count)` starts/stops `previewPlayer`
  exactly as before (fps = `state.previewPlaybackFps`, captured at preview
  start so a later proxy-fps change doesn't retroactively shift playback).
- The performance-capture strip: visible iff
  `!state.ruttEtraArmedCaptureTargets.isEmpty` — **this is intentionally not
  restricted to the Rutt-Etra effect's own detail view**. It's about
  whichever Rutt-Etra modulation slot(s) the user has armed (`source ==
  .captured`) elsewhere; you can record a take while quick-previewing *any*
  effect, since the capture slider scrubs against whatever loop is currently
  playing. Preserve this exactly — do not gate it on `selection ==
  .ruttEtra`.
- Record/stop button, capture-target picker, capture slider wired to
  `state.ingestCaptureSample(t: previewPlayer.elapsed(), v:)`, and the take
  label (port `captureTakeLabel` as a private computed property on
  `QuickPreviewBand`).

## Eligibility: which effects get a `QuickPreviewBand`

Quick Preview works by having `beginEffectPreview` write a downscaled,
frame-capped copy of the source proxies and override
`frameSequenceModulatorURL`/`frameSequenceCarrierURL` for the duration of
one render. That override is only visible to code that reads sources via
`effectiveModulatorURL()` / `effectiveCarrierURL()`. Effects whose
`run*Render()` reads a **dedicated, independent** pair of `@Published var
xxxModulatorURL: URL?` / `xxxCarrierURL: URL?` properties instead (their own
local Source A/B pickers, unrelated to the shared proxy pipeline) cannot be
reached by that override at all — attaching a "Quick Preview" button to them
would silently run the **full, uncapped** operation while claiming to be a
fast low-res preview, which is worse than having no button. This was
verified directly against every `run*Render()` guard clause in
`AppState.swift` — trust this table, no need to re-derive it:

**Include — static `requiresModulator`:**

| Effect | `requiresModulator` |
|---|---|
| Flow Displace | `true` |
| Flow Feedback | `true` |
| Coagulated Flow Blend | `true` |
| Dispersion Blend | `true` |
| Fluid Mosaic | `true` |
| Granular Mosaic | `true` |
| Controlled Datamosh | `true` (its own `datamoshModulatorURL` falls back to `effectiveModulatorURL()` when unset, so the preview override still reaches it in the common case) |
| Cascade Collage | `false` |
| Trail Cascade | `false` |
| Morphogenesis | `false` |
| Retro Static | `false` |
| Palette Quantize | `false` |

**Include — dynamic `requiresModulator` (depends on the effect's own
mode/toggle state, already present in its detail view):**

| Effect | Expression |
|---|---|
| Rutt-Etra | `state.ruttEtraUseTwoSource` |
| Channel Shift | `state.channelShiftFlowGain != 0` |
| Fluid Advection | `mode == .twoSource` (the view's own local `@State private var mode: FluidAdvectionMode`); `runEffect` must mirror the view's own mode switch (`FluidAdvectionEffectViews.swift` lines ~257-266): `.twoSource` → `runTwoSourceFluidAdvectSequenceRender()`, `.selfFlow` → `runOpticalFlowAdvectSequenceRender()`, `.procedural` → `runProceduralFluidAdvectSequenceRender()`, `.particles` → `runFieldParticlesSequenceRender()` |

**Exclude — do not add `QuickPreviewBand` (confirmed dedicated-property or
non-frame-sequence effects):**

- Bitstream Datamosh — real AVI bitstream/codec surgery on raw video files
  (`sourceAURL`/`sourceBURL` fallback, not frame directories); doesn't
  participate in the proxy pipeline at all.
- Conv-Blend, Spectral Cross-Synthesis, Audio Impulse Convolution,
  Audio-to-Video Route, Video-to-Audio Route, Pixel Sort — each reads its
  own dedicated `xxxModulatorURL`/`xxxCarrierURL` pair
  (`convBlendModulatorURL`, `crossSynthModulatorURL`, `impulseConvModulatorURL`,
  `audioRouteModulatorURL`, `videoAudioRouteModulatorURL`,
  `pixelSortModulatorURL`, and their `...CarrierURL` counterparts) —
  confirmed by direct read, zero calls to `effectiveModulatorURL()` /
  `effectiveCarrierURL()` in any of these six functions.
- Composition Timeline — different paradigm (scene-chain spec file
  execution via `runComposition`, not a single-effect render).
- Analysis, Node Graph — static panels, no render function.

If you find a discrepancy against this table while implementing (e.g. a
guard clause reads differently than described), stop and re-verify against
the actual `AppState.swift` source rather than guessing — the eligibility
rule is mechanical (grep the function body for `effectiveModulatorURL()` /
`effectiveCarrierURL()`), not a judgment call.

## Placement

Add `QuickPreviewBand(state: state, requiresModulator: ..., runEffect: {
... })` as the last element in each eligible effect's `VStack`, after its
existing Run button + summary `Text`. It should look identical across every
eligible effect (same component, different closure) — this consistency is
part of the point of the redesign.

## Acceptance criteria

- `swift build` and `swift test` clean, 158/158 unchanged (this is a
  view-layer addition — it must not change any existing test's outcome).
- Every effect in the "Include" tables above has a working `QuickPreviewBand`
  at the bottom of its detail view; every effect in "Exclude" does not.
- The capture strip logic (visibility, recording, target selection) is
  identical in behavior to the old `WorkflowPanelView`'s, just relocated.
- `AppState.swift`, `PreviewPlayerModel.swift`, `GestureRecorder.swift`
  unmodified (`git diff --stat` should show zero changes to these three
  files).
- Checkpoint (local commit, no push) per the project's standing
  `/checkpoint` convention.
