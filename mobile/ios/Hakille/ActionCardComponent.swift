import SwiftUI

public enum CardReversibility: String, Sendable {
    case undoable = "− annulable"
    case finalAction = "+ définitif"
}

public enum ActionCardHelper {
    /// Single Decision Open Constraint (spec §8 / action_card.rs:67).
    /// Shell strictly refuses to display more than 1 active decision card at a time.
    public static func filterSingleDecision(_ cards: [JSONValue]) -> [JSONValue] {
        Array(cards.prefix(1))
    }

    public static func reversibility(for card: JSONValue) -> CardReversibility {
        let isReversible = card["reversible"] == .bool(true)
        return isReversible ? .undoable : .finalAction
    }

    /// Returns the 3 mandatory badges in strict order: Cost, Data, Reversibility.
    public static func pastilles(for card: JSONValue) -> (cost: String, data: String, reversibility: String) {
        let cost = CardPresentation.money(card["cost"])
        let data = CardPresentation.destination(card["data_exit"])
        let rev = reversibility(for: card).rawValue
        return (cost, data, rev)
    }

    public static func provenanceBand(for card: JSONValue) -> ProvenanceBand {
        let prov = card["provenance"]
        if prov == .string("Hakille") {
            return .hakille
        } else if prov == .string("External") {
            return .external
        } else {
            let skill = prov["VerifiedSkill"]
            if !skill.isNull {
                return .verifiedSkill(publisher: skill["publisher"].text, name: skill["name"].text)
            }
            return .hakille
        }
    }
}

public struct ActionCardComponent: View {
    @Environment(\.colorScheme) private var scheme
    public let card: JSONValue
    public let busy: Bool
    public let onApprove: (JSONValue) -> Void
    public let onReject: (JSONValue) -> Void
    public let onEdit: (JSONValue) -> Void
    public let onRequestWhy: (JSONValue) -> Void
    public let onCancelApproval: () -> Void

    public init(
        card: JSONValue,
        busy: Bool = false,
        onApprove: @escaping (JSONValue) -> Void,
        onReject: @escaping (JSONValue) -> Void,
        onEdit: @escaping (JSONValue) -> Void = { _ in },
        onRequestWhy: @escaping (JSONValue) -> Void = { _ in },
        onCancelApproval: @escaping () -> Void = {}
    ) {
        self.card = card
        self.busy = busy
        self.onApprove = onApprove
        self.onReject = onReject
        self.onEdit = onEdit
        self.onRequestWhy = onRequestWhy
        self.onCancelApproval = onCancelApproval
    }

    private var surfaceColor: Color {
        scheme == .dark ? HakilleTokens.colorSurface1Dark : HakilleTokens.colorSurface1Light
    }

    private var surface2Color: Color {
        scheme == .dark ? HakilleTokens.colorSurface2Dark : HakilleTokens.colorSurface2Light
    }

    private var outlineColor: Color {
        scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight
    }

    public var body: some View {
        let id = card["id"]
        let pastilles = ActionCardHelper.pastilles(for: card)
        let provenance = ActionCardHelper.provenanceBand(for: card)
        let verb = sanitizeSkillText(card["verb"].text)
        let obj = sanitizeSkillText(card["object"].text)
        let reason = sanitizeSkillText(card["reason"].text)

        VStack(alignment: .leading, spacing: 12) {
            // 1. Provenance Banner
            SecurityProvenanceBanner(provenance: provenance)

            // 2. Headline: Verb + Object
            Text("\(verb) \(obj)")
                .font(HakilleTypography.title)
                .foregroundStyle(scheme == .dark ? Color.white : Color.black)

            // 3. Plain language reason
            if !reason.isEmpty {
                Text(reason)
                    .font(HakilleTypography.body)
                    .foregroundStyle(scheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light)
            }

            // 4. Three Mandatory Badges in Strict Order
            HStack(spacing: 8) {
                // Pastille 1: Cost (Monospace font)
                Text(pastilles.cost)
                    .font(HakilleTypography.mono)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(surface2Color)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(outlineColor, lineWidth: 1))

                // Pastille 2: Data Destination
                Text(pastilles.data)
                    .font(HakilleTypography.caption)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(surface2Color)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(outlineColor, lineWidth: 1))

                // Pastille 3: Reversibility (Never color alone! Icon + Text)
                Text(pastilles.reversibility)
                    .font(HakilleTypography.caption)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(surface2Color)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(outlineColor, lineWidth: 1))
            }

            // 5. Actions (Hold-to-Confirm, Reject, Edit)
            if !busy {
                Spacer().frame(height: 4)

                HoldToConfirmButton(
                    label: "Maintenir 600 ms pour valider",
                    confirmedLabel: "Action validée",
                    isEnabled: true,
                    onConfirmed: { onApprove(id) },
                    onCancelled: onCancelApproval
                )

                HStack(spacing: 8) {
                    Button(role: .destructive) {
                        onReject(id)
                    } label: {
                        Text("Refuser")
                            .frame(maxWidth: .infinity, minHeight: 44)
                    }
                    .buttonStyle(.bordered)

                    Button {
                        onEdit(id)
                    } label: {
                        Text("Modifier")
                            .frame(maxWidth: .infinity, minHeight: 44)
                    }
                    .buttonStyle(.borderless)

                    Button {
                        onRequestWhy(id)
                    } label: {
                        Text("Pourquoi ?")
                            .frame(maxWidth: .infinity, minHeight: 44)
                    }
                    .buttonStyle(.borderless)
                }
            }
        }
        .padding(16)
        .background(surfaceColor)
        .clipShape(RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusCard)))
        .overlay(
            RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusCard))
                .stroke(outlineColor, lineWidth: 1)
        )
    }
}
