# MIDI Modulation Milestone — MIDI file as a modulation source

Tier 5.3 of `docs/DEFERRED_WORK_HANDOFF.md`. Contract written 2026-07-07.

## Origin & Goal

Musicians already sequence automation in a DAW. A Standard MIDI File is parsed
to normalized envelopes exactly like WAV→RMS today: new atomic sources
`midi-cc(<n>)`, `midi-velocity`, `midi-note-density`, `midi-pitch`, fed by
`--modulator-midi <file.mid>` (and the named-modulator analog). File-based ⇒
deterministic ⇒ no carve-out: the content fingerprint joins stateful checkpoint
contracts exactly like audio media.

## Non-goals

- Live MIDI input (non-deterministic; would need a carve-out — not this).
- MIDI *output*, sysex interpretation, pitch-bend (revisit on demand;
  `midi-pitch` here means note pitch, not bend).
- No new crate dependency. **Pure-Rust minimal SMF reader** (~200 lines), the
  manual RIFF-parse precedent (memory `spectral-cross-synth-readout`): don't
  fight a format, read it directly. No GPL anywhere near this.

## The parser (`morphogen-audio/src/midi.rs`)

Reads SMF format 0 and 1. **PPQ division only** — an SMPTE-division file is a
clear error naming the limitation. Handles: header/track chunks (unknown
chunks skipped), variable-length delta times, **running status**, note-on /
note-off (note-on velocity 0 IS note-off — the classic trap), control change,
Set Tempo meta (0x51), end-of-track. All other events are skipped after their
length is consumed. Malformed files (truncated chunk, bad VLQ, missing
header) → `AudioError` variants (thiserror; **no `unwrap()`**).

**Tempo map (the contract's hard part, pinned by test):** default tempo
500 000 µs/quarter until the first Set Tempo. Tick→seconds conversion is exact
piecewise arithmetic in f64: seconds(tick) = Σ over tempo segments of
`segment_ticks * (µs_per_quarter / division) / 1e6`. Format 1: tempo events
live on track 0 but apply globally; all tracks' events merge onto one absolute
timeline (**merge order pinned:** sort by tick, ties by (track index, in-track
order) — determinism requires a total order). A pinned test hand-computes an
event's time across a mid-file 120→60 BPM change and asserts exact equality.

## Sources & envelopes (extraction in `morphogen-cli/src/modulate.rs`)

All four produce `(time_seconds, value)` sample lists consumed by the existing
`modulated_value` machinery, so `@hold`/`@smooth` behave exactly as for audio
envelopes: **hold = step function (the MIDI-natural reading), smooth = linear
interpolation between event samples.** Every envelope gets a sample at t = 0
(value = the source's silence value, unless an event sits at tick 0) so
pre-first-event frames are defined, and a final sample at end-of-track time
holding the last value.

- **`midi-cc(<n>)`** — controller `n` (0–127, validated at parse time) on any
  channel (channels merge; last-writer-wins at equal ticks per the merge
  order). Sample at each CC event: `value / 127.0` — **absolute**
  normalization, not peak-relative (a CC sweep to 64 must not read as full
  scale). Silence value 0.
- **`midi-velocity`** — sample at each note-on: `velocity / 127.0`; when the
  count of sounding notes drops to zero: sample 0. Absolute normalization.
- **`midi-note-density`** — note-on count per sliding 1.0 s window, sampled
  every 62.5 ms (the RMS-hop convention) across the file's duration, then
  **peak-normalized** (relative — the `video-audio-route-readout` trap:
  fixtures must span sparse→busy).
- **`midi-pitch`** — sample at each note-on: `key / 127.0` (absolute); holds
  through note-off (pitch of the most recent note-on; silence value 0).

Grammar: `midi-cc(<n>)` is a parameterised source (the `lfo(...)` parse
precedent — beware the dot/paren traps recorded in memory
`lfo-modulation-sources`); the other three are atomic names. All four are
media sources requiring MIDI media — the route resolver gains a third media
kind beside audio/frames: `--modulator-midi <file>` and
`--named-modulator-midi name=path` (repeatable), following
`resolve_modulator_media` exactly. `spec_text()` round-trips `midi-cc(74)`
verbatim (queue add→run byte-identity depends on it). Combinator leaves: the
new sources are atomic media leaves, so `leaf_media_sources()` /
per-(modulator, leaf) extraction generalize without special cases.

## Fingerprints & state

MIDI file content gets the same fnv1a64 fingerprint treatment as WAV media:
it joins stateful checkpoint contracts (changed file ⇒ refuse resume) and the
named-modulator fingerprint lists. Envelope sidecar cache: **not extended**
— MIDI parsing is cheap like audio (the cache exists for luma/flow only).

## Slices

- **S1 — parser + sources, direct CLI.** `midi.rs` + the four extractions +
  `--modulator-midi`/named flags on the modulatable commands + tempo-map and
  envelope unit tests (SMF fixtures built as byte arrays in test code — no
  binary fixtures in the repo) + a readout render (below).
- **S2 — queue + checkpoint.** Queue tasks persist `modulator_midi` /
  named-MIDI vectors (serde skip-when-none/empty — pre-slice JSON
  byte-identical, pinned); add-time validation through the shared resolver
  (rejection persists nothing); add→run byte-identical smoke; stateful
  checkpoint contract carries the MIDI fingerprint (changed-file refusal
  smoke on flow feedback).
- **S3 — SwiftUI.** MIDI sources join the slot source picker (needsMidi media
  guard beside needsAudio/needsFrames); shared + named modulator rows gain a
  MIDI file picker; bridge emits the new flags; no-MIDI arg arrays
  byte-identical (pinned).

## Anchors (falsifiable)

1. **Tempo exactness:** hand-computed seconds across a tempo change ==
   parser output, exact f64 equality (unit).
2. **CC staircase readout:** a generated SMF ramping CC 74 through
   0→127 in steps drives `displacement_depth=midi-cc(74):...` on a static
   gradient carrier — within-off 0.000, within-on nonzero, and `@hold` frames
   between CC events are byte-identical to each other (step function proof)
   while `@smooth` frames differ (interpolation proof).
3. **Note-on-velocity-0 == note-off** (unit, the classic trap).
4. **Determinism:** same file parsed twice ⇒ identical envelopes; queue
   add→run byte-identical (S2).
5. **Checkpoint refusal:** flow-feedback resume refuses after the MIDI file's
   content changes (S2).

## Acceptance criteria

Per slice: cargo/swift baselines before → after with numbers; clippy/fmt
clean; anchor evidence shown (exact-equality test names, frame-delta numbers,
frames Read for the readout). No `unwrap()` outside tests; no new deps.
