import Darwin
import Foundation

protocol MobileStatePersistence: Sendable {
    func load() throws -> String?
    func save(_ checkpoint: String) throws
}

enum MobileStateError: Error { case unavailable, invalidCheckpoint }

/// Petit checkpoint privé du conteneur iOS ; ni credentials, ni session réseau.
/// Les I/O sont synchrones et bornées (64 Kio), avant toute émission HTTP.
final class MobileStateStore: MobileStatePersistence {
    static let maximumBytes = 64 * 1024
    private let suppliedDirectory: URL?
    init(directory: URL? = nil) { suppliedDirectory = directory }

    private func directory() throws -> URL {
        let root: URL
        if let suppliedDirectory { root = suppliedDirectory }
        else { root = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true).appendingPathComponent("MobileRecovery", isDirectory: true) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700, .protectionKey: FileProtectionType.complete])
        var values = URLResourceValues(); values.isExcludedFromBackup = true
        var excluded = root; try excluded.setResourceValues(values)
        return root
    }
    func load() throws -> String? {
        let url = try directory().appendingPathComponent("checkpoint.json")
        let fd = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK)
        if fd < 0 {
            if errno == ENOENT { return nil }
            throw MobileStateError.unavailable
        }
        defer { Darwin.close(fd) }
        var metadata = stat()
        guard fstat(fd, &metadata) == 0, (metadata.st_mode & S_IFMT) == S_IFREG,
              metadata.st_size >= 0, metadata.st_size <= off_t(Self.maximumBytes) else { throw MobileStateError.invalidCheckpoint }
        var data = Data(); var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = Darwin.read(fd, &buffer, 4096)
            if count < 0 { if errno == EINTR { continue }; throw MobileStateError.unavailable }
            if count == 0 { break }
            guard data.count + count <= Self.maximumBytes else { throw MobileStateError.invalidCheckpoint }
            data.append(contentsOf: buffer.prefix(count))
        }
        guard let checkpoint = String(data: data, encoding: .utf8) else { throw MobileStateError.invalidCheckpoint }
        try Self.validate(checkpoint)
        return checkpoint
    }
    func save(_ checkpoint: String) throws {
        try Self.validate(checkpoint)
        let root = try directory()
        let temporary = root.appendingPathComponent(".checkpoint-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: temporary) }
        // Un nouveau fichier protégé est publié par rename atomique. Les
        // barrières durables précèdent le retour autorisant l'émission HTTP.
        try Data(checkpoint.utf8).write(to: temporary, options: [.withoutOverwriting, .completeFileProtection])
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: temporary.path)
        let fd = Darwin.open(temporary.path, O_WRONLY | O_NOFOLLOW)
        guard fd >= 0 else { throw MobileStateError.unavailable }
        let synchronized = fsync(fd) == 0; Darwin.close(fd)
        guard synchronized, Darwin.rename(temporary.path, root.appendingPathComponent("checkpoint.json").path) == 0 else { throw MobileStateError.unavailable }
        let directoryFD = Darwin.open(root.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW)
        guard directoryFD >= 0 else { throw MobileStateError.unavailable }
        defer { Darwin.close(directoryFD) }
        guard fsync(directoryFD) == 0 else { throw MobileStateError.unavailable }
    }
    static func validate(_ checkpoint: String) throws {
        guard checkpoint.utf8.count <= maximumBytes, let value = try? JSONValue.parse(checkpoint),
              case .string(let draft) = value["draft"], draft.utf8.count <= DictationPolicy.maximumDraftBytes else { throw MobileStateError.invalidCheckpoint }
        let allowed: Set<String>
        switch value["schema_version"].id {
        case 1: allowed = ["schema_version", "draft"]
        case 2: allowed = ["schema_version", "draft", "next_request_id", "uncertain_operations", "acknowledged_operations"]
        default: throw MobileStateError.invalidCheckpoint
        }
        guard Set(value.object.keys) == allowed else { throw MobileStateError.invalidCheckpoint }
        if value["schema_version"].id == 2 {
            guard value["next_request_id"].id != nil,
                  case .array(let uncertain) = value["uncertain_operations"], uncertain.count <= 32,
                  case .array(let acknowledged) = value["acknowledged_operations"], acknowledged.count <= 32 else { throw MobileStateError.invalidCheckpoint }
            for operation in uncertain {
                guard Set(operation.object.keys) == ["request_id", "kind", "phase"] else { throw MobileStateError.invalidCheckpoint }
            }
            for operation in acknowledged {
                guard Set(operation.object.keys) == ["request_id", "kind"] else { throw MobileStateError.invalidCheckpoint }
            }
        }
    }
}

/// L'acteur exécute les appels bloquants hors MainActor. L'hôte chaîne les
/// tâches pour conserver l'ordre exact des checkpoints, sans réordonnancement.
actor MobileStateIO {
    private let store: MobileStatePersistence
    init(_ store: MobileStatePersistence) { self.store = store }
    func load() throws -> String? { try store.load() }
    func save(_ checkpoint: String) throws { try store.save(checkpoint) }
}
