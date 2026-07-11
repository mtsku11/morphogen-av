import SwiftUI

/// Blend / Mosaic — mutual A×B effects. Conv-Blend is ported here from
/// RenderPanelView (it never had a Workflow duplicate); Coagulated Flow
/// Blend, Dispersion Blend, and Fluid Mosaic already live in their own
/// reasonably-scoped panel files per the milestone doc and are wired in
/// as-is.

struct ConvBlendDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .convBlend)
        .help("Each Source A frame supplies a normalized KxK luma kernel that Source B's frame is convolved with.")

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Button {
          state.chooseConvBlendModulatorDirectory()
        } label: {
          Label("Source A Frames", systemImage: "photo.on.rectangle")
        }
        Button {
          state.chooseConvBlendCarrierDirectory()
        } label: {
          Label("Source B Frames", systemImage: "photo.on.rectangle.angled")
        }
        Button {
          state.chooseConvBlendOutputDirectory()
        } label: {
          Label("Output Dir", systemImage: "folder")
        }
      }

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Stepper(value: $state.convBlendKernelSize, in: 1...15, step: 2) {
          Text("Kernel \(state.convBlendKernelSize)×\(state.convBlendKernelSize)")
        }
        .frame(width: 170, alignment: .leading)
        .help("Odd kernel edge length; larger spreads the blend wider.")

        Stepper(value: $state.convBlendAmount, in: 0...1, step: 0.05) {
          Text("Amount \(state.convBlendAmount, specifier: "%.2f")")
        }
        .frame(width: 170, alignment: .leading)
        .help("0 = Source B passthrough; 1 = fully convolved.")
      }

      MoreKnobs {
        Toggle("Colour kernels (per R/G/B)", isOn: $state.convBlendColorMode)
          .help("Extract a separate kernel from each of Source A's R/G/B channels instead of one luma kernel.")

        Picker("Backend", selection: $state.convBlendBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 200)
        .help("Metal is gated per-frame against the CPU reference.")
      }

      Button {
        state.runConvolutionalBlendRender()
      } label: {
        Label("Run Convolution Blend", systemImage: EffectListing.convBlend.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.convBlendSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }
}
