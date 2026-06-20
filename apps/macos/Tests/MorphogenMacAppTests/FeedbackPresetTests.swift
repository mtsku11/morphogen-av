@testable import MorphogenMacApp
import XCTest

final class FeedbackPresetTests: XCTestCase {
  func testAggressiveDegradationUsesStrongFeedbackWithoutReset() {
    let settings = FeedbackPresetOption.aggressiveDegradation.settings

    XCTAssertEqual(settings?.carrierAmount, 2.5)
    XCTAssertEqual(settings?.feedbackAmount, 7.0)
    XCTAssertEqual(settings?.feedbackMix, 0.92)
    XCTAssertEqual(settings?.decay, 0.998)
    XCTAssertEqual(settings?.flowSource, .opticalFlow)
    XCTAssertEqual(settings?.backend, .metal)
    XCTAssertEqual(settings?.structureMix, 0.0)
    XCTAssertNil(settings?.resetAtFrame)
  }

  func testResetDrivenCutsHasAResetFrame() {
    let settings = FeedbackPresetOption.resetDrivenCuts.settings

    XCTAssertEqual(settings?.resetAtFrame, 48)
    XCTAssertTrue(settings?.writesFlowCache == true)
  }

  func testStructuredMorphReinjectsStructureAtHighMix() {
    let settings = FeedbackPresetOption.structuredMorph.settings

    XCTAssertEqual(settings?.feedbackMix, 0.97)
    XCTAssertEqual(settings?.decay, 0.97)
    XCTAssertEqual(settings?.structureMix, 0.6)
    XCTAssertEqual(settings?.flowSource, .opticalFlow)
    XCTAssertEqual(settings?.backend, .metal)
    XCTAssertNil(settings?.resetAtFrame)
  }

  func testCustomPresetLeavesExistingValuesUntouched() {
    XCTAssertNil(FeedbackPresetOption.custom.settings)
  }
}
