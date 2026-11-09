import SwiftUI

/**
 * Vue Échelle d'autonomie par domaine (spec §24).
 * Crans 0 à 4 (Observer, Préparer, Agir sur le réversible, Agir sous enveloppe, Déléguer).
 */
public struct AutonomyLadderView: View {
    @Environment(\.colorScheme) private var scheme
    public let domainAutonomies: [JSONValue]
    public let onSaveCrank: (String, Int, UInt64?, UInt32?, UInt32?) -> Void

    private let domains = ["Mail", "Agenda", "Maison", "Fichiers", "Paiement", "Code"]
    @State private var selectedDomain = "Mail"
    @State private var currentCrank = 1
    @State private var maxCost = "1000"
    @State private var maxActions = "50"

    private let crankLabels = [
        "Cran 0 · Observer",
        "Cran 1 · Préparer",
        "Cran 2 · Agir sur le réversible",
        "Cran 3 · Agir sous enveloppe",
        "Cran 4 · Déléguer"
    ]

    private let crankDescriptions = [
        "Hakille n'entreprend rien sans votre demande directe. Idéal pour l'observation passive.",
        "Hakille prépare les actions et les propositions en attente de votre décision explicite.",
        "Hakille exécute les actions réversibles directement. Toute action annulée rétrograde le cran.",
        "Hakille exécute dans les limites strictes d'une enveloppe de coût et d'actions (90 jours).",
        "Délégation complète avec journal d'audit cryptographique et surveillance permanente."
    ]

    public init(
        domainAutonomies: [JSONValue],
        onSaveCrank: @escaping (String, Int, UInt64?, UInt32?, UInt32?) -> Void
    ) {
        self.domainAutonomies = domainAutonomies
        self.onSaveCrank = onSaveCrank
    }

    private var activeDto: JSONValue? {
        domainAutonomies.first { $0["domain"].text.caseInsensitiveCompare(selectedDomain) == .orderedSame }
    }

    private var promotionOfferable: Bool {
        activeDto?["promotion_offerable"] == .bool(true)
    }

    private var validatedCount: UInt64 {
        activeDto?["validated_count"].id ?? 0
    }

    private var surface1: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorSurface1Dark : HakilleTokens.colorSurface1Light)
    }

    private var surface2: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorSurface2Dark : HakilleTokens.colorSurface2Light)
    }

    private var outline: Color {
        Color(hex: scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Header
            HStack {
                Text("Échelle d'autonomie")
                    .font(HakilleTypography.heading)
                    .foregroundStyle(Color.white)
                Spacer()
                Text("Spec §24")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(HakilleTokens.colorLueurDark)
            }

            // Domain Picker
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(domains, id: \.self) { domain in
                        Button {
                            selectedDomain = domain
                            syncState()
                        } label: {
                            Text(domain)
                                .font(.system(size: 14, weight: selectedDomain == domain ? .bold : .regular))
                                .padding(.horizontal, 12)
                                .padding(.vertical, 6)
                                .background(selectedDomain == domain ? surface2 : surface1)
                                .foregroundStyle(selectedDomain == domain ? HakilleTokens.colorLueurDark : Color.gray)
                                .clipShape(RoundedRectangle(cornerRadius: 8))
                                .overlay(RoundedRectangle(cornerRadius: 8).stroke(outline, lineWidth: 1))
                        }
                    }
                }
            }

            // Promotion Banner
            if promotionOfferable {
                HStack(spacing: 8) {
                    Circle().fill(HakilleTokens.colorSuccesDark).frame(width: 8, height: 8)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Promotion méritée")
                            .font(.system(size: 13, weight: .bold))
                            .foregroundStyle(HakilleTokens.colorSuccesDark)
                        Text("\(validatedCount) actions validées sans correction : vous pouvez augmenter l'autonomie.")
                            .font(.system(size: 12))
                            .foregroundStyle(Color.gray)
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(surface2)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }

            // Crank Stepper & Description
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(crankLabels[currentCrank.clamped(to: 0...4)])
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(Color.white)
                    Spacer()
                    Stepper("", value: $currentCrank, in: 0...4)
                        .labelsHidden()
                }

                Text(crankDescriptions[currentCrank.clamped(to: 0...4)])
                    .font(.system(size: 13))
                    .foregroundStyle(Color.gray)
                    .lineSpacing(3)
            }
            .padding(14)
            .background(surface2)
            .clipShape(RoundedRectangle(cornerRadius: 10))

            // Envelope Configuration for Cranks 3 & 4
            if currentCrank >= 3 {
                VStack(alignment: .leading, spacing: 10) {
                    Text("Enveloppe de sécurité (validité 90 jours)")
                        .font(.system(size: 12, weight: .semibold, design: .monospaced))
                        .foregroundStyle(HakilleTokens.colorLueurDark)

                    HStack(spacing: 12) {
                        VStack(alignment: .leading) {
                            Text("Plafond (cents)").font(.system(size: 11)).foregroundStyle(Color.gray)
                            TextField("1000", text: $maxCost)
                                .textFieldStyle(.roundedBorder)
                                .keyboardType(.numberPad)
                        }
                        VStack(alignment: .leading) {
                            Text("Max actions").font(.system(size: 11)).foregroundStyle(Color.gray)
                            TextField("50", text: $maxActions)
                                .textFieldStyle(.roundedBorder)
                                .keyboardType(.numberPad)
                        }
                    }
                }
                .padding(12)
                .background(surface1)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }

            Text("Toute annulation d'une action autonome rétrograde automatiquement le domaine d'un cran.")
                .font(.system(size: 11))
                .foregroundStyle(Color.gray)

            // Save Button
            Button {
                let cost = currentCrank >= 3 ? UInt64(maxCost) ?? 1000 : nil
                let actions = currentCrank >= 3 ? UInt32(maxActions) ?? 50 : nil
                onSaveCrank(selectedDomain, currentCrank, cost, actions, 90)
            } label: {
                Text("Appliquer au domaine \(selectedDomain)")
                    .font(.system(size: 15, weight: .semibold))
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .background(surface2)
                    .foregroundStyle(Color.white)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(outline, lineWidth: 1))
            }
        }
        .padding(16)
        .background(surface1)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))
        .onAppear { syncState() }
    }

    private func syncState() {
        if let dto = activeDto {
            if case .unsigned(let c) = dto["crank"] {
                currentCrank = Int(c)
            } else if case .integer(let c) = dto["crank"] {
                currentCrank = Int(c)
            }
            let env = dto["envelope"]
            if !env.isNull {
                if let cost = env["max_cost_cents"].id { maxCost = "\(cost)" }
                if let acts = env["max_actions"].id { maxActions = "\(acts)" }
            }
        }
    }
}

private extension Comparable {
    func clamped(to limits: ClosedRange<Self>) -> Self {
        min(max(self, limits.lowerBound), limits.upperBound)
    }
}
