import XCTest
import UIKit
@testable import Hakille

final class NativeFeaturesTests: XCTestCase {
    func testVoiceModeIsExplicitAndReplyIdentityIsConsumedOnce() {
        var policy = VoiceConversationPolicy()
        let reply = VoiceReplyIdentity(context: 7, request: 8)
        XCTAssertFalse(policy.consume(reply, foreground: true, context: 7, request: 8))
        policy.enable(); policy.arm(context: 7)
        XCTAssertFalse(policy.consume(reply, foreground: false, context: 7, request: 8))
        XCTAssertFalse(policy.consume(reply, foreground: true, context: 9, request: 8))
        XCTAssertFalse(policy.consume(reply, foreground: true, context: 7, request: 9))
        XCTAssertTrue(policy.consume(reply, foreground: true, context: 7, request: 8))
        XCTAssertFalse(policy.consume(reply, foreground: true, context: 7, request: 8))
    }
    func testOldVoiceGenerationCannotSubmitOrSpeakAfterOptOutAndNewTurn() {
        var policy = VoiceConversationPolicy(); policy.enable(); policy.arm(context: 10)
        let generation = policy.generation
        policy.disable(); policy.enable(); policy.arm(context: 11)
        XCTAssertFalse(policy.accepts(context: 10, generation: generation))
        XCTAssertFalse(policy.consume(VoiceReplyIdentity(context: 10, request: 12), foreground: true, context: 11, request: 12))
    }
    func testTranscriptValidationDoesNotAcceptEmptyControlOrOversizedInput() throws {
        XCTAssertEqual(try VoiceConversationPolicy.validatedTranscript(" Bonjour \n"), "Bonjour")
        XCTAssertThrowsError(try VoiceConversationPolicy.validatedTranscript(" \n"))
        XCTAssertThrowsError(try VoiceConversationPolicy.validatedTranscript("a\0b"))
        XCTAssertThrowsError(try VoiceConversationPolicy.validatedTranscript(String(repeating: "é", count: 4097)))
    }
    func testLocationPolicyAcceptsApproximateFixButRejectsInvalidOrStaleValues() {
        let now = Date()
        XCTAssertTrue(PhoneCapabilityPolicy.accepts(PhoneLocation(latitude: 48, longitude: 2, accuracy: 5000, timestamp: now, approximate: true), now: now))
        for location in [
            PhoneLocation(latitude: .nan, longitude: 2, accuracy: 5, timestamp: now),
            PhoneLocation(latitude: 48, longitude: 181, accuracy: 5, timestamp: now),
            PhoneLocation(latitude: 48, longitude: 2, accuracy: .nan, timestamp: now),
            PhoneLocation(latitude: 48, longitude: 2, accuracy: -1, timestamp: now),
            PhoneLocation(latitude: 48, longitude: 2, accuracy: 5, timestamp: now.addingTimeInterval(-16)),
            PhoneLocation(latitude: 48, longitude: 2, accuracy: 5, timestamp: now.addingTimeInterval(10))
        ] { XCTAssertFalse(PhoneCapabilityPolicy.accepts(location, now: now)) }
    }
    func testExternalDestinationsAreExactAndRejectCommandsOrUnlistedURLs() {
        for destination in ExternalDestination.allCases { XCTAssertTrue(ExternalLaunchPolicy.allowed(destination.url)) }
        for value in ["javascript:alert(1)", "tel:1234", "sms:1234", "https://evil.example", "https://github.com.evil.example/hakille/hakille#readme", "https://github.com/hakille/hakille?token=secret", "maps://?q=private", "file:///tmp/data"] {
            XCTAssertFalse(ExternalLaunchPolicy.allowed(URL(string: value)!), value)
        }
    }
    func testDocumentReaderReadsOnlyChosenRegularBoundedFile() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("texte.txt")
        try Data("Bonjour".utf8).write(to: file)
        let reader = PhoneDocumentReader()
        let selected = try await reader.read(file)
        XCTAssertEqual(selected.name, "texte.txt"); XCTAssertEqual(selected.text, "Bonjour")
        let link = directory.appendingPathComponent("lien.txt")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: file)
        do { _ = try await reader.read(link); XCTFail("Symlink accepté") } catch { }
        let handle = try FileHandle(forWritingTo: file)
        try handle.truncate(atOffset: UInt64(PhoneCapabilityPolicy.maximumDocumentBytes + 1)); try handle.close()
        do { _ = try await reader.read(file); XCTFail("Fichier trop grand accepté") } catch { }
    }
    func testDocumentReadChecksCancellationBeforeOpeningFile() async throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data("local".utf8).write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        let reader = PhoneDocumentReader(); let gate = CancellationGate()
        let task = Task { await gate.wait(); return try await reader.read(file) }
        task.cancel(); await gate.release()
        do { _ = try await task.value; XCTFail("Lecture annulée acceptée") }
        catch is CancellationError { }
        catch { XCTFail("CancellationError attendue, sans ouverture du fichier") }
    }
    @MainActor func testLateDocumentPickerCallbackIsIgnoredAfterBackground() {
        let phone = PhoneCapabilities(); phone.foreground(true, background: false); phone.requestDocument()
        let token = phone.documentToken
        phone.foreground(false, background: true)
        if let token { phone.receiveDocument(URL(fileURLWithPath: "/ignored"), token: token) }
        XCTAssertNil(phone.document); XCTAssertFalse(phone.documentPending); XCTAssertFalse(phone.documentPickerPresented)
    }
    @MainActor func testBundledBrandAndFontAreActuallyLoadable() {
        XCTAssertNotNil(UIImage(named: "HakilleMark"))
        XCTAssertNotNil(UIFont(name: "Inter-Regular", size: 16))
    }
    func testNativeVoiceCannotArmWithoutConfirmedHub() throws {
        let bridge = MobileBridge()
        let result = try JSONValue.parse(bridge.dispatchJson(actionJson: "{\"type\":\"begin_voice\"}"))
        XCTAssertEqual(result["ok"], .bool(false))
        XCTAssertTrue(result["snapshot"]["voice_context_id"].isNull)
        XCTAssertTrue(result["request"].isNull)
    }
}

private actor CancellationGate {
    private var released = false
    private var continuation: CheckedContinuation<Void, Never>?
    func wait() async {
        if released { return }
        await withCheckedContinuation { continuation = $0 }
    }
    func release() { released = true; continuation?.resume(); continuation = nil }
}
