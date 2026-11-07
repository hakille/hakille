// Généré par `cargo xtask tokens export-swift` depuis tokens/*.toml. NE PAS ÉDITER.
// braise + surface : source unique tokens/color.toml.
import SwiftUI

#if canImport(SwiftUI)
extension Color {
public init(hex: String) {
let clean = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
var int: UInt64 = 0
Scanner(string: clean).scanHexInt64(&int)
let a, r, g, b: UInt64
switch clean.count {
case 6: // RGB (24-bit)
(a, r, g, b) = (255, (int >> 16) & 0xFF, (int >> 8) & 0xFF, int & 0xFF)
case 8: // RGBA (32-bit)
(r, g, b, a) = ((int >> 24) & 0xFF, (int >> 16) & 0xFF, (int >> 8) & 0xFF, int & 0xFF)
default:
(a, r, g, b) = (255, 0, 0, 0)
}
self.init(
.sRGB,
red: Double(r) / 255.0,
green: Double(g) / 255.0,
blue: Double(b) / 255.0,
opacity: Double(a) / 255.0
)
}

public init(rgb: UInt32) {
self.init(
.sRGB,
red: Double((rgb >> 16) & 0xFF) / 255.0,
green: Double((rgb >> 8) & 0xFF) / 255.0,
blue: Double(rgb & 0xFF) / 255.0,
opacity: 1.0
)
}
}

public struct CubicBezier: Sendable, Equatable {
public let x1: Double
public let y1: Double
public let x2: Double
public let y2: Double
public let spec: String

public init(_ x1: Double, _ y1: Double, _ x2: Double, _ y2: Double, spec: String) {
self.x1 = x1
self.y1 = y1
self.x2 = x2
self.y2 = y2
self.spec = spec
}

public func animation(duration: Double) -> Animation {
Animation.timingCurve(x1, y1, x2, y2, duration: duration)
}

public var animation: Animation {
Animation.timingCurve(x1, y1, x2, y2)
}
}
#endif

public enum HakilleTokens {
    public static let colorAvertissementDark = Color(hex: "#FBBF24")
    public static let colorAvertissementDarkHex: String = "#FBBF24"
    public static let colorAvertissementDarkRgb: UInt32 = 0xFBBF24
    public static let colorAvertissementLight = Color(hex: "#D97706")
    public static let colorAvertissementLightHex: String = "#D97706"
    public static let colorAvertissementLightRgb: UInt32 = 0xD97706
    public static let colorBraiseDark = Color(hex: "#FF7A2F")
    public static let colorBraiseDarkHex: String = "#FF7A2F"
    public static let colorBraiseDarkRgb: UInt32 = 0xFF7A2F
    public static let colorBraiseLight = Color(hex: "#C2410C")
    public static let colorBraiseLightHex: String = "#C2410C"
    public static let colorBraiseLightRgb: UInt32 = 0xC2410C
    public static let colorBraiseHaloDark = Color(hex: "#FF7A2F47")
    public static let colorBraiseHaloDarkHex: String = "#FF7A2F47"
    public static let colorBraiseHaloDarkRgb: UInt32 = 0xFF7A2F47
    public static let colorBraiseHaloLight = Color(hex: "#C2410C33")
    public static let colorBraiseHaloLightHex: String = "#C2410C33"
    public static let colorBraiseHaloLightRgb: UInt32 = 0xC2410C33
    public static let colorErreurDark = Color(hex: "#FF5A5A")
    public static let colorErreurDarkHex: String = "#FF5A5A"
    public static let colorErreurDarkRgb: UInt32 = 0xFF5A5A
    public static let colorErreurLight = Color(hex: "#DC2626")
    public static let colorErreurLightHex: String = "#DC2626"
    public static let colorErreurLightRgb: UInt32 = 0xDC2626
    public static let colorLigneDark = Color(hex: "#262A33")
    public static let colorLigneDarkHex: String = "#262A33"
    public static let colorLigneDarkRgb: UInt32 = 0x262A33
    public static let colorLigneLight = Color(hex: "#DAD6CE")
    public static let colorLigneLightHex: String = "#DAD6CE"
    public static let colorLigneLightRgb: UInt32 = 0xDAD6CE
    public static let colorLueurDark = Color(hex: "#F4EBDD")
    public static let colorLueurDarkHex: String = "#F4EBDD"
    public static let colorLueurDarkRgb: UInt32 = 0xF4EBDD
    public static let colorLueurLight = Color(hex: "#14161B")
    public static let colorLueurLightHex: String = "#14161B"
    public static let colorLueurLightRgb: UInt32 = 0x14161B
    public static let colorSuccesDark = Color(hex: "#34D399")
    public static let colorSuccesDarkHex: String = "#34D399"
    public static let colorSuccesDarkRgb: UInt32 = 0x34D399
    public static let colorSuccesLight = Color(hex: "#059669")
    public static let colorSuccesLightHex: String = "#059669"
    public static let colorSuccesLightRgb: UInt32 = 0x059669
    public static let colorSurBraiseDark = Color(hex: "#0B0C0F")
    public static let colorSurBraiseDarkHex: String = "#0B0C0F"
    public static let colorSurBraiseDarkRgb: UInt32 = 0x0B0C0F
    public static let colorSurBraiseLight = Color(hex: "#FFFFFF")
    public static let colorSurBraiseLightHex: String = "#FFFFFF"
    public static let colorSurBraiseLightRgb: UInt32 = 0xFFFFFF
    public static let colorSurface0Dark = Color(hex: "#0B0C0F")
    public static let colorSurface0DarkHex: String = "#0B0C0F"
    public static let colorSurface0DarkRgb: UInt32 = 0x0B0C0F
    public static let colorSurface0Light = Color(hex: "#F6F4F0")
    public static let colorSurface0LightHex: String = "#F6F4F0"
    public static let colorSurface0LightRgb: UInt32 = 0xF6F4F0
    public static let colorSurface1Dark = Color(hex: "#121419")
    public static let colorSurface1DarkHex: String = "#121419"
    public static let colorSurface1DarkRgb: UInt32 = 0x121419
    public static let colorSurface1Light = Color(hex: "#FFFFFF")
    public static let colorSurface1LightHex: String = "#FFFFFF"
    public static let colorSurface1LightRgb: UInt32 = 0xFFFFFF
    public static let colorSurface1ActiveDark = Color(hex: "#0E1014")
    public static let colorSurface1ActiveDarkHex: String = "#0E1014"
    public static let colorSurface1ActiveDarkRgb: UInt32 = 0x0E1014
    public static let colorSurface1ActiveLight = Color(hex: "#E4DFD6")
    public static let colorSurface1ActiveLightHex: String = "#E4DFD6"
    public static let colorSurface1ActiveLightRgb: UInt32 = 0xE4DFD6
    public static let colorSurface1HoverDark = Color(hex: "#181B22")
    public static let colorSurface1HoverDarkHex: String = "#181B22"
    public static let colorSurface1HoverDarkRgb: UInt32 = 0x181B22
    public static let colorSurface1HoverLight = Color(hex: "#F0ECE4")
    public static let colorSurface1HoverLightHex: String = "#F0ECE4"
    public static let colorSurface1HoverLightRgb: UInt32 = 0xF0ECE4
    public static let colorSurface2Dark = Color(hex: "#1A1D24")
    public static let colorSurface2DarkHex: String = "#1A1D24"
    public static let colorSurface2DarkRgb: UInt32 = 0x1A1D24
    public static let colorSurface2Light = Color(hex: "#EDEAE4")
    public static let colorSurface2LightHex: String = "#EDEAE4"
    public static let colorSurface2LightRgb: UInt32 = 0xEDEAE4
    public static let colorSurfaceDisabledDark = Color(hex: "#14161A")
    public static let colorSurfaceDisabledDarkHex: String = "#14161A"
    public static let colorSurfaceDisabledDarkRgb: UInt32 = 0x14161A
    public static let colorSurfaceDisabledLight = Color(hex: "#ECE8E0")
    public static let colorSurfaceDisabledLightHex: String = "#ECE8E0"
    public static let colorSurfaceDisabledLightRgb: UInt32 = 0xECE8E0
    public static let colorTexte1Dark = Color(hex: "#ECEEF2")
    public static let colorTexte1DarkHex: String = "#ECEEF2"
    public static let colorTexte1DarkRgb: UInt32 = 0xECEEF2
    public static let colorTexte1Light = Color(hex: "#14161B")
    public static let colorTexte1LightHex: String = "#14161B"
    public static let colorTexte1LightRgb: UInt32 = 0x14161B
    public static let colorTexte2Dark = Color(hex: "#A3A9B6")
    public static let colorTexte2DarkHex: String = "#A3A9B6"
    public static let colorTexte2DarkRgb: UInt32 = 0xA3A9B6
    public static let colorTexte2Light = Color(hex: "#4A5060")
    public static let colorTexte2LightHex: String = "#4A5060"
    public static let colorTexte2LightRgb: UInt32 = 0x4A5060
    public static let colorTexte3Dark = Color(hex: "#82899A")
    public static let colorTexte3DarkHex: String = "#82899A"
    public static let colorTexte3DarkRgb: UInt32 = 0x82899A
    public static let colorTexte3Light = Color(hex: "#5C6679")
    public static let colorTexte3LightHex: String = "#5C6679"
    public static let colorTexte3LightRgb: UInt32 = 0x5C6679
    public static let colorTexteDisabledDark = Color(hex: "#4E5463")
    public static let colorTexteDisabledDarkHex: String = "#4E5463"
    public static let colorTexteDisabledDarkRgb: UInt32 = 0x4E5463
    public static let colorTexteDisabledLight = Color(hex: "#A0A5B2")
    public static let colorTexteDisabledLightHex: String = "#A0A5B2"
    public static let colorTexteDisabledLightRgb: UInt32 = 0xA0A5B2
    public static let colorVerreFondDark = Color(hex: "#121419D1")
    public static let colorVerreFondDarkHex: String = "#121419D1"
    public static let colorVerreFondDarkRgb: UInt32 = 0x121419D1
    public static let colorVerreFondLight = Color(hex: "#FFFFFFE0")
    public static let colorVerreFondLightHex: String = "#FFFFFFE0"
    public static let colorVerreFondLightRgb: UInt32 = 0xFFFFFFE0
    public static let colorVerreRefletDark = Color(hex: "#ECEEF21F")
    public static let colorVerreRefletDarkHex: String = "#ECEEF21F"
    public static let colorVerreRefletDarkRgb: UInt32 = 0xECEEF21F
    public static let colorVerreRefletLight = Color(hex: "#FFFFFF99")
    public static let colorVerreRefletLightHex: String = "#FFFFFF99"
    public static let colorVerreRefletLightRgb: UInt32 = 0xFFFFFF99
    public static let motionDurationMsT120: Int = 120
    public static let motionDurationMsT200: Int = 200
    public static let motionDurationMsT320: Int = 320
    public static let motionDurationMsT600: Int = 600
    public static let motionEaseEaseIn = CubicBezier(0.2, 0.0, 0.0, 1.0, spec: "cubic-bezier(0.2, 0, 0, 1)")
    public static let motionEaseEaseInSpec: String = "cubic-bezier(0.2, 0, 0, 1)"
    public static let motionEaseEaseInAnimation: Animation = Animation.timingCurve(0.2, 0.0, 0.0, 1.0)
    public static let motionEaseEaseOut = CubicBezier(0.3, 0.0, 0.8, 0.15, spec: "cubic-bezier(0.3, 0, 0.8, 0.15)")
    public static let motionEaseEaseOutSpec: String = "cubic-bezier(0.3, 0, 0.8, 0.15)"
    public static let motionEaseEaseOutAnimation: Animation = Animation.timingCurve(0.3, 0.0, 0.8, 0.15)
    public static let motionHapticMsTick: Int = 10
    public static let motionSoundAttention: [(Int, Int)] = [(67, 160), (67, 160)]
    public static let motionSoundEveil: [(Int, Int)] = [(69, 90), (76, 90)]
    public static let motionSoundFait: [(Int, Int)] = [(72, 220)]
    public static let motionSpringCardDamping: Int = 32
    public static let motionSpringCardMass: Int = 1
    public static let motionSpringCardStiffness: Int = 380
    public static let presenceWireCountCompact: Int = 7
    public static let presenceWireCountFull: Int = 21
    public static let presenceWireDecayMs: Int = 80
    public static let presenceWireGapPx: Double = 3.0
    public static let presenceWireMaxHeightPx: Double = 24.0
    public static let presenceWireWidthPx: Double = 2.0
    public static let blurGlow: Int = 16
    public static let blurHudAcrylic: Int = 30
    public static let blurPanel: Int = 24
    public static let fontFallbackChain: [String] = ["Noto Sans", "Noto Sans Arabic", "Noto Sans Devanagari", "Noto Sans CJK", "Noto Sans Adlam", "Noto Sans NKo"]
    public static let fontInterfaceFamily: String = "Inter Variable"
    public static let fontMonoFamily: String = "JetBrains Mono"
    public static let layoutCardHeight: Int = 360
    public static let layoutCardWidth: Int = 720
    public static let layoutGridPtBase: Int = 4
    public static let layoutGridPtSpacings: [Int] = [4, 8, 12, 16, 24, 32, 48, 64]
    public static let layoutHudHeight: Int = 64
    public static let layoutHudRadius: Int = 12
    public static let layoutHudTopOffset: Int = 120
    public static let layoutHudWidth: Int = 720
    public static let layoutRadiusCard: Int = 16
    public static let layoutRadiusControl: Int = 8
    public static let layoutTouchPtAndroid: Int = 48
    public static let layoutTouchPtIos: Int = 44
    public static let layoutTouchPtPointerMinPx: Int = 32
    public static let typeBodyLine: Int = 24
    public static let typeBodySize: Int = 16
    public static let typeBodyWeight: Int = 400
    public static let typeBodyStrongLine: Int = 24
    public static let typeBodyStrongSize: Int = 16
    public static let typeBodyStrongWeight: Int = 500
    public static let typeCaptionLine: Int = 18
    public static let typeCaptionSize: Int = 13
    public static let typeCaptionWeight: Int = 400
    public static let typeDisplayLine: Int = 44
    public static let typeDisplaySize: Int = 40
    public static let typeDisplayWeight: Int = 600
    public static let typeHeadingLine: Int = 26
    public static let typeHeadingSize: Int = 20
    public static let typeHeadingWeight: Int = 600
    public static let typeMonoLine: Int = 20
    public static let typeMonoSize: Int = 13
    public static let typeMonoWeight: Int = 400
    public static let typeTitleLine: Int = 34
    public static let typeTitleSize: Int = 28
    public static let typeTitleWeight: Int = 600
}
