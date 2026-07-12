import SwiftUI
import AppKit

/// Compact header source control: a single "Choose Source" button per slot,
/// showing a small thumbnail and the loaded file name. Replaces the former
/// full-height `SourceSlotView` cards (probe/preview summaries now live in the
/// per-effect detail flow, not the always-visible header).
struct CompactSourceButton: View {
  let title: String
  let role: SourceRole
  let path: String
  let previewImage: NSImage?
  let onChoose: () -> Void

  /// The trailing path component of a loaded source, or nil when the stored
  /// `path` is still one of the "No … selected" / "Preview not run" placeholders.
  private var loadedName: String? {
    guard path.contains("/") else { return nil }
    return (path as NSString).lastPathComponent
  }

  var body: some View {
    Button(action: onChoose) {
      HStack(spacing: 10) {
        thumbnail

        VStack(alignment: .leading, spacing: 2) {
          Text(title)
            .font(.subheadline.weight(.semibold))
          Text(loadedName ?? "Choose source…")
            .font(.caption)
            .foregroundStyle(loadedName == nil ? Color.secondary : .primary)
            .lineLimit(1)
            .truncationMode(.middle)
        }

        Image(systemName: "folder")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
      .frame(width: 240, alignment: .leading)
      .padding(.vertical, 6)
      .padding(.horizontal, 10)
    }
    .buttonStyle(.bordered)
    .help(loadedName.map { "\(title): \($0). Click to choose a different source." }
      ?? "Choose \(title) (\(role.description.lowercased())).")
  }

  private var thumbnail: some View {
    ZStack {
      RoundedRectangle(cornerRadius: 4)
        .fill(.quaternary.opacity(0.6))

      if let previewImage {
        Image(nsImage: previewImage)
          .resizable()
          .scaledToFill()
      } else {
        Image(systemName: role == .modulator ? "waveform.path.ecg" : "film")
          .font(.callout)
          .foregroundStyle(.tint)
      }
    }
    .frame(width: 34, height: 34)
    .clipShape(RoundedRectangle(cornerRadius: 4))
  }
}
