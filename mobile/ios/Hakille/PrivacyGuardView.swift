import SwiftUI

public enum PrivacyGuardHelper {
    public static func shouldTriggerVeil(peopleCount: Int) -> Bool {
        peopleCount >= 2
    }

    public static func triggerLabel(_ event: JSONValue) -> String {
        let trigger = event["trigger"]
        if trigger.text == "EphemeralTtlPurged" {
            return "Purge RAM biométrique (TTL)"
        }
        if !trigger["ShoulderSurfingDetected"].isNull {
            let count = Int(trigger["ShoulderSurfingDetected"]["people_count"].id ?? 2)
            return "Regard tiers détecté (\(count) personnes)"
        }
        if !trigger["RawDataExfiltrationBlocked"].isNull {
            let scope = trigger["RawDataExfiltrationBlocked"]["blocked_scope"].text
            return "Exfiltration bloquée (\(scope))"
        }
        if !trigger["HardwareKillSwitchActivated"].isNull {
            let device = trigger["HardwareKillSwitchActivated"]["device"].text
            return "Coupe-circuit matériel (\(device))"
        }
        return "Bouclier de confidentialité"
    }
}

public final class PrivacyGuardDebounceFilter {
    private let requiredConsecutive: Int
    private var consecutiveTriggers = 0
    public private(set) var veilActive = false

    public init(requiredConsecutive: Int = 3) {
        self.requiredConsecutive = requiredConsecutive
    }

    public func update(peopleCount: Int) -> Bool {
        if PrivacyGuardHelper.shouldTriggerVeil(peopleCount: peopleCount) {
            consecutiveTriggers += 1
            if consecutiveTriggers >= requiredConsecutive {
                veilActive = true
            }
        } else {
            consecutiveTriggers = 0
            veilActive = false
        }
        return veilActive
    }
}

public struct PrivacyGuardBannerView: View {
    @Environment(\.colorScheme) private var scheme
    public let event: JSONValue
    public let onDismiss: () -> Void

    public init(event: JSONValue, onDismiss: @escaping () -> Void) {
        self.event = event
        self.onDismiss = onDismiss
    }

    private var textColor: Color {
        scheme == .dark ? HakilleTokens.colorTexte1Dark : HakilleTokens.colorTexte1Light
    }

    private var secondaryText: Color {
        scheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light
    }

    private var outlineColor: Color {
        scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(PrivacyGuardHelper.triggerLabel(event))
                    .font(HakilleTypography.heading)
                    .foregroundStyle(textColor)
                    .accessibilityAddTraits(.isHeader)
                Spacer()
                Button("Fermer") {
                    onDismiss()
                }
                .font(HakilleTypography.caption)
                .foregroundStyle(secondaryText)
                .frame(minWidth: 44, minHeight: 44)
            }
            if !event["description"].text.isEmpty {
                Text(event["description"].text)
                    .font(HakilleTypography.body)
                    .foregroundStyle(textColor)
            }
            if !event["recommended_action"].text.isEmpty {
                Text(event["recommended_action"].text)
                    .font(HakilleTypography.caption)
                    .foregroundStyle(secondaryText)
            }
        }
        .padding(12)
        .overlay(
            RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusCard))
                .stroke(outlineColor, lineWidth: 1)
        )
    }
}

public struct PrivacyGuardVeilView: View {
    @Environment(\.colorScheme) private var scheme
    public let event: JSONValue
    public let onDismiss: () -> Void

    public init(event: JSONValue, onDismiss: @escaping () -> Void) {
        self.event = event
        self.onDismiss = onDismiss
    }

    // Strict Obsidian palette — zero Braise (#FF7A2F) in privacy veil
    private var backgroundColor: Color {
        scheme == .dark ? HakilleTokens.colorSurface0Dark : HakilleTokens.colorSurface0Light
    }

    private var containerColor: Color {
        scheme == .dark ? HakilleTokens.colorSurface1Dark : HakilleTokens.colorSurface1Light
    }

    private var outlineColor: Color {
        scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight
    }

    private var textColor: Color {
        scheme == .dark ? HakilleTokens.colorTexte1Dark : HakilleTokens.colorTexte1Light
    }

    private var secondaryText: Color {
        scheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light
    }

    public var body: some View {
        ZStack {
            backgroundColor.opacity(0.95).ignoresSafeArea()
            Rectangle()
                .fill(.ultraThinMaterial)
                .ignoresSafeArea()

            VStack(spacing: 16) {
                Text("Bouclier PrivacyGuard actif")
                    .font(HakilleTypography.title)
                    .foregroundStyle(textColor)
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)

                Text(event["description"].text.isEmpty
                    ? "Observateur tiers détecté derrière l'écran."
                    : event["description"].text)
                    .font(HakilleTypography.body)
                    .foregroundStyle(textColor)
                    .multilineTextAlignment(.center)

                Text(event["recommended_action"].text.isEmpty
                    ? "Écran voilé pour protéger vos données souveraines."
                    : event["recommended_action"].text)
                    .font(HakilleTypography.caption)
                    .foregroundStyle(secondaryText)
                    .multilineTextAlignment(.center)

                Button {
                    onDismiss()
                } label: {
                    Text("Je suis seul · Déverrouiller l'écran")
                        .font(HakilleTypography.heading)
                        .foregroundStyle(textColor)
                        .frame(maxWidth: .infinity, minHeight: 48)
                        .overlay(
                            RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusControl))
                                .stroke(outlineColor, lineWidth: 1)
                        )
                }
            }
            .padding(24)
            .background(containerColor)
            .clipShape(RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusCard)))
            .overlay(
                RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusCard))
                    .stroke(outlineColor, lineWidth: 1)
            )
            .padding(24)
        }
    }
}
