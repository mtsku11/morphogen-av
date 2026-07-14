import SwiftUI

/// Shared per-effect detail-pane scaffolding: a title, primary knobs, a
/// "More knobs" disclosure for the rest, and a trailing summary/status line.
/// Every migrated effect view is built from these so the final spacing pass
/// (docs/UI_REDESIGN_MILESTONE.md phase 6) has one place to tune.
enum EffectDetailLayout {
  static let sectionSpacing: CGFloat = 16
  static let controlRowSpacing: CGFloat = 16
  static let knobWidth: CGFloat = 160
  /// Tighter spacing for a dense vertical run of modulation-slot rows inside
  /// "More knobs" — distinct from the looser top-level `sectionSpacing`.
  static let modGroupSpacing: CGFloat = 10
}

/// Effect title, shown at the top of every detail view.
struct EffectTitleView: View {
  let listing: EffectListing

  var body: some View {
    Label(listing.title, systemImage: listing.systemImage)
      .font(.title2.weight(.semibold))
  }
}

/// The collapsible "More knobs" section every effect detail view uses for
/// its less-frequently-tweaked controls.
struct MoreKnobs<Content: View>: View {
  @State private var isExpanded = false
  @ViewBuilder var content: () -> Content

  var body: some View {
    DisclosureGroup("More knobs", isExpanded: $isExpanded) {
      VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
        content()
      }
      .padding(.top, 8)
    }
  }
}

/// A leading-aligned wrapping layout for effect controls: items fill the full
/// row width and wrap only when they run out of space, so wide windows show
/// one dense row instead of a stack of half-empty fixed rows (the pre-knob
/// panels hard-coded one HStack per row). Items are vertically centered per
/// row since knobs and steppers have different heights.
struct ControlFlow: Layout {
  var spacing: CGFloat = EffectDetailLayout.controlRowSpacing
  var rowSpacing: CGFloat = 10

  private func rows(
    proposalWidth: CGFloat, subviews: Subviews
  ) -> [(range: Range<Int>, height: CGFloat)] {
    var result: [(Range<Int>, CGFloat)] = []
    var rowStart = 0
    var x: CGFloat = 0
    var rowHeight: CGFloat = 0
    for index in subviews.indices {
      let size = subviews[index].sizeThatFits(.unspecified)
      let needed = x == 0 ? size.width : x + spacing + size.width
      if x > 0 && needed > proposalWidth {
        result.append((rowStart..<index, rowHeight))
        rowStart = index
        x = size.width
        rowHeight = size.height
      } else {
        x = needed
        rowHeight = max(rowHeight, size.height)
      }
    }
    if rowStart < subviews.count {
      result.append((rowStart..<subviews.count, rowHeight))
    }
    return result
  }

  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let width = proposal.width ?? 900
    let laidOut = rows(proposalWidth: width, subviews: subviews)
    let height =
      laidOut.map(\.height).reduce(0, +) + rowSpacing * CGFloat(max(0, laidOut.count - 1))
    return CGSize(width: width, height: height)
  }

  func placeSubviews(
    in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()
  ) {
    let laidOut = rows(proposalWidth: bounds.width, subviews: subviews)
    var y = bounds.minY
    for row in laidOut {
      var x = bounds.minX
      for index in row.range {
        let size = subviews[index].sizeThatFits(.unspecified)
        subviews[index].place(
          at: CGPoint(x: x, y: y + (row.height - size.height) / 2),
          proposal: ProposedViewSize(size))
        x += size.width + spacing
      }
      y += row.height + rowSpacing
    }
  }
}

/// A stepped rotary knob for discrete options — the replacement for the
/// panels' dropdown pickers. Drag up/right to step forward, down/left to step
/// back; click to advance to the next option; right-click for the full list.
struct OptionKnob<Option: Hashable>: View {
  let label: String
  @Binding var selection: Option
  let options: [Option]
  let optionLabel: (Option) -> String

  @State private var dragStartIndex: Int?

  private var index: Int {
    options.firstIndex(of: selection) ?? 0
  }

  var body: some View {
    VStack(spacing: 3) {
      Text(label)
        .font(.caption2)
        .foregroundStyle(.secondary)
        .lineLimit(1)

      KnobDial(index: index, count: options.count)
        .frame(width: 40, height: 40)
        .contentShape(Circle())
        .gesture(
          DragGesture(minimumDistance: 2)
            .onChanged { value in
              guard !options.isEmpty else { return }
              let start = dragStartIndex ?? index
              dragStartIndex = start
              let steps = Int(((value.translation.width - value.translation.height) / 14).rounded())
              let target = min(max(start + steps, 0), options.count - 1)
              if target != index {
                selection = options[target]
              }
            }
            .onEnded { _ in dragStartIndex = nil }
        )
        .onTapGesture {
          guard !options.isEmpty else { return }
          selection = options[(index + 1) % options.count]
        }

      Text(options.isEmpty ? "—" : optionLabel(selection))
        .font(.caption)
        .lineLimit(1)
        .frame(maxWidth: 92)
    }
    .frame(width: 96)
    .help("\(label): \(optionLabel(selection)) — drag or click to change, right-click for the list.")
    .contextMenu {
      ForEach(options, id: \.self) { option in
        Button {
          selection = option
        } label: {
          if option == selection {
            Label(optionLabel(option), systemImage: "checkmark")
          } else {
            Text(optionLabel(option))
          }
        }
      }
    }
  }
}

extension OptionKnob
where
  Option: CaseIterable & Identifiable & RawRepresentable, Option.RawValue == String,
  Option.AllCases: RandomAccessCollection
{
  /// Convenience for the common enum case: all cases, labelled by raw value.
  init(_ label: String, selection: Binding<Option>) {
    self.init(
      label: label,
      selection: selection,
      options: Array(Option.allCases),
      optionLabel: { $0.rawValue }
    )
  }
}

/// The dial face: tick ring over a 270° sweep with an accent pointer at the
/// selected option.
struct KnobDial: View {
  let index: Int
  let count: Int

  private func angle(for position: Int) -> Angle {
    guard count > 1 else { return .degrees(0) }
    let fraction = Double(position) / Double(count - 1)
    return .degrees(-135 + 270 * fraction)
  }

  var body: some View {
    ZStack {
      Circle()
        .fill(.quaternary.opacity(0.6))
      Circle()
        .strokeBorder(.tertiary, lineWidth: 1)

      // Tick ring: one tick per option up to a readable density.
      let tickCount = min(count, 24)
      ForEach(0..<max(tickCount, 2), id: \.self) { tick in
        let position = count <= 24 ? tick : tick * (count - 1) / max(tickCount - 1, 1)
        Capsule()
          .fill(position == index ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
          .frame(width: 1.5, height: 4)
          .offset(y: -17)
          .rotationEffect(angle(for: position))
      }

      // Pointer.
      Capsule()
        .fill(.tint)
        .frame(width: 3, height: 12)
        .offset(y: -9)
        .rotationEffect(angle(for: index))
    }
    .animation(.easeOut(duration: 0.12), value: index)
  }
}
