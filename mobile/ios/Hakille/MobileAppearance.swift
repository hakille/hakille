import SwiftUI

enum HakilleTypography {
    static let body = Font.custom("Inter-Regular", size: CGFloat(HakilleTokens.typeBodySize), relativeTo: .body)
    static let title = Font.custom("Inter-Regular", size: CGFloat(HakilleTokens.typeTitleSize), relativeTo: .title).weight(.semibold)
    static let heading = Font.custom("Inter-Regular", size: CGFloat(HakilleTokens.typeHeadingSize), relativeTo: .headline).weight(.semibold)
    static let caption = Font.custom("Inter-Regular", size: CGFloat(HakilleTokens.typeCaptionSize), relativeTo: .caption)
}

struct VoicePresenceMark: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dynamicTypeSize) private var typeSize
    let listening: Bool
    let speaking: Bool
    private var light: Color { Color(hakille: scheme == .dark ? HakilleTokens.colorLueurDark : HakilleTokens.colorLueurLight) }
    var body: some View {
        Image("HakilleMark").renderingMode(.original).resizable().scaledToFit()
            .frame(width: typeSize.isAccessibilitySize ? 88 : 144, height: typeSize.isAccessibilitySize ? 88 : 144)
            .padding(16)
            .background(Circle().fill(light.opacity(listening || speaking ? 0.06 : 0.02)))
            .overlay(Circle().stroke(light.opacity(listening || speaking ? 0.7 : 0.15), lineWidth: listening || speaking ? 2 : 1))
            // Signe statique ; aucune animation ni mesure audio fictive.
            .accessibilityHidden(true)
    }
}
