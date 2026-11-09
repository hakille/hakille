import SwiftUI

/**
 * Vue feuille « Pourquoi » à trois niveaux (spec §25).
 * Niveau 1 : En une phrase concise.
 * Niveau 2 : Faits du graphe mobilisés avec bouton « Ce fait est faux ».
 * Niveau 3 : Raisonnement technique, confiance, modèle, coût, destination données.
 */
public struct WhySheetView: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss
    public let whySheet: JSONValue
    public let onDismiss: () -> Void
    public let onDisputeFact: (String) -> Void

    public init(
        whySheet: JSONValue,
        onDismiss: @escaping () -> Void,
        onDisputeFact: @escaping (String) -> Void = { _ in }
    ) {
        self.whySheet = whySheet
        self.onDismiss = onDismiss
        self.onDisputeFact = onDisputeFact
    }

    private var sentence: String {
        let text = whySheet["sentence"].text
        return text.isEmpty ? "Je n'ai pas gardé la raison de ce choix." : text
    }

    private var facts: [JSONValue] {
        whySheet["facts"].array
    }

    private var plan: String {
        whySheet["plan"].text
    }

    private var powers: [String] {
        whySheet["powers"].array.map(\.text)
    }

    private var confidenceWord: String {
        let word = whySheet["confidence_word"].text
        return word.isEmpty ? "basse" : word
    }

    private var confidenceValue: Double {
        whySheet["confidence_value"].doubleValue ?? 0.0
    }

    private var modelLine: String {
        let m = whySheet["model_line"].text
        return m.isEmpty ? "Qwen 2.5 0.5B Local CPU" : m
    }

    private var costLabel: String {
        let c = whySheet["cost_label"].text
        return c.isEmpty ? "0.00 EUR" : c
    }

    private var dataLine: String {
        let d = whySheet["data_line"].text
        return d.isEmpty ? "Local uniquement" : d
    }

    private var wouldChange: String {
        whySheet["would_change"].text
    }

    private var confidenceColor: Color {
        if confidenceValue >= 0.85 { return HakilleTokens.colorSuccesDark }
        if confidenceValue >= 0.50 { return HakilleTokens.colorAvertissementDark }
        return HakilleTokens.colorErreurDark
    }

    private var surface1: Color {
        scheme == .dark ? HakilleTokens.colorSurface1Dark : HakilleTokens.colorSurface1Light
    }

    private var surface2: Color {
        scheme == .dark ? HakilleTokens.colorSurface2Dark : HakilleTokens.colorSurface2Light
    }

    private var outline: Color {
        scheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight
    }

    public var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    // NIVEAU 1
                    VStack(alignment: .leading, spacing: 8) {
                        Text("NIVEAU 1 · EN BREF")
                            .font(.system(size: 11, weight: .bold, design: .monospaced))
                            .foregroundStyle(HakilleTokens.colorLueurDark)

                        Text(sentence)
                            .font(.system(size: 17, weight: .regular))
                            .foregroundStyle(Color.white)
                            .lineSpacing(4)
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(surface2)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))

                    // NIVEAU 2
                    VStack(alignment: .leading, spacing: 12) {
                        Text("NIVEAU 2 · FAITS DU GRAPHE MOBILISÉS")
                            .font(.system(size: 11, weight: .bold, design: .monospaced))
                            .foregroundStyle(HakilleTokens.colorLueurDark)

                        if facts.isEmpty {
                            Text("Aucun fait externe mobilisé directement. Décision basée sur les règles locales.")
                                .font(.system(size: 14))
                                .foregroundStyle(Color.gray)
                        } else {
                            ForEach(Array(facts.enumerated()), id: \.offset) { _, fact in
                                let label = fact["label"].text
                                let source = fact["source"].text
                                let date = fact["date"].text
                                let factId = fact["id"].text.isEmpty ? label : fact["id"].text

                                HStack(alignment: .center) {
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(label)
                                            .font(.system(size: 15))
                                            .foregroundStyle(Color.white)
                                        Text("Source : \(source) · \(date)")
                                            .font(.system(size: 12))
                                            .foregroundStyle(Color.gray)
                                    }
                                    Spacer()
                                    Button("Ce fait est faux") {
                                        onDisputeFact(factId)
                                    }
                                    .font(.system(size: 12, weight: .semibold))
                                    .foregroundStyle(HakilleTokens.colorBraiseDark)
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 6)
                                    .overlay(
                                        RoundedRectangle(cornerRadius: 6)
                                            .stroke(HakilleTokens.colorBraiseDark, lineWidth: 1)
                                    )
                                }
                                .padding(12)
                                .background(surface1)
                                .clipShape(RoundedRectangle(cornerRadius: 8))
                                .overlay(RoundedRectangle(cornerRadius: 8).stroke(outline, lineWidth: 1))
                            }
                        }
                    }

                    // NIVEAU 3
                    VStack(alignment: .leading, spacing: 12) {
                        Text("NIVEAU 3 · RAISONNEMENT TECHNIQUE & GARANTIES")
                            .font(.system(size: 11, weight: .bold, design: .monospaced))
                            .foregroundStyle(HakilleTokens.colorLueurDark)

                        VStack(alignment: .leading, spacing: 12) {
                            HStack {
                                Text("Indice de confiance :").foregroundStyle(Color.gray)
                                Spacer()
                                HStack(spacing: 6) {
                                    Circle().fill(confidenceColor).frame(width: 8, height: 8)
                                    Text("\(confidenceWord) (\(Int(confidenceValue * 100)) %)")
                                        .font(.system(size: 13, weight: .bold, design: .monospaced))
                                        .foregroundStyle(confidenceColor)
                                }
                            }

                            HStack {
                                Text("Moteur d'inférence :").foregroundStyle(Color.gray)
                                Spacer()
                                Text(modelLine).foregroundStyle(Color.white)
                            }

                            HStack {
                                Text("Coût prévisionnel :").foregroundStyle(Color.gray)
                                Spacer()
                                Text(costLabel)
                                    .font(.system(size: 13, design: .monospaced))
                                    .foregroundStyle(Color.white)
                            }

                            HStack {
                                Text("Destination des données :").foregroundStyle(Color.gray)
                                Spacer()
                                Text(dataLine).foregroundStyle(Color.white)
                            }

                            if !powers.isEmpty {
                                Text("Capacités requises :").foregroundStyle(Color.gray)
                                HStack(spacing: 6) {
                                    ForEach(powers, id: \.self) { power in
                                        Text(power)
                                            .font(.system(size: 11, design: .monospaced))
                                            .padding(.horizontal, 8)
                                            .padding(.vertical, 2)
                                            .background(Color.black.opacity(0.4))
                                            .clipShape(RoundedRectangle(cornerRadius: 4))
                                            .overlay(RoundedRectangle(cornerRadius: 4).stroke(outline, lineWidth: 1))
                                    }
                                }
                            }

                            if !plan.isEmpty {
                                Divider().background(outline)
                                Text("Plan d'exécution détaillé :").font(.system(size: 12)).foregroundStyle(Color.gray)
                                Text(plan).font(.system(size: 14)).foregroundStyle(Color.white)
                            }

                            if !wouldChange.isEmpty {
                                Text("Impact de validation :").font(.system(size: 12)).foregroundStyle(Color.gray)
                                Text(wouldChange).font(.system(size: 14)).foregroundStyle(Color.white)
                            }
                        }
                        .font(.system(size: 14))
                        .padding(16)
                        .background(surface2)
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                        .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))
                    }
                }
                .padding(20)
            }
            .background(HakilleTokens.colorSurface0Dark)
            .navigationTitle("Pourquoi cette action ?")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Fermer") {
                        onDismiss()
                        dismiss()
                    }
                }
            }
        }
    }
}
