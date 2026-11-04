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
    @AppStorage("appearance") private var appearance = "dark"
    @AppStorage("dictationLanguage") private var dictationLanguage = "system"
    @State private var selection = 0
    @State private var activityTab = 0
    @State private var hubTab = 0
    @State private var showWriting = false
    @State private var showConversationConsent = false
    @State private var showClearDraft = false
    @State private var endpoint = ""
    @State private var token = ""
    @State private var search = ""
    @State private var memoryText = ""
    private var requestBusy: Bool { !model.snapshot["pending"].isNull }
    private var busy: Bool { requestBusy || model.dictationStatus.isBusy || !model.persistenceReady }
    private var hub: Bool { !model.snapshot["hub_id"].isNull }
    private var foreground: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorTexte1Dark : HakilleTokens.colorTexte1Light) }
    private var secondary: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorTexte2Dark : HakilleTokens.colorTexte2Light) }
    private var surface: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorSurface1Dark : HakilleTokens.colorSurface1Light) }
    private var light: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorLueurDark : HakilleTokens.colorLueurLight) }
    private var localeIdentifier: String { dictationLanguage == "system" ? Locale.current.identifier : dictationLanguage }
    private var background: Color { Color(hakille: colorScheme == .dark ? HakilleTokens.colorSurface0Dark : HakilleTokens.colorSurface0Light) }

    var body: some View {
        TabView(selection: $selection) {
            page("Voice") {
                voiceHome
                if showWriting || !model.snapshot["draft"].text.isEmpty || !model.snapshot["chat"].array.isEmpty { chat }
            }.tabItem { Label("Voice", systemImage: "waveform") }.tag(0)
            page("Memory") { memory }.tabItem { Label("Memory", systemImage: "brain") }.tag(1)
            page("Activity") {
                Section { Picker("Activity", selection: $activityTab) { Text("Journal").tag(0); Text("Agents").tag(1) }.pickerStyle(.segmented) }
                if activityTab == 0 { journal } else { agents }
            }.tabItem { Label("Activity", systemImage: "list.bullet.rectangle") }.tag(2)
            page("Hub") {
                Section { Picker("Hub", selection: $hubTab) { Text("Connection").tag(0); Text("Status").tag(1); Text("Phone").tag(2) }.pickerStyle(.segmented) }
                if hubTab == 0 { connection }
                else if hubTab == 1 { status }
                else { PhoneCapabilitiesView(phone: model.phone) }
            }.tabItem { Label("Hub", systemImage: "network") }.tag(3)
        }
        .onChange(of: selection) { destination in
            if destination != 0 { model.disableConversation(); model.cancelDictation(); model.stopSpokenReply() }
            model.dispatch("cancel_approval")
        }
        .tint(foreground).font(HakilleTypography.body)
        .environment(\.layoutDirection, forceRtl ? .rightToLeft : systemDirection)
        .preferredColorScheme(appearance == "system" ? nil : appearance == "light" ? .light : .dark)
        .confirmationDialog("Enable voice conversation?", isPresented: $showConversationConsent, titleVisibility: .visible) {
            Button("Enable conversation") { model.enableConversation() }
            Button("Cancel", role: .cancel) { }
        } message: {
            Text("Each tap on Speak records up to 30 seconds locally. Its final transcription is sent to your selected hub, and its final reply is read aloud by the installed iOS voice. No continuous listening, automatic approval or control of other apps.")
        }
        .confirmationDialog("Clear the draft?", isPresented: $showClearDraft, titleVisibility: .visible) {
            Button("Clear draft", role: .destructive) { model.dispatch("set_draft", ["text": .string("")]) }
            Button("Cancel", role: .cancel) { }
        }
    }

    private func page<Content: View>(_ title: LocalizedStringKey, @ViewBuilder content: () -> Content) -> some View {
        NavigationStack {
            List {
                Section {
                    Label(connectionLabel, systemImage: model.snapshot["connection"].text == "connected" ? "link" : "link.badge.plus").font(HakilleTypography.caption)
                    if !model.error.isEmpty { Text(model.error).foregroundStyle(foreground).textSelection(.enabled) }
                    if !model.storageError.isEmpty {
                        Text(model.storageError).foregroundStyle(foreground).textSelection(.enabled)
                        Button("Retry recovery storage") { model.retryPersistence() }
                    }
                    recoveryWarnings
                    if let outcome = model.snapshot["outcomes"].array.last { Text("\(outcome["state"].text) · \(outcome["message"].text)").textSelection(.enabled) }
                    if requestBusy {
                        ProgressView("Request in progress. No automatic retry.")
                        Button("Cancel waiting — result may be unknown") { model.cancelRequest() }
                    }
                    if model.snapshot["history_trimmed"] == .bool(true) { Text("Older entries are omitted. Consult the journal.").font(HakilleTypography.caption) }
                    ForEach(Array(model.snapshot["notifications"].array.enumerated()), id: \.offset) { _, notification in Text(notification.text).textSelection(.enabled) }
                    if !hub { Button("Connect a hub") { hubTab = 0; selection = 3 } }
                }.listRowBackground(surface)
                ForEach(model.snapshot["cards"].array, id: \.stableID) { card in
                    Section { DecisionView(card: card, approval: model.snapshot["approval"], busy: busy) }.listRowBackground(surface)
                }
                content()
            }.listStyle(.insetGrouped).scrollContentBackground(.hidden).background(background)
                .listRowSeparatorTint(Color(hakille: colorScheme == .dark ? HakilleTokens.colorLigneDark : HakilleTokens.colorLigneLight))
                .navigationTitle(title).navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .navigationBarLeading) {
                        Image("HakilleMark").renderingMode(.original).resizable().scaledToFit()
                            .frame(width: 32, height: 32)
                            .accessibilityLabel("Hakille")
                    }
                }
                .toolbarBackground(background, for: .navigationBar, .tabBar)
                .toolbarBackground(.visible, for: .navigationBar, .tabBar)
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
    private var voiceHome: some View {
        Section {
            VStack(spacing: 24) {
                VoicePresenceMark(listening: model.dictationStatus.phase == .listening, speaking: model.speaking)
                VStack(spacing: 8) {
                    Text(presenceTitle).font(HakilleTypography.title).multilineTextAlignment(.center)
                    Text(LocalizedStringKey(model.conversationEnabled ? "One tap, one voice turn with your hub." : "Speak here. Review your draft before sending."))
                        .font(HakilleTypography.caption).foregroundStyle(secondary).multilineTextAlignment(.center)
                }
                Picker("Voice mode", selection: Binding(get: { model.conversationEnabled }, set: { enabled in
                    if enabled { showConversationConsent = true } else { model.disableConversation() }
                })) {
                    Text("Dictation").tag(false)
                    Text("Conversation").tag(true)
                }.pickerStyle(.segmented).disabled(model.dictationStatus.isBusy)
                if model.speaking {
                    Button { model.stopSpokenReply() } label: { Label("Stop spoken reply", systemImage: "stop.fill").frame(maxWidth: .infinity, minHeight: 48) }.buttonStyle(.bordered)
                } else if model.dictationStatus.isBusy {
                    if model.dictationStatus.phase == .listening {
                        Button { model.stopDictation() } label: { Label("Stop speaking", systemImage: "stop.fill").frame(maxWidth: .infinity, minHeight: 48) }.buttonStyle(.bordered)
                    }
                    if model.dictationStatus.phase != .saving {
                        Button("Cancel dictation", role: .cancel) { model.cancelDictation() }.frame(minHeight: 44)
                    }
                } else {
                    Button {
                        if model.conversationEnabled { model.startConversationTurn(localeIdentifier: localeIdentifier) }
                        else { model.startDictation(localeIdentifier: localeIdentifier) }
                    } label: {
                        Label("Speak", systemImage: "mic.fill").font(HakilleTypography.heading)
                            .frame(maxWidth: .infinity, minHeight: 52).padding(.horizontal, 16)
                            .background(light, in: RoundedRectangle(cornerRadius: CGFloat(HakilleTokens.layoutRadiusControl)))
                            .foregroundStyle(background)
                    }.buttonStyle(.plain).disabled(busy || !model.snapshot["approval"].isNull)
                        .accessibilityHint(Text(LocalizedStringKey(model.conversationEnabled ? "Sends the final transcription to your hub and reads its reply." : "Records local dictation into the draft without sending it.")))
                }
                if !model.dictationStatus.message.isEmpty { Text(model.dictationStatus.message).font(HakilleTypography.caption).textSelection(.enabled) }
                if !model.voiceMessage.isEmpty { Text(model.voiceMessage).font(HakilleTypography.caption).textSelection(.enabled) }
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 16) { writingButton; journalButton }
                    VStack(spacing: 8) { writingButton; journalButton }
                }
                DisclosureGroup("Voice options") {
                    Picker("Dictation language", selection: $dictationLanguage) {
                        Text("Device language").tag("system"); Text("Français").tag("fr-FR"); Text("English").tag("en-US")
                    }.disabled(model.dictationStatus.isBusy || requestBusy || model.speaking)
                    Text("Speech recognition stays on this phone when supported. Conversation text goes to the selected hub; reply speech uses an installed iOS voice. Voice availability depends on the device and language.").font(HakilleTypography.caption).foregroundStyle(secondary)
                    if model.conversationEnabled { Button("Turn off voice conversation") { model.disableConversation() } }
                }
            }.frame(maxWidth: .infinity).padding(.vertical, 24)
        }.listRowBackground(surface)
        .onChange(of: model.dictationStatus.message) { message in
            if !message.isEmpty { UIAccessibility.post(notification: .announcement, argument: message) }
        }
    }
    private var presenceTitle: LocalizedStringKey {
        if model.speaking { return "Reading the reply" }
        switch model.dictationStatus.phase {
        case .listening: return "Listening on this phone"
        case .authorizing, .waitingForForeground, .preparing: return "Preparing the microphone"
        case .finishing: return "Finishing transcription"
        case .saving: return "Saving your words"
        default: return requestBusy ? "Waiting for the hub" : "The microphone is off"
        }
    }
    private var writingButton: some View {
        Button { showWriting.toggle() } label: { Label("Write", systemImage: "square.and.pencil").frame(minHeight: 44) }.buttonStyle(.bordered)
    }
    private var journalButton: some View {
        Button { activityTab = 0; selection = 2 } label: { Label("Journal", systemImage: "list.bullet.rectangle").frame(minHeight: 44) }.buttonStyle(.bordered)
    }
    private var chat: some View {
        Group {
            Section("Your draft") {
                TextField("Message to Hakille", text: Binding(get: { model.snapshot["draft"].text }, set: { model.dispatch("set_draft", ["text": .string($0)]) }), axis: .vertical).disabled(busy)
                    .lineLimit(3...10)
                Button("Send") { model.dispatch("submit") }.frame(minHeight: 44).disabled(!hub || busy || model.snapshot["draft"].text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                if !model.snapshot["draft"].text.isEmpty { Button("Clear draft", role: .destructive) { showClearDraft = true }.disabled(busy) }
                if model.conversationEnabled { Text("The keyboard sends text only; it does not start a spoken reply.").font(HakilleTypography.caption) }
            }.listRowBackground(surface)
            ForEach(Array(model.snapshot["chat"].array.enumerated()), id: \.offset) { _, row in
                Section(header: Text(LocalizedStringKey(row["role"].text == "user" ? "You" : "Hakille"))) {
                    Text(row["text"].text).textSelection(.enabled)
                }.listRowBackground(surface)
            }
        }
    }
    @ViewBuilder private var recoveryWarnings: some View {
        if model.snapshot["recovery_required"] == .bool(true) {
            Text("An earlier operation may have affected a hub. Check its journal before continuing. Nothing is automatically resent.")
                .font(HakilleTypography.heading).foregroundStyle(light)
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
            Text("Warning read for operation #\(operation["request_id"].text); its effect is still unknown.").font(HakilleTypography.caption).textSelection(.enabled)
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
                    Text("\(row["state"].text) · \(row["message"].text)").textSelection(.enabled)
                    if !row["result"].isNull { DisclosureGroup("Response details") { Text(row["result"].text).textSelection(.enabled) } }
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
                Section { Text(row["text"].text).textSelection(.enabled); Text("\(row["source"].text) · \(row["tags"].text)").font(HakilleTypography.caption) }
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
                Section { Text("\(row["timestamp"].text) · \(row["action"].text)").font(HakilleTypography.heading); Text(row["summary"].text).textSelection(.enabled) }
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
                Picker("Appearance", selection: $appearance) {
                    Text("Dark").tag("dark"); Text("Light").tag("light"); Text("System").tag("system")
                }
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
        Text(agent["name"].text).font(HakilleTypography.heading)
        DisclosureGroup("Agent details") { JSONDetails(value: agent) }
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
        Text("Decision required").font(HakilleTypography.heading).foregroundStyle(accent)
        Text("\(card["verb"].text) \(card["object"].text)").font(HakilleTypography.title)
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
