import SwiftUI

/**
 * Vue Catalogue d'agents et compétences pour iOS (spec §28 & §30).
 * Segrégation stricte :
 * - Agents Exécutables (WASM vérifié)
 * - Agents Déclaratifs (Manifestes d'intention sans binaire exécutable)
 */
public struct AgentCatalogView: View {
    @Environment(\.colorScheme) private var scheme
    public let agents: [JSONValue]
    public let lastAgentRun: JSONValue
    public let busy: Bool
    public let onRefresh: () -> Void
    public let onRunAgent: (String, String) -> Void

    @State private var runningAgentName: String?
    @State private var inputJson = "{}"

    public init(
        agents: [JSONValue],
        lastAgentRun: JSONValue,
        busy: Bool,
        onRefresh: @escaping () -> Void,
        onRunAgent: @escaping (String, String) -> Void
    ) {
        self.agents = agents
        self.lastAgentRun = lastAgentRun
        self.busy = busy
        self.onRefresh = onRefresh
        self.onRunAgent = onRunAgent
    }

    private var executableAgents: [JSONValue] {
        agents.filter { $0["artifact_present"] == .bool(true) }
    }

    private var declarativeAgents: [JSONValue] {
        agents.filter { $0["artifact_present"] != .bool(true) }
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
                VStack(alignment: .leading, spacing: 2) {
                    Text("Catalogue d'agents")
                        .font(HakilleTypography.heading)
                        .foregroundStyle(Color.white)
                    Text("\(executableAgents.count) exécutable(s) · \(declarativeAgents.count) déclaratif(s)")
                        .font(.system(size: 12))
                        .foregroundStyle(Color.gray)
                }
                Spacer()
                Button("Actualiser") {
                    onRefresh()
                }
                .disabled(busy)
            }
            .padding(16)
            .background(surface1)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))

            // Section 1 : Agents Exécutables
            Text("AGENTS EXÉCUTABLES (WASM VÉRIFIÉ)")
                .font(.system(size: 11, weight: .bold, design: .monospaced))
                .foregroundStyle(HakilleTokens.colorSuccesDark)

            if executableAgents.isEmpty {
                Text("Aucun agent WASM exécutable n'est actuellement provisionné sur le hub.")
                    .font(.system(size: 13))
                    .foregroundStyle(Color.gray)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(surface1)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
            } else {
                ForEach(executableAgents, id: \.agentName) { agent in
                    let name = agent["name"].text
                    let sha256 = agent["sha256"].text
                    let caps = agent["required_caps"].array.map(\.text)

                    VStack(alignment: .leading, spacing: 10) {
                        HStack {
                            Text(name)
                                .font(.system(size: 16, weight: .bold))
                                .foregroundStyle(Color.white)
                            Spacer()
                            Text("WASM VÉRIFIÉ")
                                .font(.system(size: 10, weight: .bold, design: .monospaced))
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(HakilleTokens.colorSuccesDark.opacity(0.2))
                                .foregroundStyle(HakilleTokens.colorSuccesDark)
                                .clipShape(RoundedRectangle(cornerRadius: 4))
                                .overlay(RoundedRectangle(cornerRadius: 4).stroke(HakilleTokens.colorSuccesDark, lineWidth: 1))
                        }

                        if !sha256.isEmpty {
                            Text("SHA-256 : \(String(sha256.prefix(16)))…\(String(sha256.suffix(8)))")
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(HakilleTokens.colorLueurDark)
                        }

                        if !caps.isEmpty {
                            HStack(spacing: 6) {
                                ForEach(caps, id: \.self) { cap in
                                    Text(cap)
                                        .font(.system(size: 11, design: .monospaced))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(Color.black.opacity(0.4))
                                        .clipShape(RoundedRectangle(cornerRadius: 4))
                                }
                            }
                        }

                        Button {
                            runningAgentName = name
                            inputJson = "{}"
                        } label: {
                            Text("Lancer en bac à sable")
                                .font(.system(size: 13, weight: .semibold))
                                .frame(maxWidth: .infinity, minHeight: 38)
                                .background(surface2)
                                .foregroundStyle(Color.white)
                                .clipShape(RoundedRectangle(cornerRadius: 8))
                                .overlay(RoundedRectangle(cornerRadius: 8).stroke(outline, lineWidth: 1))
                        }
                        .disabled(busy)
                    }
                    .padding(16)
                    .background(surface1)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))
                }
            }

            // Section 2 : Agents Déclaratifs
            Text("AGENTS DÉCLARATIFS (MANIFESTES D'INTENTION)")
                .font(.system(size: 11, weight: .bold, design: .monospaced))
                .foregroundStyle(Color.gray)

            if declarativeAgents.isEmpty {
                Text("Aucun agent déclaratif répertorié.")
                    .font(.system(size: 13))
                    .foregroundStyle(Color.gray)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(surface1)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
            } else {
                ForEach(declarativeAgents, id: \.agentName) { agent in
                    let name = agent["name"].text
                    let caps = agent["required_caps"].array.map(\.text)

                    VStack(alignment: .leading, spacing: 10) {
                        HStack {
                            Text(name)
                                .font(.system(size: 16, weight: .semibold))
                                .foregroundStyle(Color.white)
                            Spacer()
                            Text("DÉCLARATIF")
                                .font(.system(size: 10, weight: .semibold, design: .monospaced))
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(Color.gray.opacity(0.2))
                                .foregroundStyle(Color.gray)
                                .clipShape(RoundedRectangle(cornerRadius: 4))
                                .overlay(RoundedRectangle(cornerRadius: 4).stroke(Color.gray, lineWidth: 1))
                        }

                        Text("Manifeste d'intention seul. Aucun artefact binaire disponible sur ce hub.")
                            .font(.system(size: 12))
                            .foregroundStyle(Color.gray)

                        if !caps.isEmpty {
                            HStack(spacing: 6) {
                                ForEach(caps, id: \.self) { cap in
                                    Text(cap)
                                        .font(.system(size: 11, design: .monospaced))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(Color.black.opacity(0.4))
                                        .clipShape(RoundedRectangle(cornerRadius: 4))
                                }
                            }
                        }

                        Text("Binaire WASM absent")
                            .font(.system(size: 13))
                            .foregroundStyle(Color.gray)
                            .frame(maxWidth: .infinity, minHeight: 38)
                            .background(surface2.opacity(0.5))
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                    }
                    .padding(16)
                    .background(surface1)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))
                }
            }

            // Last Execution Outcome
            if !lastAgentRun.isNull {
                VStack(alignment: .leading, spacing: 8) {
                    Text("DERNIÈRE EXÉCUTION EN BAC À SABLE")
                        .font(.system(size: 11, weight: .bold, design: .monospaced))
                        .foregroundStyle(HakilleTokens.colorSuccesDark)

                    Text("Agent : \(lastAgentRun["name"].text) (\(lastAgentRun["duration_ms"].text) ms)")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(Color.white)

                    Text("Empreinte entrée : \(String(lastAgentRun["input_sha256"].text.prefix(16)))…")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(HakilleTokens.colorLueurDark)

                    Text("Empreinte sortie : \(String(lastAgentRun["output_sha256"].text.prefix(16)))…")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(HakilleTokens.colorLueurDark)

                    Text(lastAgentRun["output_json"].text)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(Color.white)
                        .textSelection(.enabled)
                }
                .padding(16)
                .background(surface2)
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .overlay(RoundedRectangle(cornerRadius: 12).stroke(outline, lineWidth: 1))
            }
        }
        .sheet(item: $runningAgentName) { agentName in
            NavigationStack {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Entrée JSON pour \(agentName) :")
                        .font(.system(size: 14))
                        .foregroundStyle(Color.white)

                    TextEditor(text: $inputJson)
                        .font(.system(size: 13, design: .monospaced))
                        .frame(minHeight: 120)
                        .padding(8)
                        .background(surface2)
                        .clipShape(RoundedRectangle(cornerRadius: 8))

                    Button("Exécuter en bac à sable") {
                        onRunAgent(agentName, inputJson)
                        runningAgentName = nil
                    }
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .background(HakilleTokens.colorBraiseDark)
                    .foregroundStyle(Color.black)
                    .clipShape(RoundedRectangle(cornerRadius: 8))

                    Spacer()
                }
                .padding(20)
                .background(Color(hex: HakilleTokens.colorSurface0Dark))
                .navigationTitle("Lancer \(agentName)")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Annuler") { runningAgentName = nil }
                    }
                }
            }
        }
    }
}

extension String: @retroactive Identifiable {
    public var id: String { self }
}

private extension JSONValue {
    var agentName: String { self["name"].text }
}
