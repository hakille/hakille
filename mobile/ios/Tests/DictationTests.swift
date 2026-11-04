import XCTest
@testable import Hakille

final class DictationTests: XCTestCase {
    func testPermissionDialogDoesNotStartCaptureAndBackgroundRejectsLateGrant() throws {
        var policy = DictationPolicy()
        let token = try policy.begin(foreground: true)
        XCTAssertFalse(policy.leaveForeground(background: false))
        XCTAssertEqual(policy.phase, .authorizing)
        XCTAssertFalse(policy.finish(token: token))
        XCTAssertTrue(policy.leaveForeground(background: true))
        XCTAssertFalse(policy.transition(.listening, token: token))
    }
    func testListeningStopsOnInactivityAndOldFinalCannotFinishNextSession() throws {
        var policy = DictationPolicy()
        let old = try policy.begin(foreground: true)
        XCTAssertTrue(policy.transition(.listening, token: old))
        XCTAssertTrue(policy.leaveForeground(background: false))
        let current = try policy.begin(foreground: true)
        XCTAssertFalse(policy.finish(token: old))
        XCTAssertTrue(policy.transition(.listening, token: current))
        XCTAssertTrue(policy.finish(token: current))
        XCTAssertFalse(policy.finish(token: current))
    }
    func testForegroundAndLocalRecognitionAreRequired() {
        var policy = DictationPolicy()
        XCTAssertThrowsError(try policy.begin(foreground: false))
        XCTAssertThrowsError(try DictationPolicy.requireLocalSupport(supported: false, available: true))
        XCTAssertThrowsError(try DictationPolicy.requireLocalSupport(supported: true, available: false))
        XCTAssertNoThrow(try DictationPolicy.requireLocalSupport(supported: true, available: true))
    }
    func testFinalTranscriptPreservesExistingDraftAndNeverOverwritesChangedDraft() throws {
        XCTAssertEqual(try DictationPolicy.completedDraft(original: "Bonjour", current: "Bonjour", transcript: " monde \n"), "Bonjour\nmonde")
        XCTAssertThrowsError(try DictationPolicy.completedDraft(original: "A", current: "B", transcript: "texte"))
        XCTAssertThrowsError(try DictationPolicy.completedDraft(original: "", current: "", transcript: " \n"))
        XCTAssertThrowsError(try DictationPolicy.completedDraft(original: "", current: "", transcript: "secret\0texte"))
        XCTAssertThrowsError(try DictationPolicy.completedDraft(original: "", current: "", transcript: String(repeating: "é", count: 4097)))
    }
    func testCancelInvalidatesPermissionAndRecognitionCallbacks() throws {
        var policy = DictationPolicy()
        let token = try policy.begin(foreground: true)
        policy.cancel()
        XCTAssertFalse(policy.accepts(token))
        XCTAssertFalse(policy.transition(.listening, token: token))
        XCTAssertFalse(policy.finish(token: token))
    }
}
