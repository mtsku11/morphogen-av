import Foundation

/// Global source-direction control shown in the header, beside the two
/// "Choose Source" buttons. Determines which loaded source acts as the
/// modulator (analysis) versus the carrier (material) for the
/// two-source-capable effects. Single-source effects ignore it — see
/// `EffectListing.supportsSourceDirection` — so switching to one of those
/// leaves its normal Source-B-as-carrier behaviour untouched.
enum SourceRelationship: String, CaseIterable, Identifiable {
  /// The default: Source A modulates Source B (A = modulator, B = carrier).
  case aModifiesB = "A modifies B"
  /// Roles swapped: Source B modulates Source A (B = modulator, A = carrier).
  case bModifiesA = "B modifies A"
  /// One source drives its own transformation. Uses Source B for both roles,
  /// matching the existing self-flow semantics (which read the carrier).
  case selfModify = "Self modifies itself"

  var id: String { rawValue }
}
