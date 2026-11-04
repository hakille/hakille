import Foundation
import Security

/// The endpoint and its token are one Keychain item, excluded from iCloud migration.
actor CredentialStore {
    private var revision: UInt64 = 0
    private let service = "dev.hakille.mobile.gateway.v1"
    private var query: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
         kSecAttrAccount as String: "hub", kSecAttrSynchronizable as String: false]
    }
    func save(_ credentials: HubCredentials, generation: UInt64) throws {
        guard generation >= revision else { return }; revision = generation
        let data = try JSONEncoder().encode(credentials)
        let attributes: [String: Any] = [kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]
        let status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            guard SecItemAdd(query.merging(attributes, uniquingKeysWith: { _, new in new }) as CFDictionary, nil) == errSecSuccess else { throw GatewayError.storage }
        } else if status != errSecSuccess { throw GatewayError.storage }
    }
    func load() throws -> HubCredentials? {
        var request = query; request[kSecReturnData as String] = true; request[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(request as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data else { throw GatewayError.storage }
        return try JSONDecoder().decode(HubCredentials.self, from: data)
    }
    func clear(generation: UInt64) throws {
        guard generation >= revision else { return }; revision = generation
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw GatewayError.storage }
    }
}
