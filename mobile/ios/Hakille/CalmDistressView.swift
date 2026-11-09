import SwiftUI
import UIKit

public enum CalmDistressConstants {
    public static let calmCopy: String = "Je reste là. Un geste pour appeler ton contact, un autre pour les urgences."
    public static let emergencyNumber: String = "112"
}

public struct CalmDistressView: View {
    @Environment(\.colorScheme) private var scheme
    public let trustedContact: String?
    public let onDismiss: () -> Void

    public init(trustedContact: String? = nil, onDismiss: @escaping () -> Void) {
        self.trustedContact = trustedContact
        self.onDismiss = onDismiss
    }

    private var backgroundColor: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorSurface0Dark : HakilleTokens.colorSurface0Light)
    }

    private var containerColor: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorSurface1Dark : HakilleTokens.colorSurface1Light)
    }

    private var surface2Color: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorSurface2Dark : HakilleTokens.colorSurface2Light)
    }

    private var outlineColor: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight)
    }

    private var textColor: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorTexte1Dark : HakilleTokens.colorTexte1Light)
    }

    private var secondaryText: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light)
    }

    private func dial(number: String) {
        if let url = URL(string: "tel://\(number.filter { $0.isNumber || $0 == "+" })") {
            UIApplication.shared.open(url)
        }
    }

    public var body: some View {
        ZStack {
            backgroundColor.ignoresSafeArea()

            VStack(spacing: 20) {
                // Static Calm Copy (strictly zero animations or pulsating graphics)
                Text(CalmDistressConstants.calmCopy)
                    .font(HakilleTypography.title)
                    .foregroundStyle(textColor)
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)

                Text("Hakille est en veille sécurisée. Aucune donnée n'est envoyée.")
                    .font(HakilleTypography.caption)
                    .foregroundStyle(secondaryText)
                    .multilineTextAlignment(.center)

                Spacer().frame(height: 8)

                // Direct Dial 1: Trusted Contact
                if let contact = trustedContact, !contact.isEmpty {
                    Button {
                        dial(number: contact)
                    } label: {
                        Text("Appeler mon contact de confiance (\(contact))")
                            .font(HakilleTypography.heading)
                            .foregroundStyle(textColor)
                            .frame(maxWidth: .infinity, minHeight: 56)
                            .background(surface2Color)
                            .clipShape(RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusControl)))
                            .overlay(RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusControl)).stroke(outlineColor, lineWidth: 1))
                    }
                }

                // Direct Dial 2: Emergency Services (112)
                Button {
                    dial(number: CalmDistressConstants.emergencyNumber)
                } label: {
                    Text("Appeler les urgences (112)")
                        .font(HakilleTypography.heading)
                        .foregroundStyle(textColor)
                        .frame(maxWidth: .infinity, minHeight: 56)
                        .background(surface2Color)
                        .clipShape(RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusControl)))
                        .overlay(RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusControl)).stroke(outlineColor, lineWidth: 1))
                }

                Spacer().frame(height: 4)

                // Safe Dismiss Button
                Button {
                    onDismiss()
                } label: {
                    Text("Je vais bien, fermer")
                        .font(HakilleTypography.body)
                        .foregroundStyle(secondaryText)
                        .frame(maxWidth: .infinity, minHeight: 48)
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
