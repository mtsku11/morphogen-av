import Foundation

enum SourceRole: String, Identifiable, Sendable {
  case modulator = "Modulator"
  case carrier = "Carrier"

  var id: String { rawValue }

  var description: String {
    switch self {
    case .modulator:
      return "Analysis source"
    case .carrier:
      return "Material source"
    }
  }
}
