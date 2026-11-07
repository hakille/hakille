import XCTest
@testable import Hakille

final class BoundaryTests: XCTestCase {
    func testURLBoundaries() throws {
        for url in ["http://example.com", "http://10.0.2.2:17840", "http://localhost:17840", "https://user:secret@example.com", "https://example.com?token=x", "https://example.com#fragment", "https://example.com/v1/request", "file:///tmp/token", "https://example.com:0"] {
            XCTAssertThrowsError(try EndpointPolicy.canonical(url), url)
        }
        XCTAssertEqual(try EndpointPolicy.canonical("https://EXAMPLE.com/"), "https://example.com")
        XCTAssertEqual(try EndpointPolicy.canonical("http://127.0.0.1:17840"), "http://127.0.0.1:17840")
        XCTAssertThrowsError(try EndpointPolicy.validateToken("secret\r\nInjected: header"))
    }
    func testUInt64ProtocolIDNeverBecomesDouble() throws {
        let value = try JSONValue.parse("{\"id\":18446744073709551614}")
        XCTAssertEqual(value["id"].id, UInt64.max - 1)
        XCTAssertEqual(try JSONValue.parse(value.encoded())["id"].id, UInt64.max - 1)
    }
    func testCredentialsDescriptionIsRedacted() {
        let credential = HubCredentials(endpoint: "https://example.com", token: "secret-token")
        XCTAssertFalse(String(describing: credential).contains("secret-token"))
    }
    func testCardMoneyKeepsMicroUnitsAndCurrency() throws {
        let value = try JSONValue.parse("{\"micros\":1,\"currency\":\"EUR\"}")
        XCTAssertEqual(CardPresentation.money(value, locale: Locale(identifier: "en_US")), "0.000001 EUR")
        let cloud = try JSONValue.parse("{\"Cloud\":{\"provider\":\"test-provider\"}}")
        XCTAssertTrue(CardPresentation.destination(cloud).contains("test-provider"))
    }

}
