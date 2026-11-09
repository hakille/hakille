import SwiftUI

public enum HakilleTypography {
    public static let body = Font.custom("Inter-Regular", size: CGFloat(HakilleTokens.typeBodySize), relativeTo: .body)
    public static let title = Font.custom("Inter-Regular", size: CGFloat(HakilleTokens.typeTitleSize), relativeTo: .title).weight(.semibold)
    public static let heading = Font.custom("Inter-Regular", size: CGFloat(HakilleTokens.typeHeadingSize), relativeTo: .headline).weight(.semibold)
    public static let caption = Font.custom("Inter-Regular", size: CGFloat(HakilleTokens.typeCaptionSize), relativeTo: .caption)
    public static let mono = Font.system(size: CGFloat(HakilleTokens.typeMonoSize), weight: .regular, design: .monospaced)
}

public enum HakillePalette {
    public static func surface0(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorSurface0Dark : HakilleTokens.colorSurface0Light
    }
    public static func surface1(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorSurface1Dark : HakilleTokens.colorSurface1Light
    }
    public static func surface2(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorSurface2Dark : HakilleTokens.colorSurface2Light
    }
    public static func ligne(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight
    }
    public static func lueur(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorLueurDark : HakilleTokens.colorLueurLight
    }
    public static func texte1(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorTexte1Dark : HakilleTokens.colorTexte1Light
    }
    public static func texte2(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light
    }
    /// Strictly reserved for validation, 600ms hold-progress, and decision-ring
    public static func braise(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorBraiseDark : HakilleTokens.colorBraiseLight
    }
    public static func braiseHalo(for scheme: ColorScheme) -> Color {
        scheme == .dark ? HakilleTokens.colorBraiseHaloDark : HakilleTokens.colorBraiseHaloLight
    }
}

public struct HairlineBorderModifier: ViewModifier {
    public let color: Color
    public let cornerRadius: CGFloat
    public func body(content: Content) -> some View {
        content.overlay(
            RoundedRectangle(cornerRadius: cornerRadius)
                .stroke(color, lineWidth: 1)
        )
    }
}

public struct MinTouchTargetModifier: ViewModifier {
    public let minSize: CGFloat
    public func body(content: Content) -> some View {
        content.frame(minWidth: minSize, minHeight: minSize)
    }
}

public extension View {
    func hairlineBorder(color: Color, cornerRadius: CGFloat = CGFloat(HakilleTokens.layoutRadiusCard)) -> some View {
        modifier(HairlineBorderModifier(color: color, cornerRadius: cornerRadius))
    }
    func minTouchTarget(min: CGFloat = CGFloat(HakilleTokens.layoutTouchPtIos)) -> some View {
        modifier(MinTouchTargetModifier(minSize: min))
    }
    func acrylicHud() -> some View {
        self.blur(radius: CGFloat(HakilleTokens.blurHudAcrylic))
    }
    func acrylicPanel() -> some View {
        self.blur(radius: CGFloat(HakilleTokens.blurPanel))
    }
    func acrylicGlow() -> some View {
        self.blur(radius: CGFloat(HakilleTokens.blurGlow))
    }
}

/// Fallback voice presence mark (preserved for backward compatibility, delegate to LivingPresenceAuraView)
public struct VoicePresenceMark: View {
    public let listening: Bool
    public let speaking: Bool
    public init(listening: Bool, speaking: Bool = false) {
        self.listening = listening
        self.speaking = speaking
    }
    public var body: some View {
        LivingPresenceAuraView(
            state: speaking ? .speaking : (listening ? .listening : .idle),
            size: 176
        )
    }
}
