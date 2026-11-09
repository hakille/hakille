import XCTest
@testable import Hakille

final class DesignSystemTokensTests: XCTestCase {

    func testOledSurfaceHexValues() {
        XCTAssertEqual(HakilleTokens.colorSurface0DarkHex, "#0B0C0F")
        XCTAssertEqual(HakilleTokens.colorSurface0LightHex, "#F6F4F0")
        XCTAssertEqual(HakilleTokens.colorSurface1DarkHex, "#121419")
        XCTAssertEqual(HakilleTokens.colorSurface1LightHex, "#FFFFFF")
        XCTAssertEqual(HakilleTokens.colorSurface2DarkHex, "#1A1D24")
        XCTAssertEqual(HakilleTokens.colorSurface2LightHex, "#EDEAE4")

        XCTAssertEqual(HakilleTokens.colorLigneDarkHex, "#262A33")
        XCTAssertEqual(HakilleTokens.colorLigneLightHex, "#DAD6CE")
        XCTAssertEqual(HakilleTokens.colorLueurDarkHex, "#F4EBDD")
        XCTAssertEqual(HakilleTokens.colorLueurLightHex, "#14161B")

        XCTAssertEqual(HakilleTokens.colorBraiseDarkHex, "#FF7A2F")
        XCTAssertEqual(HakilleTokens.colorBraiseLightHex, "#C2410C")
        XCTAssertEqual(HakilleTokens.colorBraiseHaloDarkHex, "#FF7A2F47")
    }

    func testWireCountsAndMotion() {
        XCTAssertEqual(HakilleTokens.presenceWireCountFull, 21)
        XCTAssertEqual(HakilleTokens.presenceWireCountCompact, 7)
        XCTAssertEqual(HakilleTokens.presenceWireDecayMs, 80)
        XCTAssertEqual(HakilleTokens.presenceWireGapPx, 3.0)
        XCTAssertEqual(HakilleTokens.presenceWireWidthPx, 2.0)

        // Test wireCount helper threshold (< 24pt)
        XCTAssertEqual(wireCountForSize(size: 16), 7)
        XCTAssertEqual(wireCountForSize(size: 23), 7)
        XCTAssertEqual(wireCountForSize(size: 24), 21)
        XCTAssertEqual(wireCountForSize(size: 176), 21)
    }

    func testMinimumTouchTargets() {
        XCTAssertEqual(HakilleTokens.layoutTouchPtAndroid, 48)
        XCTAssertEqual(HakilleTokens.layoutTouchPtIos, 44)
    }

    func testHapticTickDuration() {
        XCTAssertEqual(HakilleTokens.motionHapticMsTick, 10)
    }

    func testGlassmorphicBlurTokens() {
        XCTAssertEqual(HakilleTokens.blurHudAcrylic, 30)
        XCTAssertEqual(HakilleTokens.blurPanel, 24)
        XCTAssertEqual(HakilleTokens.blurGlow, 16)
    }
}
