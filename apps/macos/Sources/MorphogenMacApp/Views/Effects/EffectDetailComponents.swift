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
