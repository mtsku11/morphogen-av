import SwiftUI

struct AnalysisPanelView: View {
  private let analysisTypes = [
    "luminance",
    "edge map",
    "optical flow",
    "depth map",
    "audio RMS",
    "spectral centroid",
    "onset strength",
    "STFT",
    "grain descriptors"
  ]

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      Text("Analysis")
        .font(.headline)

      ForEach(analysisTypes, id: \.self) { analysisType in
        HStack(spacing: 8) {
          Image(systemName: "checkmark.circle")
            .foregroundStyle(.secondary)
          Text(analysisType)
            .font(.callout)
          Spacer()
        }
      }
    }
    .padding(12)
    .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 8))
  }
}
