import SwiftUI

/// Shared modulation-routing controls used by nearly every effect's "More
/// knobs" section: a mod slot's source/scale/offset row, its enum variant,
/// the modulator-media row, named-modulator declarations, and the spatial
/// matte config row. Originally private to RenderPanelView.swift; moved here
/// when that file's dead outer shell was deleted (docs/UI_REDESIGN_MILESTONE.md
/// phase 5) since these are reused across every per-category effect view
/// file, not scoped to any one of them.

/// One knob's modulation slot: source picker (Off = no route) plus the affine
/// scale/offset mapping, shown only when a source is chosen.
struct ModulationSlotRow: View {
  let label: String
  @Binding var source: ModulationSourceOption
  @Binding var scale: Double
  @Binding var offset: Double
  @Binding var samplingOverride: ModulationSamplingOverrideOption
  // Defaults suit [0, 1] knobs; pixel-unit targets (channel-shift offsets)
  // pass wider ranges so the envelope can span a visible shift.
  var scaleRange: ClosedRange<Double> = -8...8
  var scaleStep = 0.1
  var offsetRange: ClosedRange<Double> = -1...1
  var offsetStep = 0.05
  // Named-modulator binding; nil (the default) hides the picker so call sites
  // predating named modulators are unchanged. The picker only shows once at
  // least one named modulator is declared (`modulatorNames` non-empty).
  var modulator: Binding<String>? = nil
  var modulatorNames: [String] = []
  // LFO opt-in; nil (the default) omits LFO from the source picker so call
  // sites predating LFO are unchanged. Mirrors the named-modulator binding.
  var lfoShape: Binding<LfoShapeOption>? = nil
  var lfoRate: Binding<Double>? = nil
  var lfoPhase: Binding<Double>? = nil
  // Performance-capture opt-in; false (the default) omits Captured from the
  // source picker so panels without a capture story are unchanged. Mirrors
  // the LFO opt-in.
  var captureAvailable = false
  // MIDI opt-in; false (the default) omits the four midi-* sources so panels
  // without a MIDI story are unchanged. The CC-number binding shows a stepper
  // when the source is MIDI CC (spelled per-slot as midi-cc(<n>)).
  var midiAvailable = false
  var midiCcNumber: Binding<Int>? = nil

  var body: some View {
    ControlFlow {
      OptionKnob(
        label: "Mod \(label)",
        selection: $source,
        options: ModulationSourceOption.allCases.filter {
          ($0 != .lfo || lfoShape != nil) && ($0 != .captured || captureAvailable)
            && (!$0.isMidi || midiAvailable)
        },
        optionLabel: { $0.rawValue }
      )
      .help("Analysis envelope routed onto this knob; Off keeps the knob constant.")

      if source != .off {
        if source == .captured {
          Text("Requires performance-capture recording (not available in this build)")
            .font(.caption)
            .foregroundStyle(.secondary)
            .frame(width: 220, alignment: .leading)
            .help("Arm this slot, then record a gesture take onto it. The capture-recording UI is a known gap in the current shell.")
        }
        if source == .midiCc, let midiCcNumber {
          Stepper(value: midiCcNumber, in: 0...127, step: 1) {
            Text("CC \(midiCcNumber.wrappedValue)")
          }
          .frame(width: 120, alignment: .leading)
          .help("MIDI controller number this slot reads (route: midi-cc(<n>)).")
        }
        if source == .lfo, let lfoShape, let lfoRate, let lfoPhase {
          OptionKnob("Shape", selection: lfoShape)
            .help("LFO waveform; every shape spans [0, 1] and starts at 0.")

          Stepper(value: lfoRate, in: 0.05...60, step: 0.05) {
            Text("Rate \(lfoRate.wrappedValue, specifier: "%.2f") Hz")
          }
          .frame(width: 160, alignment: .leading)
          .help("Cycles per second on the render's timeline.")

          Stepper(value: lfoPhase, in: 0...1, step: 0.05) {
            Text("Phase \(lfoPhase.wrappedValue, specifier: "%.2f")")
          }
          .frame(width: 150, alignment: .leading)
          .help("Phase offset in cycles (0.25 = a quarter cycle).")
        } else if source != .captured, let modulator, !modulatorNames.isEmpty {
          OptionKnob(
            label: "Modulator",
            selection: modulator,
            options: [""] + modulatorNames,
            optionLabel: { $0.isEmpty ? "Default" : $0 }
          )
          .help("Which modulator media this route reads; Default uses the panel's Modulator WAV/Frames.")
        }

        Stepper(value: $scale, in: scaleRange, step: scaleStep) {
          Text("Scale \(scale, specifier: "%.2f")")
        }
        .frame(width: 150, alignment: .leading)
        .help("knob = clamp(envelope × scale + offset)")

        Stepper(value: $offset, in: offsetRange, step: offsetStep) {
          Text("Offset \(offset, specifier: "%.2f")")
        }
        .frame(width: 160, alignment: .leading)

        OptionKnob("Sampling", selection: $samplingOverride)
          .help("Overrides this route's sampling; Default inherits the panel Sampling picker.")
      }
    }
  }
}

/// Mod slot for an enum knob: instead of opaque scale/offset steppers, two
/// variant pickers — envelope 0 selects **From**, envelope 1 selects **To**
/// (`enumModulationMapping` emits the equivalent affine route). From == To is
/// legal and holds the knob at that variant (the continuity identity).
struct EnumModulationSlotRow<Option>: View
where
  Option: CaseIterable & Identifiable & Hashable & RawRepresentable,
  Option.RawValue == String,
  Option.AllCases: RandomAccessCollection
{
  let label: String
  @Binding var source: ModulationSourceOption
  @Binding var from: Option
  @Binding var to: Option
  @Binding var samplingOverride: ModulationSamplingOverrideOption
  // Named-modulator binding; nil (the default) hides the picker so call sites
  // predating named modulators are unchanged. Mirrors `ModulationSlotRow`.
  var modulator: Binding<String>? = nil
  var modulatorNames: [String] = []

  var body: some View {
    ControlFlow {
      // Enum slots don't opt in to LFO, capture, or MIDI (this slice) — filter all out.
      OptionKnob(
        label: "Mod \(label)",
        selection: $source,
        options: ModulationSourceOption.allCases.filter {
          $0 != .lfo && $0 != .captured && !$0.isMidi
        },
        optionLabel: { $0.rawValue }
      )
      .help("Analysis envelope routed onto this knob; Off keeps the knob constant.")

      if source != .off {
        if let modulator, !modulatorNames.isEmpty {
          OptionKnob(
            label: "Modulator",
            selection: modulator,
            options: [""] + modulatorNames,
            optionLabel: { $0.isEmpty ? "Default" : $0 }
          )
          .help("Which modulator media this route reads; Default uses the panel's Modulator WAV/Frames.")
        }

        OptionKnob("From", selection: $from)
          .help("Variant selected when the envelope is at 0.")

        Text("→")
          .foregroundStyle(.secondary)

        OptionKnob("To", selection: $to)
          .help("Variant selected when the envelope is at 1; in between, the envelope steps through the variants From→To.")

        OptionKnob("Sampling", selection: $samplingOverride)
          .help("Overrides this route's sampling; Default inherits the panel Sampling picker.")
      }
    }
  }
}

/// Spatial matte config row (Tier 5.4 S2, docs/SPATIAL_MATTE_MILESTONE.md):
/// source picker (Off/A-Luma/A-Flow/A-Edge), gain stepper, and a matte-frames
/// directory picker. The gain stepper and frames picker only appear once a
/// source is chosen, matching `ModulationMediaRow`'s reveal-on-active idiom.
struct MatteConfigRow: View {
  @Binding var source: MatteSourceOption
  @Binding var gain: Double
  let framesURL: URL?
  let chooseFrames: () -> Void
  /// Explains the matte-frames default for this command (rutt-etra/channel-
  /// shift fall back to Source A; palette-quantize has no such fallback).
  let framesHelp: String

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      HStack(spacing: 16) {
        Picker("Matte", selection: $source) {
          ForEach(MatteSourceOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 280)
        .help(
          "Gate the effect's blend per-pixel by analysis of the matte frames "
          + "instead of applying it uniformly.")

        if source != .off {
          Stepper(value: $gain, in: 0...4, step: 0.1) {
            Text("Gain \(gain, specifier: "%.2f")")
          }
          .frame(width: 140, alignment: .leading)
          .help("Applied after the source's fixed normalization/lift, before clamp to [0,1].")
        }
      }

      if source != .off {
        HStack(spacing: 16) {
          Button("Matte Frames…", action: chooseFrames)
          Text(framesURL?.lastPathComponent ?? "none selected")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .help(framesHelp)
      }
    }
  }
}

struct ModulationMediaRow: View {
  let sources: [ModulationSourceOption]
  let audioURL: URL?
  let framesURL: URL?
  @Binding var sampling: ModulationSamplingOption
  let chooseAudio: () -> Void
  let chooseFrames: () -> Void
  // MIDI media; defaulted so panels without a MIDI story are unchanged.
  var midiURL: URL? = nil
  var chooseMidi: (() -> Void)? = nil

  var body: some View {
    if sources.contains(where: { $0 != .off }) {
      HStack(spacing: 16) {
        if sources.contains(where: \.needsAudio) {
          Button("Modulator WAV…", action: chooseAudio)
          Text(audioURL?.lastPathComponent ?? "none selected")
            .font(.caption)
            .foregroundStyle(.secondary)
        }

        if sources.contains(where: \.needsFrames) {
          Button("Modulator Frames…", action: chooseFrames)
          Text(framesURL?.lastPathComponent ?? "none selected")
            .font(.caption)
            .foregroundStyle(.secondary)
        }

        if sources.contains(where: \.needsMidi), let chooseMidi {
          Button("Modulator MIDI…", action: chooseMidi)
          Text(midiURL?.lastPathComponent ?? "none selected")
            .font(.caption)
            .foregroundStyle(.secondary)
        }

        Picker("Sampling", selection: $sampling) {
          ForEach(ModulationSamplingOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 200)
        .help("Hold steps between envelope samples; Smooth interpolates linearly.")
      }
    }
  }
}

/// Declares extra named modulators for a panel: a name field plus WAV/Frames
/// pickers per row, and an Add button. A mod slot's Modulator picker binds to
/// one of these by name; the panel's default `ModulationMediaRow` still covers
/// unnamed slots.
struct NamedModulatorsSection: View {
  @Binding var modulators: [NamedModulatorEntry]
  let onAdd: () -> Void
  let onRemove: (UUID) -> Void
  let chooseAudio: (UUID) -> Void
  let chooseFrames: (UUID) -> Void
  // MIDI picker; nil (the default) hides the button so panels without a MIDI
  // story are unchanged.
  var chooseMidi: ((UUID) -> Void)? = nil

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      HStack {
        Text("Named Modulators")
          .font(.caption.weight(.semibold))
          .foregroundStyle(.secondary)
        Button("Add", action: onAdd)
          .help("Declare another modulator so different slots can read different media.")
      }

      ForEach($modulators) { $entry in
        HStack(spacing: 12) {
          TextField("Name", text: $entry.name)
            .frame(width: 120)
            .help("Route grammar: target=name.source. Must be non-empty and unique.")

          Button("WAV…") { chooseAudio(entry.id) }
          Text(entry.audioURL?.lastPathComponent ?? "—")
            .font(.caption)
            .foregroundStyle(.secondary)

          Button("Frames…") { chooseFrames(entry.id) }
          Text(entry.framesURL?.lastPathComponent ?? "—")
            .font(.caption)
            .foregroundStyle(.secondary)

          if let chooseMidi {
            Button("MIDI…") { chooseMidi(entry.id) }
            Text(entry.midiURL?.lastPathComponent ?? "—")
              .font(.caption)
              .foregroundStyle(.secondary)
          }

          Button(role: .destructive) { onRemove(entry.id) } label: {
            Image(systemName: "trash")
          }
          .help("Remove this modulator; slots bound to it reset to Default.")
        }
      }
    }
  }
}
