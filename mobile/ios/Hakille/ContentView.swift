import SwiftUI
import UIKit

extension Color {
    init(hakille hex: String) {
        let value = UInt64(hex.dropFirst(), radix: 16) ?? 0
        self.init(red: Double((value >> 16) & 255) / 255, green: Double((value >> 8) & 255) / 255, blue: Double(value & 255) / 255)
    }
}

@MainActor struct ContentView: View {
    @EnvironmentObject private var model: MobileModel
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.layoutDirection) private var systemDirection
    @AppStorage("layoutRtl") private var forceRtl = false
    @AppStorage("dictationLanguage") private var dictationLanguage = "system"
    @State private var selection = 0
    @State private var endpoint = ""
    @State private var token = ""
    @State private var search = ""
    @State private var memoryText = ""
    private var requestBusy: Bool { !model.snapshot["pending"].isNull }
    private var busy: Bool { requestBusy || model.dictationStatus.isBusy || !model.persistenceReady }
    private var hub: Bool { !model.snapshot["hub_id"].isNull }
    private var foreground: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorTexte1Dark : HakilleTokens.colorTexte1Light) }
    private var background: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorSurface0Dark : HakilleTokens.colorSurface0Light) }

    var body: some View {
        TabView(selection: $selection) {
            page("Chat") { chat }.tabItem { Label("Chat", systemImage: "bubble.left.and.bubble.right") }.tag(0)
            page("Status") { status }.tabItem { Label("Status", systemImage: "info.circle") }.tag(1)
            page("Memory") { memory }.tabItem { Label("Memory", systemImage: "brain") }.tag(2)
            page("Journal") { journal }.tabItem { Label("Journal", systemImage: "list.bullet.rectangle") }.tag(3)
            page("Agents") { agents }.tabItem { Label("Agents", systemImage: "square.stack.3d.up") }.tag(4)
            page("Connection") { connection }.tabItem { Label("Connection", systemImage: "network") }.tag(5)
        }.tint(foreground).environment(\.layoutDirection, forceRtl ? .rightToLeft : systemDirection)
    }

    private func page<Content: View>(_ title: LocalizedStringKey, @ViewBuilder content: () -> Content) -> some View {
        NavigationStack {
            List {
                Section {
                    Text(connectionLabel)
                    if !model.error.isEmpty { Text(model.error).foregroundStyle(.red).textSelection(.enabled) }
                    if !model.storageError.isEmpty {
                        Text(model.storageError).foregroundStyle(.red).textSelection(.enabled)
                        Button("Retry recovery storage") { model.retryPersistence() }
                    }
                    recoveryWarnings
                    if let outcome = model.snapshot["outcomes"].array.last { Text("\(outcome["state"].text) · \(outcome["message"].text)").textSelection(.enabled) }
                    if requestBusy {
                        ProgressView("Request in progress. No automatic retry.")
                        Button("Cancel waiting — result may be unknown") { model.cancelRequest() }
                    }
                    if model.snapshot["history_trimmed"] == .bool(true) { Text("Older entries are omitted. Consult the journal.").font(.footnote) }
                    ForEach(Array(model.snapshot["notifications"].array.enumerated()), id: \.offset) { _, notification in Text(notification.text).textSelection(.enabled) }
                    if !hub { Button("Connect a hub") { selection = 5 } }
                }
                ForEach(model.snapshot["cards"].array, id: \.stableID) { card in
                    Section { DecisionView(card: card, approval: model.snapshot["approval"], busy: busy) }
                }
                content()
            }.scrollContentBackground(.hidden).background(background).navigationTitle(title)
        }
    }
    private var connectionLabel: LocalizedStringKey {
        switch model.snapshot["connection"].text {
        case "connected": return "Connected"
        case "connecting": return "Connecting"
        case "unknown": return "Connection or outcome unknown"
        default: return "Disconnected"
        }
    }
    private var chat: some View {
        Group {
            ForEach(Array(model.snapshot["chat"].array.enumerated()), id: \.offset) { _, row in
                Section(header: Text(LocalizedStringKey(row["role"].text == "user" ? "You" : "Hakille"))) { Text(row["text"].text).textSelection(.enabled) }
            }
            Section {
                TextField("Message to Hakille", text: Binding(get: { model.snapshot["draft"].text }, set: { model.dispatch("set_draft", ["text": .string($0)]) }), axis: .vertical).disabled(busy)
                Button("Send") { model.dispatch("submit") }.disabled(!hub || busy || model.snapshot["draft"].text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                dictationControls
            }
        }
    }
    private var dictationControls: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Local dictation").font(.headline)
            Text("Up to 30 seconds, on this device only. Dictation fills the draft; it never sends a message.").font(.footnote)
            Picker("Dictation language", selection: $dictationLanguage) {
                Text("Device language").tag("system")
                Text("Français").tag("fr-FR")
                Text("English").tag("en-US")
            }.disabled(model.dictationStatus.isBusy)
            if model.dictationStatus.isBusy {
                if model.dictationStatus.phase == .listening {
                    Button { model.stopDictation() } label: { Label("Stop and use the transcription", systemImage: "stop.circle").frame(minHeight: 44) }
                }
                if model.dictationStatus.phase != .saving {
                    Button(role: .cancel) { model.cancelDictation() } label: { Text("Cancel dictation").frame(minHeight: 44) }
                }
            } else {
                Button {
                    model.startDictation(localeIdentifier: dictationLanguage == "system" ? Locale.current.identifier : dictationLanguage)
                } label: { Label("Dictate into the draft", systemImage: "mic").frame(minHeight: 44) }
                    .disabled(busy || !model.snapshot["approval"].isNull)
                    .accessibilityHint("Requests speech and microphone permission, then starts local dictation.")
            }
            if !model.dictationStatus.message.isEmpty {
                Text(model.dictationStatus.message).textSelection(.enabled)
                    .accessibilityLabel(Text(model.dictationStatus.message))
            }
        }.onChange(of: model.dictationStatus.message) { message in
            if !message.isEmpty { UIAccessibility.post(notification: .announcement, argument: message) }
        }
    }
    @ViewBuilder private var recoveryWarnings: some View {
        if model.snapshot["recovery_required"] == .bool(true) {
            Text("An earlier operation may have affected a hub. Check its journal before continuing. Nothing is automatically resent.")
                .font(.headline).foregroundStyle(.orange)
        }
        ForEach(Array(model.snapshot["uncertain_operations"].array.enumerated()), id: \.offset) { _, operation in
            if operation["phase"].text == "unknown" {
                Text("Uncertain operation #\(operation["request_id"].text) · \(operation["kind"].text)").textSelection(.enabled)
                Button("I have read this warning — the effect remains unknown") {
                    model.dispatch("acknowledge_unknown", ["request_id": operation["request_id"]])
                }.disabled(busy)
            }
        }
        ForEach(Array(model.snapshot["acknowledged_operations"].array.enumerated()), id: \.offset) { _, operation in
            Text("Warning read for operation #\(operation["request_id"].text); its effect is still unknown.").font(.footnote).textSelection(.enabled)
        }
    }
    private var status: some View {
        Group {
            Section {
                Button("Refresh") { model.dispatch("refresh_status") }.disabled(!hub || busy)
                JSONDetails(value: model.snapshot["status"])
            }
            Section("Request outcomes") {
                ForEach(Array(model.snapshot["outcomes"].array.enumerated()), id: \.offset) { _, row in
                    Text("\(row["state"].text) · \(row["message"].text)\n\(row["result"].text)").textSelection(.enabled)
                }
            }
        }
    }
    private var memory: some View {
        Group {
            Section {
                TextField("Memory search", text: $search)
                Button("Search") { model.dispatch("load_memory", ["query": search.isEmpty ? .null : .string(search), "limit": .integer(50)]) }.disabled(!hub || busy)
            }
            ForEach(Array(model.snapshot["memory"].array.enumerated()), id: \.offset) { _, row in
                Section { Text(row["text"].text).textSelection(.enabled); Text("\(row["source"].text) · \(row["tags"].text)").font(.footnote) }
            }
            Section {
                TextField("Text to remember", text: $memoryText, axis: .vertical)
                Button("Save memory") { model.dispatch("remember", ["text": .string(memoryText)]) }.disabled(!hub || busy || memoryText.isEmpty)
            }
        }
    }
    private var journal: some View {
        Group {
            Section { Button("Refresh") { model.dispatch("load_journal", ["limit": .integer(50)]) }.disabled(!hub || busy) }
            ForEach(Array(model.snapshot["journal"].array.enumerated()), id: \.offset) { _, row in
                Section { Text("\(row["timestamp"].text) · \(row["action"].text)").font(.headline); Text(row["summary"].text).textSelection(.enabled) }
            }
        }
    }
    private var agents: some View {
        Group {
            Section { Button("Refresh") { model.dispatch("load_agents") }.disabled(!hub || busy) }
            ForEach(model.snapshot["agents"].array, id: \.agentName) { agent in Section { AgentView(agent: agent, enabled: hub && !busy) } }
        }
    }
    private var connection: some View {
        Group {
            Section {
                Toggle("Right-to-left layout", isOn: $forceRtl)
                Text("Your gateway connects this app to Hakille. Use HTTPS, or 127.0.0.1 through a local tunnel. Keep the desktop client disconnected.")
                TextField("Gateway base URL", text: $endpoint).textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.URL)
                    .onChange(of: endpoint) { _ in token = "" }
                SecureField("Mobile gateway token", text: $token).textInputAutocapitalization(.never).autocorrectionDisabled()
                Button("Connect and save securely") { model.connect(endpoint: endpoint, token: token); token = "" }.disabled(busy || endpoint.isEmpty || token.isEmpty)
            }
            if !model.savedEndpoint.isEmpty {
                Section {
                    Text(model.savedEndpoint)
                    Button("Reconnect saved hub") { model.connect(endpoint: "", token: "", useSaved: true) }.disabled(busy)
                }
            }
            Section { Button("Disconnect and erase credentials", role: .destructive) { model.disconnect(); token = "" } }
        }
    }
}

private extension JSONValue {
    var stableID: String { self["id"].text }
    var agentName: String { self["name"].text }
}

private struct JSONDetails: View {
    let value: JSONValue
    var body: some View {
        if value.isNull { Text("No data received.") }
        else { ForEach(value.object.keys.sorted(), id: \.self) { key in Text("\(key): \(value[key].text)").textSelection(.enabled) } }
    }
}

@MainActor private struct AgentView: View {
    @EnvironmentObject private var model: MobileModel
    let agent: JSONValue
    let enabled: Bool
    @State private var input = "{}"
    var body: some View {
        Text(agent["name"].text).font(.headline)
        JSONDetails(value: agent)
        if agent["artifact_present"] == .bool(true) {
            TextField("Agent input (JSON)", text: $input, axis: .vertical).autocorrectionDisabled().textInputAutocapitalization(.never)
            Button("Run this agent") { model.dispatch("run_agent", ["name": agent["name"], "input_json": .string(input)]) }.disabled(!enabled)
        } else { Text("Agent artifact unavailable") }
    }
}

@MainActor private struct DecisionView: View {
    @EnvironmentObject private var model: MobileModel
    @Environment(\.colorScheme) private var colorScheme
    let card: JSONValue
    let approval: JSONValue
    let busy: Bool
    @State private var started = false
    @State private var gestureGeneration: UInt64 = 0
    @GestureState private var touching = false
    private var armed: Bool { approval["card_id"] == card["id"] }
    private var accent: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorBraiseDark : HakilleTokens.colorBraiseLight) }
    private var onAccent: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorSurBraiseDark : HakilleTokens.colorSurBraiseLight) }
    private func action(_ type: String) { model.dispatch(type, ["card_id": card["id"]]) }
    var body: some View {
        Text("Decision required").font(.headline).foregroundStyle(accent)
        Text("\(card["verb"].text) \(card["object"].text)").font(.title2)
        Text("Reason: \(card["reason"].text)")
        Text("Cost: \(CardPresentation.money(card["cost"]))")
        Text("Data destination: \(CardPresentation.destination(card["data_exit"]))")
        Text("Provenance: \(CardPresentation.provenance(card["provenance"]))")
        Text(LocalizedStringKey(card["reversible"] == .bool(true) ? "Reversible" : "Irreversible"))
        if !busy {
            Text(LocalizedStringKey(armed ? "Confirm approval after 600 ms" : "Hold for 600 ms, then release to approve"))
                .frame(maxWidth: .infinity, minHeight: 56).padding(8).background(accent, in: RoundedRectangle(cornerRadius: 12)).foregroundStyle(onAccent)
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0)
                    .updating($touching) { _, state, _ in state = true }
                    .onChanged { _ in if !started { gestureGeneration &+= 1; started = true; action("begin_approval") } }
                    .onEnded { value in
                        gestureGeneration &+= 1
                        let wasStarted = started; started = false
                        if wasStarted && abs(value.translation.width) < 20 && abs(value.translation.height) < 20 { action("approve") }
                        else { model.dispatch("cancel_approval") }
                    })
                .onChange(of: touching) { active in
                    guard !active && started else { return }
                    let epoch = gestureGeneration
                    // Laisser onEnded finaliser le même geste ; une annulation
                    // différée ne doit jamais toucher le geste suivant.
                    DispatchQueue.main.async {
                        if gestureGeneration == epoch && started && !touching {
                            gestureGeneration &+= 1; started = false
                            model.dispatch("cancel_approval")
                        }
                    }
                }
                .onDisappear { gestureGeneration &+= 1; started = false; model.dispatch("cancel_approval") }
                .accessibilityAddTraits(.isButton)
                .accessibilityLabel("Approve in two steps. Activate once to begin confirmation, then again after 600 milliseconds.")
                .accessibilityAction { action(armed ? "approve" : "begin_approval") }
            Button("Reject", role: .destructive) { action("reject") }
            if armed { Button("Cancel confirmation") { model.dispatch("cancel_approval") } }
        }
    }
}
