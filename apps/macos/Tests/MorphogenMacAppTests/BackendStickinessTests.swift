@testable import MorphogenMacApp
import XCTest

/// The render-backend pickers persist across launches (UserDefaults-backed) so a chosen
/// backend — e.g. Metal, now that the optical-flow port makes it faster — stays selected.
/// First launch (no stored value) falls back to the per-effect default.
@MainActor
final class BackendStickinessTests: XCTestCase {
  private let keys = [
    "backend.feedback", "backend.fluid", "backend.granularPool", "backend.vocoder",
    "backend.audioRoute", "backend.datamosh", "backend.convBlend",
  ]

  override func setUp() {
    super.setUp()
    for key in keys { UserDefaults.standard.removeObject(forKey: key) }
  }

  override func tearDown() {
    for key in keys { UserDefaults.standard.removeObject(forKey: key) }
    super.tearDown()
  }

  func testDefaultsApplyWhenNothingPersisted() {
    let state = AppState()
    XCTAssertEqual(state.datamoshBackend, .cpu)
    XCTAssertEqual(state.feedbackBackend, .metal)
  }

  func testSelectingABackendPersistsAndReloads() {
    let first = AppState()
    first.datamoshBackend = .metal
    first.vocoderBackend = .metal

    // A fresh instance models the next launch: it reads the persisted choice.
    let next = AppState()
    XCTAssertEqual(next.datamoshBackend, .metal)
    XCTAssertEqual(next.vocoderBackend, .metal)
  }

  func testReturningToTheDefaultIsAlsoPersisted() {
    let first = AppState()
    first.datamoshBackend = .metal
    first.datamoshBackend = .cpu

    let next = AppState()
    XCTAssertEqual(next.datamoshBackend, .cpu)
  }
}
