import XCTest
@testable import Hakille

private final class MemoryCheckpointStore: MobileStatePersistence, @unchecked Sendable {
    private let lock = NSLock()
    private var value: String?
    private var writes: [String] = []
    private var failWrites = false
    init(_ value: String? = nil) { self.value = value }
    func load() throws -> String? {
        XCTAssertFalse(Thread.isMainThread, "Les I/O ne doivent pas bloquer MainActor")
        lock.lock(); defer { lock.unlock() }; return value
    }
    func save(_ checkpoint: String) throws {
        XCTAssertFalse(Thread.isMainThread, "fsync ne doit pas bloquer MainActor")
        lock.lock(); defer { lock.unlock() }
        if failWrites { throw MobileStateError.unavailable }
        value = checkpoint; writes.append(checkpoint)
    }
    func rejectWrites() { lock.lock(); failWrites = true; lock.unlock() }
    func recorded() -> [String] { lock.lock(); defer { lock.unlock() }; return writes }
}

final class RecoveryTests: XCTestCase {
    private let checkpoint = "{\"schema_version\":2,\"draft\":\"À relire\",\"next_request_id\":9007199254740993,\"uncertain_operations\":[{\"request_id\":9007199254740992,\"kind\":\"remember\",\"phase\":\"in_flight\"}],\"acknowledged_operations\":[]}"

    func testActualUniFFIBridgeDispatchAndDurableRecovery() throws {
        // Ce test charge réellement la bibliothèque Rust liée à l'application.
        let bridge = MobileBridge()
        let restored = try JSONValue.parse(bridge.restoreDraftJson(draftJson: checkpoint))
        XCTAssertEqual(restored["ok"], .bool(true))
        XCTAssertEqual(restored["snapshot"]["draft"].text, "À relire")
        XCTAssertEqual(restored["snapshot"]["recovery_required"], .bool(true))
        XCTAssertEqual(restored["snapshot"]["microphone_enabled"], .bool(false))
        XCTAssertTrue(restored["request"].isNull)
        XCTAssertTrue(restored["snapshot"]["pending"].isNull)
        let read = try JSONValue.parse(bridge.dispatchJson(actionJson: "{\"type\":\"acknowledge_unknown\",\"request_id\":9007199254740992}"))
        XCTAssertEqual(read["ok"], .bool(true))
        XCTAssertTrue(read["request"].isNull)
        let saved = try JSONValue.parse(bridge.draftJson())
        XCTAssertEqual(saved["next_request_id"].id, 9007199254740993)
        XCTAssertEqual(saved["acknowledged_operations"].array.first?["request_id"].id, 9007199254740992)
        let restarted = MobileBridge()
        let again = try JSONValue.parse(restarted.restoreDraftJson(draftJson: saved.encoded()))
        XCTAssertEqual(again["ok"], .bool(true))
        XCTAssertEqual(again["snapshot"]["acknowledged_operations"].array.count, 1)
        XCTAssertTrue(again["request"].isNull)
    }
    func testAtomicCheckpointRoundTripAndRejectedWritePreservesPriorFile() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = MobileStateStore(directory: directory)
        XCTAssertNil(try store.load())
        try store.save(checkpoint)
        XCTAssertEqual(try store.load(), checkpoint)
        XCTAssertThrowsError(try store.save("{\"schema_version\":1,\"draft\":\"\",\"token\":\"not-allowed\"}"))
        XCTAssertEqual(try store.load(), checkpoint)
        let replacement = "{\"schema_version\":1,\"draft\":\"nouveau\"}"
        try store.save(replacement)
        XCTAssertEqual(try store.load(), replacement)
    }
    func testCheckpointRejectsSymlinkAndOversizedInput() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let target = directory.appendingPathComponent("other.json")
        try Data(checkpoint.utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: directory.appendingPathComponent("checkpoint.json"), withDestinationURL: target)
        XCTAssertThrowsError(try MobileStateStore(directory: directory).load())
        XCTAssertThrowsError(try MobileStateStore.validate(String(repeating: "x", count: 65537)))
    }
    @MainActor func testInvalidSavedStateIsNeverOverwrittenAndNoRequestIsCreated() async throws {
        let store = MemoryCheckpointStore("invalid")
        let model = MobileModel(stateStore: store)
        try await waitUntil { !model.storageError.isEmpty }
        model.dispatch("set_hub", ["hub_id": .string("https://example.com")])
        model.dispatch("submit")
        XCTAssertFalse(model.persistenceReady)
        XCTAssertTrue(model.snapshot["pending"].isNull)
        XCTAssertTrue(store.recorded().isEmpty)
    }
    @MainActor func testDisconnectKeepsUnknownOperationsAndDraftWithoutCredentials() async throws {
        let store = MemoryCheckpointStore(checkpoint)
        let model = MobileModel(stateStore: store)
        try await waitUntil { model.persistenceReady && !store.recorded().isEmpty }
        model.foreground(true)
        model.disconnect()
        XCTAssertEqual(model.snapshot["draft"].text, "À relire")
        XCTAssertEqual(model.snapshot["recovery_required"], .bool(true))
        XCTAssertEqual(model.snapshot["uncertain_operations"].array.count, 1)
        XCTAssertTrue(model.snapshot["pending"].isNull)
        XCTAssertTrue(model.snapshot["hub_id"].isNull)
    }
    @MainActor func testStorageFailureBeforeEmissionClearsPendingWithoutInventingUnknownEffect() async throws {
        let store = MemoryCheckpointStore()
        let model = MobileModel(stateStore: store)
        try await waitUntil { model.persistenceReady && !store.recorded().isEmpty }
        model.foreground(true)
        model.dispatch("set_hub", ["hub_id": .string("https://example.com")])
        model.dispatch("set_draft", ["text": .string("test")])
        store.rejectWrites()
        model.dispatch("submit")
        try await waitUntil { !model.storageError.isEmpty && model.snapshot["pending"].isNull }
        XCTAssertEqual(model.snapshot["recovery_required"], .bool(false))
        XCTAssertTrue(model.snapshot["uncertain_operations"].array.isEmpty)
        XCTAssertEqual(model.snapshot["outcomes"].array.last?["state"].text, "failed")
    }
    @MainActor func testCancelBeforeCheckpointCompletesDoesNotInventRemoteEffect() async throws {
        let store = MemoryCheckpointStore()
        let model = MobileModel(stateStore: store)
        try await waitUntil { model.persistenceReady && !store.recorded().isEmpty }
        model.foreground(true)
        model.dispatch("set_hub", ["hub_id": .string("https://example.com")])
        model.dispatch("set_draft", ["text": .string("test")])
        model.dispatch("submit")
        XCTAssertFalse(model.snapshot["pending"].isNull)
        // Aucun yield : le transport n'a pas pu commencer sur MainActor.
        model.cancelRequest()
        XCTAssertTrue(model.snapshot["pending"].isNull)
        XCTAssertTrue(model.snapshot["uncertain_operations"].array.isEmpty)
        XCTAssertEqual(model.snapshot["outcomes"].array.last?["state"].text, "failed")
    }
    @MainActor private func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0..<5000 {
            if condition() { return }
            try await Task.sleep(nanoseconds: 1_000_000)
        }
        XCTFail("Condition asynchrone non satisfaite")
        throw MobileStateError.unavailable
    }
}
