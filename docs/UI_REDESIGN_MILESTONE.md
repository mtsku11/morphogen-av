# SwiftUI Shell Redesign — Sidebar + Detail

## Problem

`ContentView` today is one long `VStack` in a single `ScrollView`:

1. Source A / Source B slots (`SourceSlotView`).
2. `WorkflowPanelView` (1,388 lines) — a curated "quick" surface for 8
   effects (Flow Displace, Flow Feedback, Fluid Advection, Granular Mosaic,
   Datamosh, Bitstream Datamosh, Video Vocoder, Trail Cascade).
3. One `DisclosureGroup` labeled "Advanced render queue, diagnostics, and
   experimental controls" wrapping `NodeGraphPlaceholderView`,
   `AnalysisPanelView`, `CompositionPanelView`, `CoagulatedBlendPanelView`,
   `DispersionBlendPanelView`, `FluidMosaicPanelView`, and `RenderPanelView`
   (3,025 lines) — the full knob-by-knob controls for those same 8 effects
   *plus* ~14 more effects that have no "quick" surface at all (Rutt-Etra,
   Morphogenesis, Cascade Collage, Retro Static, Channel Shift, Palette
   Quantize, Cross-Synth, Audio Impulse Convolution, Audio-Video Route,
   Video-Audio Route, Conv-Blend, Pixel Sort).

The Workflow/Advanced split is not a real category boundary — it's an
accident of build order. `WorkflowPanelView`'s "quick" controls for e.g.
Datamosh bind to the exact same `@Published` properties on `AppState`
(`state.datamoshPreset`, `state.bitstreamOperation`, ...) as
`RenderPanelView`'s full section for the same effect — it's the same effect
rendered twice from the same state, not two different things. Meanwhile
everything renders unconditionally in one scrolling page regardless of which
effect is actually in use, which is the wasted-space complaint.

User-confirmed target (see the sidebar+detail ASCII preview approved in this
milestone's kickoff): a `NavigationSplitView` — one categorized sidebar
listing every effect, a detail pane showing only the selected effect's
controls, and a persistent header (Source A/B + global render/export
settings) that never scrolls away.

## Non-negotiable constraint: this is a VIEW-ONLY refactor

**Do not touch `AppState.swift`'s business logic, the `run*Render()`
functions, `RustBridgePlaceholder`, or any `@Published` property's type or
name.** Every effect's existing `Picker`/`Stepper`/`Toggle` block already
binds correctly to working state and a working render pipeline — the job is
to move those existing blocks into a new container structure, not rewrite
what they do. Where `WorkflowPanelView` and `RenderPanelView` currently
render the *same effect* twice (see the 8-effect overlap above), merge them
into one detail view per effect with the commonly-used knobs visible by
default and the rest behind a `DisclosureGroup("More knobs")` — this
directly deletes the duplication instead of picking one copy arbitrarily.
Verify every deleted/moved binding still compiles against `AppState`; if a
knob exists in both copies with different framing, keep the more complete
(RenderPanelView) version's controls and the more concise (WorkflowPanelView)
version's ordering/help text where they differ.

## Target architecture

- `ContentView.swift` becomes the `NavigationSplitView` shell:
  - **Sidebar**: `List` of `EffectCategory` sections, each containing
    `EffectListing` rows (see catalog below). Selection drives
    `@State private var selection: EffectListing?`.
  - **Persistent header** (above or beside the split view, never inside a
    scroll that hides it): the existing `SourceSlotView` pair, plus the
    render-quality/export-format/output controls currently at the top of
    `RenderPanelView` (lines ~1-45: Render Quality, Output Format, ProRes
    FPS/Profile pickers) — these are global, not per-effect, and apply to
    every render regardless of which effect is selected.
  - **Detail pane**: switches on `selection` to show exactly one effect's
    controls (basic + "More knobs" disclosure + Run button + status text),
    or a placeholder/overview state when nothing is selected yet.
- Suggested new file layout (adjust as needed, this is not gospel):
  - `Views/Sidebar/EffectCatalog.swift` — the `EffectCategory`/`EffectListing`
    enums driving the sidebar (see catalog below), pure data, no logic.
  - `Views/Sidebar/EffectSidebarView.swift` — the sidebar list itself.
  - `Views/Effects/<Category>EffectViews.swift` — one file per category below,
    each holding the per-effect detail `View` structs (moved/merged from
    `WorkflowPanelView.swift` + `RenderPanelView.swift`).
  - `Views/GlobalRenderSettingsView.swift` — the pulled-out Render
    Quality/Export/ProRes header block.
  - Keep `CompositionPanelView.swift`, `CoagulatedBlendPanelView.swift`,
    `DispersionBlendPanelView.swift`, `FluidMosaicPanelView.swift` as their
    own files (already reasonably scoped) — just wire them in as sidebar
    entries instead of items inside the old `DisclosureGroup`.
  - `AnalysisPanelView` and `NodeGraphPlaceholderView` are both static
    placeholders (no live state, no render function) — give them a "Tools"
    or "Diagnostics" sidebar section rather than deleting them.
- Delete `WorkflowPanelView.swift`'s and `RenderPanelView.swift`'s outer
  shells once their contents are redistributed; the per-effect `@ViewBuilder`
  bodies are the reusable part, the surrounding
  `VStack`/`DisclosureGroup("Advanced knobs")`/scroll wrapper is not.

## Effect catalog (sidebar categories — group as shown; naming is illustrative, keep it close to existing on-screen titles)

**Displacement**
- Flow Displace (`runTwoSourceFrameSequenceRender`)
- Flow Feedback (`runFlowFeedbackSequenceRender`) — "Temporal Flow Feedback"
- Rutt-Etra (`runRuttEtraSequenceRender`) — "Rutt-Etra — Luma-Displaced Scanlines"

**Fluid / Advection**
- Fluid Advection (`runProceduralFluidAdvectSequenceRender`,
  `runTwoSourceFluidAdvectSequenceRender`,
  `runOpticalFlowAdvectSequenceRender`, `runFieldParticlesSequenceRender` —
  one section, mode picker selects which of the 4 runs)

**Blend / Mosaic (mutual A×B)**
- Conv-Blend (`runConvolutionalBlendRender`) — "Convolutional AV Blending"
- Coagulated Flow Blend (`CoagulatedBlendPanelView`,
  `runCoagulatedBlendSequenceRender`)
- Dispersion Blend (`DispersionBlendPanelView`, `runDispersionBlendRender`)
- Fluid Mosaic (`FluidMosaicPanelView`, `runFluidMosaicRender`)

**Feedback / Datamosh**
- Controlled Datamosh (`runDatamoshRender`)
- Bitstream Datamosh (`runBitstreamDatamoshRender`) — note the fallback fix
  just shipped in `AppState.swift` (`bitstreamInputVideoURL ?? sourceAURL`);
  preserve it exactly, don't revert to the old required-local-picker form.
- Cascade Collage (`runCascadeCollageSequenceRender`) — "Scribbled-Edge Tile
  Cascade"
- Trail Cascade (`runTrailCascadeSequenceRender`) — distinct effect from
  Cascade Collage despite the similar name (different state fields:
  `cascadeFieldType`/`cascadeTileSize`/`cascadeGridSpacing`/`cascadeAdvect`
  vs `cascadeCollageBlockBlend`); keep them as two separate sidebar rows.

**Generative**
- Morphogenesis (`runMorphogenesisSequenceRender`) — model picker
  (Gray-Scott/FHN/Lenia) lives inside this one section, do not split into
  three sidebar rows.
- Granular Mosaic (`runGranularMosaicPoolSequenceRender`) — "Temporal Pool
  (Joint-AV)"

**Post / Look**
- Retro Static (`runRetroStaticSequenceRender`)
- Channel Shift (`runChannelShiftSequenceRender`)
- Palette Quantize (`runPaletteQuantizeSequenceRender`)
- Pixel Sort (`runPixelSortRender`)

**Audio / Cross-Synth**
- Video Vocoder (`runVideoVocoderSequenceRender`) — "Tonal Routing"
- Spectral Cross-Synthesis (`runSpectralCrossSynthRender`)
- Audio Impulse Convolution (`runAudioImpulseConvolutionRender`)
- Audio-to-Video Route (`runAudioVideoRouteRender`)
- Video-to-Audio Route (`runVideoAudioRouteRender`)

**Composition**
- Composition Timeline (`CompositionPanelView`, `runComposition`) — a
  different paradigm (scene chain spec file, not a single-effect render);
  give it its own sidebar section rather than folding it into any category
  above.

**Tools** (no render function — static/diagnostic panels)
- Analysis (`AnalysisPanelView`)
- Node Graph (`NodeGraphPlaceholderView`)

That's ~24 render effects + Composition + 2 tool panels. Every one of them
must be reachable from the sidebar — none should require re-adding a
Workflow-style "quick 8" special case.

## Phased plan (checkpoint after each phase passes `swift build && swift test`)

1. **Scaffold**: `EffectCatalog.swift` (data only) + `EffectSidebarView.swift`
   + the new `NavigationSplitView` shell in `ContentView.swift`, detail pane
   showing a placeholder ("Select an effect") for every catalog entry. Should
   build and run with zero effect controls wired yet — proves the navigation
   frame before migrating content.
2. **Migrate one category** (suggest Displacement first, smallest) end to
   end: move its 2-3 effects' actual controls into the new detail views,
   confirm each still renders correctly and its Run button still calls the
   same `AppState` method. Use this as the template for the rest.
3. **Migrate the remaining categories**, one at a time, each its own
   checkpoint. Merge Workflow/Advanced duplicates as you reach the 8
   overlapping effects.
4. **Pull out the global render/export header**, wire the persistent
   Source A/B + global settings area.
5. **Delete the dead shells** (`WorkflowPanelView.swift`'s/
   `RenderPanelView.swift`'s now-empty outer structure) once nothing
   references them; keep any `private enum`/helper types that individual
   effect views still depend on (move them alongside the views that use
   them).
6. **Final pass**: consistent spacing/padding across all detail views (this
   is part of the "clunky formatting" complaint — pick one spacing scale and
   apply it everywhere, don't leave each migrated block with its
   file-of-origin's ad hoc padding).

## Acceptance criteria

- `swift build` and `swift test` both clean (capture the pre-change pass
  count — currently 158 tests, 0 failures — and confirm it's unchanged;
  this is a view-layer move, it shouldn't change test outcomes at all).
- Every effect in the catalog above is reachable from the sidebar with
  working controls and a working Run button (spot-check a handful by hand —
  the app is a GUI, there's no automated screenshot harness for it, so this
  needs `swift run MorphogenMacApp` + actually clicking through).
- No `WorkflowPanelView` vs `RenderPanelView` duplicate controls remain for
  the 8 previously-overlapping effects.
- Source A/B and global render/export settings are visible regardless of
  which sidebar item is selected.
- Checkpoint (local commit, no push) after each phase per the project's
  standing `/checkpoint` convention — this is a large change and long
  uncommitted stretches risk losing work.
