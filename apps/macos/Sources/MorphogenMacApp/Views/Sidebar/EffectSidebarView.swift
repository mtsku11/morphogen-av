import SwiftUI

/// The sidebar half of the `NavigationSplitView` shell: every effect in
/// `EffectCatalog.swift`, grouped by category, driving `selection`.
struct EffectSidebarView: View {
  @Binding var selection: EffectListing?

  var body: some View {
    List(selection: $selection) {
      ForEach(EffectCategory.allCases) { category in
        Section(category.rawValue) {
          ForEach(EffectListing.allCases.filter { $0.category == category }) { item in
            Label(item.title, systemImage: item.systemImage)
              .tag(item)
          }
        }
      }
    }
    .listStyle(.sidebar)
    .navigationTitle("Effects")
  }
}
