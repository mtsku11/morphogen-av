import SwiftUI

struct NodeGraphPlaceholderView: View {
  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      Text("Node Graph")
        .font(.headline)

      VStack(alignment: .leading, spacing: 18) {
        graphRow(["Source A", "Analysis", "Modulation Signal"], icon: "waveform.path")
        graphRow(["Source B", "Carrier Processing", "Output"], icon: "film.stack")
      }
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 8))
  }

  private func graphRow(_ labels: [String], icon: String) -> some View {
    HStack(spacing: 10) {
      ForEach(Array(labels.enumerated()), id: \.offset) { index, label in
        node(label, icon: index == 0 ? icon : "circle.hexagongrid")

        if index < labels.count - 1 {
          Image(systemName: "arrow.right")
            .foregroundStyle(.secondary)
        }
      }
    }
  }

  private func node(_ label: String, icon: String) -> some View {
    HStack(spacing: 8) {
      Image(systemName: icon)
      Text(label)
        .lineLimit(1)
        .minimumScaleFactor(0.85)
    }
    .font(.callout)
    .padding(.horizontal, 10)
    .padding(.vertical, 8)
    .background(.background.opacity(0.8), in: RoundedRectangle(cornerRadius: 8))
  }
}
