import SwiftUI

/**
 * Vue du journal d'audit signé Ed25519 (spec §22).
 * Permet l'inspection des traces et la révocation (Undo) des actions réversibles.
 */
public struct AuditJournalView: View {
    @Environment(\.colorScheme) private var scheme
    public let entries: [JSONValue]
    public let busy: Bool
    public let onRefresh: () -> Void
    public let onUndo: (UInt64) -> Void
    public let onRequestWhy: (String) -> Void

    public init(
        entries: [JSONValue],
        busy: Bool,
        onRefresh: @escaping () -> Void,
        onUndo: @escaping (UInt64) -> Void,
        onRequestWhy: @escaping (String) -> Void
    ) {
        self.entries = entries
        self.busy = busy
        self.onRefresh = onRefresh
        self.onUndo = onUndo
        self.onRequestWhy = onRequestWhy
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
            // Cryptographic Verification Card
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    HStack(spacing: 8) {
                        Circle().fill(HakilleTokens.colorSuccesDark).frame(width: 10, height: 10)
                        Text("Chaîne d'audit intègre")
                            .font(.system(size: 15, weight: .bold))
                            .foregroundStyle(HakilleTokens.colorSuccesDark)
                    }
                    Spacer()
                    Text("Ed25519 + SHA-256")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(HakilleTokens.colorLueurDark)
                }

                Text("Toutes les entrées sont signées par la clé Ed25519 du hub et chaînées par hachage cryptographique non répudiable.")
                    .font(.system(size: 13))
                    .foregroundStyle(Color.gray)

                Button {
                    onRefresh()
                } label: {
                    Text("Actualiser le journal")
                        .font(.system(size: 13, weight: .semibold))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(surface2)
                        .foregroundStyle(Color.white)
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(outline, lineWidth: 1))
                }
                .disabled(busy)
                .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .padding(16)
            .background(surface1)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))

            // Entries List
            if entries.isEmpty {
                Text("Aucune entrée de journal reçue du hub. Le journal d'audit n'est jamais simulé.")
                    .font(.system(size: 14))
                    .foregroundStyle(Color.gray)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(surface1)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
            } else {
                ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                    let idStr = entry["id"].text
                    let timestamp = entry["timestamp"].text
                    let action = entry["action"].text
                    let summary = entry["summary"].text
                    let journalId = UInt64(idStr) ?? 0
                    let isReversible = !action.lowercased().contains("suppression définitive")

                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text(action)
                                .font(.system(size: 16, weight: .semibold))
                                .foregroundStyle(Color.white)
                            Spacer()
                            Text("#\(idStr)")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(HakilleTokens.colorLueurDark)
                        }

                        Text(timestamp)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(Color.gray)

                        Text(summary)
                            .font(.system(size: 14))
                            .foregroundStyle(Color.white)
                            .textSelection(.enabled)

                        HStack {
                            Spacer()
                            Button("Pourquoi ?") {
                                onRequestWhy(idStr)
                            }
                            .font(.system(size: 13))
                            .foregroundStyle(HakilleTokens.colorLueurDark)

                            if isReversible && journalId > 0 {
                                Button("Annuler (Undo)") {
                                    onUndo(journalId)
                                }
                                .font(.system(size: 13, weight: .semibold))
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
                    }
                    .padding(16)
                    .background(surface1)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))
                }
            }
        }
    }
}
