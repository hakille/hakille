import XCTest
@testable import Hakille

final class HoldToConfirmTests: XCTestCase {

    func testRequiredHoldDurationIs600ms() {
        XCTAssertEqual(HOLD_TO_CONFIRM_MS, 600, "Hold duration must be strictly 600ms")
    }

    func testProgressMathClamped() {
        XCTAssertEqual(computeHoldProgress(elapsedMs: 0), 0.0, accuracy: 1e-6)
        XCTAssertEqual(computeHoldProgress(elapsedMs: 150), 0.25, accuracy: 1e-6)
        XCTAssertEqual(computeHoldProgress(elapsedMs: 300), 0.5, accuracy: 1e-6)
        XCTAssertEqual(computeHoldProgress(elapsedMs: 450), 0.75, accuracy: 1e-6)
        XCTAssertEqual(computeHoldProgress(elapsedMs: 600), 1.0, accuracy: 1e-6)
        XCTAssertEqual(computeHoldProgress(elapsedMs: 601), 1.0, accuracy: 1e-6)
        XCTAssertEqual(computeHoldProgress(elapsedMs: 1200), 1.0, accuracy: 1e-6)
    }

    func testEarlyReleaseIsStrictlyIncomplete() {
        let progress599 = computeHoldProgress(elapsedMs: 599)
        XCTAssertLessThan(progress599, 1.0, "599ms hold must be strictly less than 1.0")
        XCTAssertEqual(progress599, 599.0 / 600.0, accuracy: 1e-6)
    }
}
