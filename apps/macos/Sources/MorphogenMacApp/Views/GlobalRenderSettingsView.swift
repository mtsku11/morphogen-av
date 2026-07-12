import SwiftUI

/// The global, effect-independent part of the persistent header: render
/// quality / export format / ProRes settings (formerly duplicated at the
/// top of RenderPanelView and inside WorkflowPanelView's render band), plus
/// the source-proxy extraction step that most frame-sequence effects need
/// before they can render anything (formerly WorkflowPanelView's "1. Sources
/// and Proxies" band — proxies aren't per-effect, so they belong here next
/// to Source A/B, not duplicated into 20+ per-effect detail views).
struct GlobalRenderSettingsView: View {
  @ObservedObject var state: AppState

  @State private var showsProxyTools = false
  @State private var showsQualityTools = false

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
      // Collapsed by default, like Sources & Proxies — the quality/export
      // settings are set-and-forget, not something the header needs to spend
      // a full row on.
      DisclosureGroup("Quality & Export", isExpanded: $showsQualityTools) {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Picker("Quality", selection: $state.renderQuality) {
            ForEach(RenderQualityOption.allCases) { option in
              Text(option.rawValue).tag(option)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 360)

          Picker("Format", selection: $state.exportFormat) {
            ForEach(ExportFormatOption.allCases) { option in
              Text(option.rawValue).tag(option)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 160)

          Picker("ProRes FPS", selection: $state.proResFrameRate) {
            ForEach(ProResFrameRateOption.allCases) { option in
              Text(option.rawValue).tag(option)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 130)

          Picker("ProRes Profile", selection: $state.proResProfile) {
            ForEach(ProResExportProfile.allCases) { profile in
              Text(profile.displayName).tag(profile)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 220)

          Button {
            state.exportLastFrameSequenceProResMovie()
          } label: {
            Label("Export ProRes", systemImage: "film.badge.plus")
          }
        }
        .padding(.top, 8)
      }

      DisclosureGroup("Sources & Proxies", isExpanded: $showsProxyTools) {
        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
          HStack(spacing: EffectDetailLayout.controlRowSpacing) {
            Button {
              state.probeSelectedSources()
            } label: {
              Label("Probe Sources", systemImage: "waveform.path.ecg.rectangle")
            }

            Button {
              state.probePreviewFrames()
            } label: {
              Label("Decode Preview Frames", systemImage: "rectangle.on.rectangle")
            }

            Button {
              state.chooseMediaProxyOutputDirectory()
            } label: {
              Label("Proxy Output", systemImage: "folder.badge.plus")
            }

            Button {
              state.extractSelectedSourceProxies()
            } label: {
              Label("Extract Proxies", systemImage: "square.stack.3d.down.forward")
            }
            .buttonStyle(.borderedProminent)
            .disabled(state.isExtractingProxies)

            if state.isExtractingProxies {
              ProgressView()
                .controlSize(.small)
            }
          }

          HStack(spacing: EffectDetailLayout.controlRowSpacing) {
            Stepper(value: $state.mediaProxyFrameRate, in: 1...60, step: 1) {
              Text("Proxy \(state.mediaProxyFrameRate, specifier: "%.0f") fps")
            }
            .frame(width: 140, alignment: .leading)

            Stepper(value: $state.mediaProxyMaxFrames, in: 1...600, step: 1) {
              Text("Limit \(state.mediaProxyMaxFrames) frames")
            }
            .frame(width: 170, alignment: .leading)

            Button {
              state.chooseFrameSequenceOutputDirectory()
            } label: {
              Label("Sequence Output", systemImage: "folder.badge.plus")
            }

            Picker("Preview", selection: $state.showcaseIntensity) {
              ForEach(ShowcaseIntensityOption.allCases) { intensity in
                Text(intensity.rawValue).tag(intensity)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)

            Button {
              state.runShowcasePreviewRender()
            } label: {
              Label("Showcase Preview", systemImage: "sparkles")
            }
            .buttonStyle(.bordered)
            .disabled(state.isExtractingProxies)
          }

          Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 6) {
            GridRow {
              Text("A Frames")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
              Text(state.frameSequenceModulatorPath)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(2)
            }
            GridRow {
              Text("B Frames")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
              Text(state.frameSequenceCarrierPath)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(2)
            }
            GridRow {
              Text("Proxy Root")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
              Text(state.mediaProxyOutputPath)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(2)
            }
            GridRow {
              Text("Output Root")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
              Text(state.frameSequenceOutputPath)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(2)
            }
          }
        }
        .padding(.top, 8)
      }

      HStack(spacing: 8) {
        Image(systemName: "info.circle")
          .foregroundStyle(.secondary)
        Text(state.statusMessage)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }
    }
  }
}
