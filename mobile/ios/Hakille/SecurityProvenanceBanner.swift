import SwiftUI
import UIKit

public let CLIPBOARD_SENSITIVE_TTL_SECS: TimeInterval = 60.0

public enum ProvenanceBand: Equatable, Sendable {
    case hakille
    case verifiedSkill(publisher: String, name: String)
    case external

    public var label: String {
        switch self {
        case .hakille:
            return "Hakille"
        case .verifiedSkill(let publisher, let name):
            return "skill vérifié (\(publisher) · \(name))"
        case .external:
            return "externe"
        }
    }

    public func looksLikeInstalled(_ installed: [String]) -> Bool {
        switch self {
        case .verifiedSkill(_, let name):
            let lower = name.lowercased()
            return installed.contains { other in
                let otherLower = other.lowercased()
                return otherLower != lower && (otherLower.contains(lower) || lower.contains(otherLower))
            }
        default:
            return false
        }
    }
}

public enum ExternalBlockConstants {
    public static let sideRulePt: CGFloat = 2.0
    public static let badge: String = "externe"
}

public func sanitizeSkillText(_ input: String) -> String {
    // 1. Strip entire <script>...</script> and <style>...</style> blocks with body
    var clean = input.replacingOccurrences(of: "(?is)<script\\b[^>]*>.*?</script>", with: "", options: .regularExpression)
    clean = clean.replacingOccurrences(of: "(?is)<style\\b[^>]*>.*?</style>", with: "", options: .regularExpression)

    // 2. Strip all remaining HTML/XML tags
    clean = clean.replacingOccurrences(of: "<[^>]*>", with: "", options: .regularExpression)

    // 3. Strip ANSI escape sequences: CSI, OSC, single char Fe, and bare ESC
    clean = clean.replacingOccurrences(of: "\u{001B}\\[[0-9;?]*[a-zA-Z]", with: "", options: .regularExpression)
    clean = clean.replacingOccurrences(of: "\u{001B}\\].*?(\u{0007}|\u{001B}\\\\)", with: "", options: .regularExpression)
    clean = clean.replacingOccurrences(of: "\u{001B}[@-Z\\-_]", with: "", options: .regularExpression)
    clean = clean.replacingOccurrences(of: "\u{001B}", with: "")

    return clean
}

public func copySensitiveToClipboard(text: String, label: String = "Sensitive") {
    let expiration = Date().addingTimeInterval(CLIPBOARD_SENSITIVE_TTL_SECS)
    let pasteboard = UIPasteboard.general
    pasteboard.setItems(
        [[UIPasteboard.typeListString[0] as String: text]],
        options: [.expirationDate: expiration]
    )
}

public struct SecurityProvenanceBanner: View {
    @Environment(\.colorScheme) private var scheme
    public let provenance: ProvenanceBand
    public let warning: String?

    public init(provenance: ProvenanceBand, warning: String? = nil) {
        self.provenance = provenance
        self.warning = warning
    }

    private var isExternal: Bool {
        provenance == .external
    }

    private var lateralRuleColor: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight)
    }

    public var body: some View {
        HStack {
            Text(provenance.label)
                .font(HakilleTypography.caption)
                .padding(.horizontal, 8)
                .padding(.vertical, 2)
                .background(
                    RoundedRectangle(cornerRadius: 4)
                        .fill(isExternal ? lateralRuleColor.opacity(0.4) : Color(hex: scheme == .dark ? HakilleTokens.colorSurface2Dark : HakilleTokens.colorSurface2Light))
                )
                .foregroundStyle(isExternal ? Color(hex: scheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light) : (scheme == .dark ? Color.white : Color.black))

            Spacer()

            if let warning = warning {
                Text(warning)
                    .font(HakilleTypography.caption)
                    .foregroundStyle(Color(hex: scheme == .dark ? HakilleTokens.colorAvertissementDark : HakilleTokens.colorAvertissementLight))
            }
        }
        .padding(.leading, isExternal ? 8 : 0)
        .overlay(alignment: .leading) {
            if isExternal {
                Rectangle()
                    .fill(lateralRuleColor)
                    .frame(width: ExternalBlockConstants.sideRulePt)
            }
        }
        .padding(.vertical, 4)
    }
}
