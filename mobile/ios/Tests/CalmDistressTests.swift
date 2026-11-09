import XCTest
@testable import Hakille

final class CalmDistressTests: XCTestCase {

    func testCalmDistressCopyMatchesSpec() {
        let expected = "Je reste là. Un geste pour appeler ton contact, un autre pour les urgences."
        XCTAssertEqual(CalmDistressConstants.calmCopy, expected)
        XCTAssertTrue(CalmDistressConstants.calmCopy.contains("contact"))
        XCTAssertTrue(CalmDistressConstants.calmCopy.contains("urgences"))
    }

    func testEmergencyNumber() {
        XCTAssertEqual(CalmDistressConstants.emergencyNumber, "112")
    }

    func testCalmDistressContainsNoBraise() {
        let braise = HakilleTokens.colorBraiseDarkHex
        XCTAssertNotEqual(braise, HakilleTokens.colorSurface0DarkHex)
        XCTAssertNotEqual(braise, HakilleTokens.colorSurface1DarkHex)
        XCTAssertNotEqual(braise, HakilleTokens.colorSurface2DarkHex)
        XCTAssertNotEqual(braise, HakilleTokens.colorTexte1DarkHex)
        XCTAssertNotEqual(braise, HakilleTokens.colorTexte2DarkHex)
    }
}
