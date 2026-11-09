import XCTest
@testable import Hakille

final class PairingPrivacyAndGuestTests: XCTestCase {

    func testQrPairingParsesValid64HexAndRedactsCredentials() throws {
        let validHex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        let qrJson = """
        {"endpoint":"https://Hub.Example.COM:8443/","token":"\(validHex)"}
        """
        let creds = try EndpointPolicy.parsePairingQr(qrJson)
        XCTAssertEqual(creds.endpoint, "https://hub.example.com:8443")
        XCTAssertEqual(creds.token, validHex)
        XCTAssertEqual(String(describing: creds), "HubCredentials(redacted)")
        XCTAssertFalse(String(describing: creds).contains(validHex))

        // Reject 63-char hex
        let shortHex = String(validHex.prefix(63))
        XCTAssertThrowsError(try EndpointPolicy.parsePairingQr("""
        {"endpoint":"https://hub.example.com","token":"\(shortHex)"}
        """))

        // Reject non-hex
        let badHex = "g" + String(validHex.dropFirst())
        XCTAssertThrowsError(try EndpointPolicy.parsePairingQr("""
        {"endpoint":"https://hub.example.com","token":"\(badHex)"}
        """))

        // Reject HTTP on non-loopback
        XCTAssertThrowsError(try EndpointPolicy.parsePairingQr("""
        {"endpoint":"http://192.168.1.50:8080","token":"\(validHex)"}
        """))
    }

    func testCameraScaleAndDocumentSha256Integrity() {
        XCTAssertEqual(PhoneCapabilityPolicy.maximumDocumentBytes, 8 * 1024 * 1024)
        XCTAssertEqual(PhoneCapabilityPolicy.maximumPhotoBytes, 8 * 1024 * 1024)
        XCTAssertEqual(PhoneCapabilityPolicy.maximumPreviewChars, 4000)
        XCTAssertEqual(PhoneCapabilityPolicy.maximumCameraLongEdgePx, 1536)
        XCTAssertEqual(PhoneCapabilityPolicy.maximumCameraTotalPixels, 4_000_000)

        let scale = PhoneCapabilityPolicy.computeCameraScale(width: 2000, height: 1500)
        XCTAssertEqual(scale, 1536.0 / 2000.0, accuracy: 1e-6)

        let doc = LocalDocument(name: "test.txt", data: Data("Bonjour".utf8), text: "Bonjour")
        XCTAssertEqual(doc.sha256.count, 64)
    }

    func testGpsRejectsMockLocationAndStaleSample() {
        let now = Date()
        let validFix = PhoneLocation(latitude: 48.8566, longitude: 2.3522, accuracy: 12.0, timestamp: now, approximate: false, mock: false)
        XCTAssertTrue(PhoneCapabilityPolicy.accepts(validFix, now: now))

        let mockFix = PhoneLocation(latitude: 48.8566, longitude: 2.3522, accuracy: 5.0, timestamp: now, approximate: false, mock: true)
        XCTAssertFalse(PhoneCapabilityPolicy.accepts(mockFix, now: now))
    }

    func testPrivacyGuardShoulderSurfingThresholdAndDebounce() {
        XCTAssertFalse(PrivacyGuardHelper.shouldTriggerVeil(peopleCount: 0))
        XCTAssertFalse(PrivacyGuardHelper.shouldTriggerVeil(peopleCount: 1))
        XCTAssertTrue(PrivacyGuardHelper.shouldTriggerVeil(peopleCount: 2))

        let filter = PrivacyGuardDebounceFilter(requiredConsecutive: 3)
        XCTAssertFalse(filter.update(peopleCount: 1))
        XCTAssertFalse(filter.update(peopleCount: 2))
        XCTAssertFalse(filter.update(peopleCount: 1))
        XCTAssertFalse(filter.update(peopleCount: 2))
        XCTAssertFalse(filter.update(peopleCount: 2))
        XCTAssertTrue(filter.update(peopleCount: 2))
    }

    func testGuestPassRingFraction1PxInvariantAndCapabilityConfinement() {
        XCTAssertEqual(GuestPassHelper.guestRingPx, 1)
        XCTAssertEqual(GuestPassHelper.computeRingFraction(elapsedSecs: 0, durationSecs: 3600), 1.0, accuracy: 1e-6)
        XCTAssertEqual(GuestPassHelper.computeRingFraction(elapsedSecs: 1800, durationSecs: 3600), 0.5, accuracy: 1e-6)
        XCTAssertEqual(GuestPassHelper.computeRingFraction(elapsedSecs: 3600, durationSecs: 3600), 0.0, accuracy: 1e-6)
        XCTAssertTrue(GuestPassHelper.isExpired(elapsedSecs: 3600, durationSecs: 3600))
        XCTAssertTrue(GuestPassHelper.isExpired(elapsedSecs: 0, durationSecs: 0))

        XCTAssertTrue(GuestPassHelper.isAuthorizedForGuest(power: "wifi.connect", expired: false, requestedAction: "wifi.connect"))
        XCTAssertFalse(GuestPassHelper.isAuthorizedForGuest(power: "wifi.connect", expired: false, requestedAction: "wallet.spend"))
        XCTAssertFalse(GuestPassHelper.isAuthorizedForGuest(power: "wifi.connect", expired: false, requestedAction: "memory.forget"))
        XCTAssertFalse(GuestPassHelper.isAuthorizedForGuest(power: "wifi.connect", expired: true, requestedAction: "wifi.connect"))
    }
}
