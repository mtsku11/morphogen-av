import SwiftUI
import AppKit

struct SourceSlotView: View {
  let title: String
  let role: SourceRole
  @Binding var path: String
  let probeSummary: String
  let previewSummary: String
  let previewImage: NSImage?
  let onChoose: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack {
        VStack(alignment: .leading, spacing: 2) {
          Text(title)
            .font(.headline)
          Text(role.rawValue)
            .font(.subheadline)
            .foregroundStyle(.secondary)
        }

        Spacer()

        Image(systemName: role == .modulator ? "waveform.path.ecg" : "film")
          .font(.title3)
          .foregroundStyle(.tint)
      }

      Text(role.description)
        .font(.caption)
        .foregroundStyle(.secondary)

      ZStack {
        RoundedRectangle(cornerRadius: 6)
          .fill(.black.opacity(0.08))

        if let previewImage {
          Image(nsImage: previewImage)
            .resizable()
            .scaledToFit()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
          Image(systemName: "rectangle.dashed")
            .font(.title2)
            .foregroundStyle(.secondary)
        }
      }
      .frame(height: 104)
      .clipShape(RoundedRectangle(cornerRadius: 6))

      Text(path)
        .font(.system(.caption, design: .monospaced))
        .lineLimit(2)
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading)

      Text(probeSummary)
        .font(.caption)
        .lineLimit(3)
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading)

      Text(previewSummary)
        .font(.caption)
        .lineLimit(3)
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading)

      Button {
        onChoose()
      } label: {
        Label("Choose Source", systemImage: "folder")
      }
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.quaternary.opacity(0.55), in: RoundedRectangle(cornerRadius: 8))
  }
}
