import SwiftUI

/**
 * Vue de la mémoire souveraine et graphe de connaissances SPO (spec §23).
 * Affiche les triplets Sujet-Prédicat-Objet, les sources certifiées, et permet
 * le droit à l'oubli cryptographique avec génération de reçu d'effacement.
 */
public struct MemoryGraphView: View {
    @Environment(\.colorScheme) private var scheme
    public let memoryRows: [JSONValue]
    public let tombstones: [String]
    public let busy: Bool
    public let onSearch: (String) -> Void
    public let onRemember: (String) -> Void
    public let onForgetFact: (String) -> Void

    @State private var search = ""
    @State private var newFact = ""
    @State private var showingForgetAlert = false
    @State private var targetRowId = ""

    public init(
        memoryRows: [JSONValue],
        tombstones: [String],
        busy: Bool,
        onSearch: @escaping (String) -> Void,
        onRemember: @escaping (String) -> Void,
        onForgetFact: @escaping (String) -> Void
    ) {
        self.memoryRows = memoryRows
        self.tombstones = tombstones
        self.busy = busy
        self.onSearch = onSearch
        self.onRemember = onRemember
        self.onForgetFact = onForgetFact
    }

    private func sourceColor(_ source: String) -> Color {
        switch source.lowercased() {
        case "user", "utilisateur": return Color(hex: "#5A98D6")
        case "system", "système": return Color(hex: "#9B72CF")
        case "inferred", "déduit": return Color(hex: "#DDA752")
        default: return Color(hex: "#888888")
        }
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
            // Search Card
            VStack(alignment: .leading, spacing: 12) {
                Text("Recherche dans la mémoire souveraine")
                    .font(HakilleTypography.heading)
                    .foregroundStyle(Color.white)

                HStack {
                    TextField("Mots-clés ou entité...", text: $search)
                        .textFieldStyle(.roundedBorder)

                    Button("Rechercher") {
                        onSearch(search)
                    }
                    .disabled(busy)
                }
            }
            .padding(16)
            .background(surface1)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))

            // Tombstones Banner
            if !tombstones.isEmpty {
                HStack(spacing: 8) {
                    Circle().fill(HakilleTokens.colorAvertissementDark).frame(width: 8, height: 8)
                    Text("\(tombstones.count) fait(s) révoqué(s) (tombstones cryptographiques locales)")
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(Color.gray)
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(surface2)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }

            // Triplets List
            if memoryRows.isEmpty {
                Text("Aucune ligne reçue. La mémoire n'est pas simulée ici.")
                    .font(.system(size: 14))
                    .foregroundStyle(Color.gray)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(surface1)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
            } else {
                ForEach(Array(memoryRows.enumerated()), id: \.offset) { _, row in
                    let rowId = row["id"].text
                    let text = row["text"].text
                    let source = row["source"].text.isEmpty ? "User" : row["source"].text
                    let tags = row["tags"].array.map(\.text)

                    VStack(alignment: .leading, spacing: 10) {
                        HStack {
                            Text(source)
                                .font(.system(size: 11, weight: .bold, design: .monospaced))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                .background(sourceColor(source).opacity(0.2))
                                .foregroundStyle(sourceColor(source))
                                .clipShape(RoundedRectangle(cornerRadius: 4))
                                .overlay(RoundedRectangle(cornerRadius: 4).stroke(sourceColor(source), lineWidth: 1))

                            Spacer()
                            Text(rowId)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(Color.gray)
                        }

                        Text(text)
                            .font(.system(size: 16))
                            .foregroundStyle(Color.white)
                            .textSelection(.enabled)

                        if !tags.isEmpty {
                            HStack(spacing: 6) {
                                ForEach(tags, id: \.self) { tag in
                                    Text("#\(tag)")
                                        .font(.system(size: 12))
                                        .foregroundStyle(Color.gray)
                                }
                            }
                        }

                        HStack {
                            Spacer()
                            Button("Ce fait est faux / Oublier") {
                                targetRowId = rowId
                                showingForgetAlert = true
                            }
                            .font(.system(size: 12, weight: .semibold))
                            .foregroundStyle(HakilleTokens.colorBraiseDark)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .overlay(
                                RoundedRectangle(cornerRadius: 6)
                                    .stroke(HakilleTokens.colorBraiseDark, lineWidth: 1)
                            )
                            .disabled(busy)
                        }
                    }
                    .padding(16)
                    .background(surface1)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))
                }
            }

            // New Fact Anchoring Card
            VStack(alignment: .leading, spacing: 12) {
                Text("Ancrer un nouveau fait en mémoire")
                    .font(HakilleTypography.heading)
                    .foregroundStyle(Color.white)

                TextField("Texte du fait à mémoriser...", text: $newFact, axis: .vertical)
                    .textFieldStyle(.roundedBorder)

                Button("Mémoriser") {
                    if !newFact.isEmpty {
                        onRemember(newFact)
                        newFact = ""
                    }
                }
                .disabled(busy || newFact.isEmpty)
                .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .padding(16)
            .background(surface1)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))
        }
        .confirmationDialog(
            "Confirmer l'effacement définitif",
            isPresented: $showingForgetAlert,
            titleVisibility: .visible
        ) {
            Button("Confirmer l'oubli", role: .destructive) {
                onForgetFact(targetRowId)
                targetRowId = ""
            }
            Button("Annuler", role: .cancel) {
                targetRowId = ""
            }
        } message: {
            Text("Ce fait sera physiquement purgé du graphe souverain sur le hub. Une preuve d'effacement cryptographique scellée sera conservée.")
        }
    }
}
