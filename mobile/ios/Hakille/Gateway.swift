import Foundation

struct HubCredentials: Codable, Sendable, CustomStringConvertible {
    var description: String { "HubCredentials(redacted)" }
    let endpoint: String
    let token: String
}

enum GatewayError: LocalizedError {
    case invalidEndpoint, invalidToken, invalidResponse, storage
    var errorDescription: String? {
        switch self {
        case .invalidEndpoint: return String(localized: "Invalid gateway URL. Use HTTPS, or HTTP on 127.0.0.1, without credentials, path, query or fragment.")
        case .invalidToken: return String(localized: "Invalid gateway token.")
        case .invalidResponse: return String(localized: "Gateway response invalid or too large. No automatic retry.")
        case .storage: return String(localized: "Secure credential storage failed.")
        }
    }
}

enum EndpointPolicy {
    static func canonical(_ raw: String) throws -> String {
        guard let components = URLComponents(string: raw), let host = components.host,
              components.user == nil, components.password == nil,
              components.query == nil, components.fragment == nil,
              components.path.isEmpty || components.path == "/",
              components.port == nil || (1...65535).contains(components.port!),
              components.scheme == "https" || (components.scheme == "http" && host == "127.0.0.1")
        else { throw GatewayError.invalidEndpoint }
        var canonical = components
        canonical.host = host.lowercased(); canonical.path = ""
        guard let url = canonical.url, !host.isEmpty else { throw GatewayError.invalidEndpoint }
        return url.absoluteString
    }
    static func validateToken(_ token: String) throws {
        guard (1...4096).contains(token.utf8.count), token.unicodeScalars.allSatisfy({ (33...126).contains($0.value) }) else { throw GatewayError.invalidToken }
    }
    static func validateHexBearerToken(_ token: String) throws {
        guard token.utf8.count == 64,
              token.unicodeScalars.allSatisfy({ ("0"..."9").contains($0) || ("a"..."f").contains($0) || ("A"..."F").contains($0) })
        else { throw GatewayError.invalidToken }
    }
    static func parsePairingQr(_ rawJson: String) throws -> HubCredentials {
        guard !rawJson.isEmpty, rawJson.utf8.count <= 8192,
              let data = rawJson.data(using: .utf8),
              let rawObj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(rawObj.keys) == Set(["endpoint", "token"]),
              let rawEndpoint = rawObj["endpoint"] as? String,
              let rawToken = rawObj["token"] as? String
        else { throw GatewayError.invalidEndpoint }
        let trimmedEndpoint = rawEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedToken = rawToken.trimmingCharacters(in: .whitespacesAndNewlines)
        let canonicalEndpoint = try canonical(trimmedEndpoint)
        try validateHexBearerToken(trimmedToken)
        return HubCredentials(endpoint: canonicalEndpoint, token: trimmedToken)
    }
}

private final class RefuseRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) { completionHandler(nil) }
}

struct GatewayTransport {
    static let maxBytes = 1024 * 1024
    func send(credentials: HubCredentials, frame: String, sessionID: String?) async throws -> String {
        let canonical = try EndpointPolicy.canonical(credentials.endpoint)
        try EndpointPolicy.validateToken(credentials.token)
        guard let url = URL(string: canonical + "/v1/request"), frame.utf8.count <= Self.maxBytes else { throw GatewayError.invalidResponse }
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 700; configuration.timeoutIntervalForResource = 700
        configuration.waitsForConnectivity = false; configuration.httpCookieStorage = nil; configuration.urlCache = nil
        let session = URLSession(configuration: configuration, delegate: RefuseRedirects(), delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 700)
        request.httpMethod = "POST"; request.httpBody = Data(frame.utf8); request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer " + credentials.token, forHTTPHeaderField: "Authorization")
        if let sessionID, !sessionID.isEmpty { request.setValue(sessionID, forHTTPHeaderField: "X-Hakille-Session") }
        let (bytes, response) = try await session.bytes(for: request)
        guard let response = response as? HTTPURLResponse, !(300...399).contains(response.statusCode),
              response.expectedContentLength <= Self.maxBytes else { throw GatewayError.invalidResponse }
        var data = Data(); data.reserveCapacity(min(Self.maxBytes, 16_384))
        for try await byte in bytes {
            guard data.count < Self.maxBytes else { throw GatewayError.invalidResponse }
            data.append(byte)
        }
        guard let json = String(data: data, encoding: .utf8) else { throw GatewayError.invalidResponse }
        return json // Rust consumes success and error envelopes, including issue_unknown.
    }

    func sendStream(credentials: HubCredentials, frame: String, sessionID: String?, onFrame: @escaping (String) -> Void) async throws {
        let canonical = try EndpointPolicy.canonical(credentials.endpoint)
        try EndpointPolicy.validateToken(credentials.token)
        guard let url = URL(string: canonical + "/v1/request"), frame.utf8.count <= Self.maxBytes else { throw GatewayError.invalidResponse }
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 700; configuration.timeoutIntervalForResource = 700
        configuration.waitsForConnectivity = false; configuration.httpCookieStorage = nil; configuration.urlCache = nil
        let session = URLSession(configuration: configuration, delegate: RefuseRedirects(), delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 700)
        request.httpMethod = "POST"; request.httpBody = Data(frame.utf8); request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        request.setValue("Bearer " + credentials.token, forHTTPHeaderField: "Authorization")
        if let sessionID, !sessionID.isEmpty { request.setValue(sessionID, forHTTPHeaderField: "X-Hakille-Session") }
        let (bytes, response) = try await session.bytes(for: request)
        guard let response = response as? HTTPURLResponse, (200...299).contains(response.statusCode) else { throw GatewayError.invalidResponse }
        for try await line in bytes.lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("data:") {
                let data = String(trimmed.dropFirst(5)).trimmingCharacters(in: .whitespaces)
                if data != "{}" {
                    onFrame(data)
                }
            } else if trimmed.hasPrefix("event: done") {
                break
            }
        }
    }
}
