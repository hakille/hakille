import Foundation
import Combine

@MainActor final class MobileModel: ObservableObject {
    @Published private(set) var snapshot: JSONValue = .object([:])
    @Published private(set) var error = ""
    @Published private(set) var storageError = ""
    @Published private(set) var savedEndpoint = ""
    @Published private(set) var dictationStatus = DictationStatus()
    @Published private(set) var conversationEnabled = false
    @Published private(set) var speaking = false
    @Published private(set) var voiceMessage = ""
    @Published private(set) var requestStartTime: Date?
    let phone = PhoneCapabilities()
    private var voicePolicy = VoiceConversationPolicy()
    private var voiceCaptureGeneration: UUID?
    private var voiceLocale = Locale.current.identifier
    private lazy var speechOutput = NativeSpeechOutput { [weak self] active, message in
        self?.speaking = active; self?.voiceMessage = message
    }
    private var bridge = MobileBridge()
    private let store = CredentialStore()
    private let stateStore: MobileStateIO
    private var persistenceTail: Task<Bool, Never>?
    private var restorationTask: Task<Void, Never>?
    private let gateway = GatewayTransport()
    private var stateLoaded = false
    private var credentials: HubCredentials?
    private var generation: UInt64 = 0
    private var requestTask: Task<Void, Never>?
    private var transportStartedID: UInt64?
    private var configurationTask: Task<Void, Never>?
    private var isForeground = false
    private var dictationOriginal = ""
    private lazy var dictation = NativeDictationController(
        onStatus: { [weak self] in self?.receiveDictationStatus($0) },
        onFinal: { [weak self] text in
            guard let self else { throw DictationFailure.interrupted }
            return try self.applyDictatedText(text)
        })

    init(stateStore: MobileStatePersistence = MobileStateStore()) {
        self.stateStore = MobileStateIO(stateStore)
        snapshot = (try? .parse(bridge.snapshotJson())) ?? .null
        retryPersistence()
        let epoch = generation
        Task {
            do { let saved = try await store.load(); if generation == epoch { savedEndpoint = saved?.endpoint ?? "" } }
            catch { if generation == epoch { self.error = String(localized: "Saved credentials unavailable. Reconnect or erase them.") } }
        }
    }
    var persistenceReady: Bool { stateLoaded && storageError.isEmpty }
    func retryPersistence() {
        guard restorationTask == nil else { return }
        restorationTask = Task {
            defer { restorationTask = nil }
            do {
                if !stateLoaded {
                    if let checkpoint = try await stateStore.load() {
                        let result = try JSONValue.parse(bridge.restoreDraftJson(draftJson: checkpoint))
                        guard result["ok"] == .bool(true) else { throw MobileStateError.invalidCheckpoint }
                        snapshot = result["snapshot"]
                    }
                    stateLoaded = true
                }
                _ = await enqueueSave(bridge.draftJson()).value
            } catch { storageError = String(localized: "Recovery storage is unavailable. No request will be sent. Unlock the device and retry.") }
        }
    }
    private func enqueueSave(_ checkpoint: String) -> Task<Bool, Never> {
        let previous = persistenceTail
        let task = Task { [self] in
            // Attendre aussi un échec précédent : une nouvelle sauvegarde
            // explicite ne doit jamais être dépassée par une écriture ancienne.
            _ = await previous?.value
            do {
                try await stateStore.save(checkpoint)
                storageError = ""
                return true
            } catch {
                storageError = String(localized: "Recovery storage is unavailable. No request will be sent. Unlock the device and retry.")
                return false
            }
        }
        persistenceTail = task
        return task
    }
    @discardableResult private func resetConnection() -> UInt64 {
        generation &+= 1; configurationTask?.cancel(); requestTask?.cancel(); requestTask = nil; credentials = nil
        disableConversation()
        dictation.cancel()
        cancelUnsentRequest()
        transportStartedID = nil
        // Interrompre avant l'export : une opération en vol devient inconnue.
        // Le nouveau bridge conserve son compteur et tous les avertissements,
        // y compris lors d'un changement de hub ou d'un effacement du token.
        _ = accept(bridge.dispatchJson(actionJson: "{\"type\":\"set_foreground\",\"active\":false}"))
        let replacement = MobileBridge()
        if let result = try? JSONValue.parse(replacement.restoreDraftJson(draftJson: bridge.draftJson())), result["ok"] == .bool(true) {
            bridge = replacement; snapshot = result["snapshot"]
            _ = accept(bridge.dispatchJson(actionJson: "{\"type\":\"set_foreground\",\"active\":\(isForeground)}"))
        } else { storageError = String(localized: "Recovery storage is unavailable. No request will be sent. Unlock the device and retry.") }
        return generation
    }
    func connect(endpoint: String, token: String, useSaved: Bool = false) {
        guard persistenceReady else { return }
        let epoch = resetConnection()
        guard persistenceReady else { return }
        configurationTask = Task {
            do {
                let candidate: HubCredentials
                if useSaved {
                    guard let saved = try await store.load() else { throw GatewayError.storage }; candidate = saved
                } else { candidate = HubCredentials(endpoint: endpoint.trimmingCharacters(in: .whitespacesAndNewlines), token: token.trimmingCharacters(in: .whitespacesAndNewlines)) }
                let checked = HubCredentials(endpoint: try EndpointPolicy.canonical(candidate.endpoint), token: candidate.token)
                try EndpointPolicy.validateToken(checked.token); try Task.checkCancellation()
                guard generation == epoch else { return }
                try await store.save(checked, generation: epoch); try Task.checkCancellation()
                guard generation == epoch else { return }
                credentials = checked; savedEndpoint = checked.endpoint; error = ""
                dispatch("set_hub", ["hub_id": .string(checked.endpoint)]); dispatch("refresh_status")
            } catch is CancellationError { }
            catch { if generation == epoch { self.error = String(localized: "Connection configuration failed. Check URL, token and secure storage.") } }
        }
    }
    func disconnect() {
        let epoch = resetConnection(); savedEndpoint = ""
        Task { do { try await store.clear(generation: epoch) } catch { if generation == epoch { self.error = String(localized: "Could not erase saved credentials.") } } }
    }
    func dispatch(_ type: String, _ fields: [String: JSONValue] = [:]) {
        guard persistenceReady else { return }
        guard !dictationStatus.isBusy || type == "cancel_approval" else {
            error = DictationFailure.busy.localizedDescription; return
        }
        var action = fields; action["type"] = .string(type)
        guard let json = try? JSONValue.object(action).encoded() else { return }
        _ = accept(bridge.dispatchJson(actionJson: json))
    }
    func foreground(_ active: Bool, background: Bool = false) {
        isForeground = active
        phone.foreground(active, background: background)
        let permissionDialog = !background && (dictationStatus.phase == .authorizing || dictationStatus.phase == .waitingForForeground)
        if !active {
            dictation.foreground(false, background: background)
            if !permissionDialog { disableConversation() }
            speechOutput.stop()
            generation &+= 1; requestTask?.cancel(); requestTask = nil; configurationTask?.cancel()
            cancelUnsentRequest(); transportStartedID = nil
        }
        _ = accept(bridge.dispatchJson(actionJson: "{\"type\":\"set_foreground\",\"active\":\(active)}"))
        // Le cœur redevient actif avant le callback d'armement vocal.
        if active { dictation.foreground(true, background: false) }
    }
    func enableConversation() {
        guard isForeground, !dictationStatus.isBusy, persistenceReady else { return }
        voicePolicy.enable(); conversationEnabled = true
        voiceMessage = String(localized: "Voice mode enabled. Each tap sends one final transcription to the hub, then reads its reply.")
    }
    func disableConversation() {
        let hadVoice = voicePolicy.enabled || voiceCaptureGeneration != nil
        let requestID = snapshot["voice_request_id"].id
        voicePolicy.disable(); conversationEnabled = false; voiceCaptureGeneration = nil
        speechOutput.stop()
        if hadVoice { dictation.cancel() }
        if let requestID, snapshot["pending"]["request_id"].id == requestID { cancelRequest() }
        if hadVoice { _ = accept(bridge.dispatchJson(actionJson: "{\"type\":\"cancel_voice\"}")) }
        voiceMessage = ""
    }
    func startConversationTurn(localeIdentifier: String) {
        guard conversationEnabled, isForeground, persistenceReady, snapshot["pending"].isNull, !dictationStatus.isBusy else { return }
        guard snapshot["draft"].text.isEmpty else {
            voiceMessage = String(localized: "Send or clear the existing draft before starting a voice turn."); return
        }
        guard snapshot["connection"].text == "connected", snapshot["cards"].array.isEmpty, snapshot["recovery_required"] != .bool(true) else {
            voiceMessage = String(localized: "Connect and refresh the hub, and resolve pending decisions before starting a voice turn."); return
        }
        speechOutput.stop(); voiceLocale = localeIdentifier
        let epoch = voicePolicy.generation; voiceCaptureGeneration = epoch
        dictation.start(localeIdentifier: localeIdentifier, beforeCapture: { [weak self] in
            guard let self, self.voicePolicy.enabled, self.voicePolicy.generation == epoch, self.isForeground, self.persistenceReady else { throw DictationFailure.interrupted }
            let json = self.bridge.dispatchJson(actionJson: "{\"type\":\"begin_voice\"}")
            guard let result = try? JSONValue.parse(json), result["ok"] == .bool(true), let context = result["snapshot"]["voice_context_id"].id else {
                _ = self.accept(json); throw DictationFailure.busy
            }
            self.voicePolicy.arm(context: context); _ = self.accept(json)
        }, completionMessage: String(localized: "Transcription ready. Follow the hub request status."))
    }
    func stopSpokenReply() { speechOutput.stop(message: String(localized: "Spoken reply stopped. Read the reply on screen.")) }
    private func receiveDictationStatus(_ status: DictationStatus) {
        dictationStatus = status
        if status.phase == .failed || status.phase == .cancelled, voiceCaptureGeneration != nil {
            voiceCaptureGeneration = nil; voicePolicy.clearTurn()
            _ = accept(bridge.dispatchJson(actionJson: "{\"type\":\"cancel_voice\"}"))
            voiceMessage = status.message
        }
    }
    private func updateVoiceReply() {
        guard voicePolicy.enabled else { return }
        if let context = voicePolicy.context, snapshot["voice_context_id"].id != context {
            disableConversation(); return
        }
        let reply = snapshot["voice_reply"]
        if reply.isNull, let request = snapshot["voice_request_id"].id, snapshot["pending"].isNull,
           snapshot["outcomes"].array.contains(where: { $0["request_id"].id == request && $0["state"].text == "confirmed" }) {
            voiceCaptureGeneration = nil
            voiceMessage = String(localized: "The hub reply is available on screen, but has no eligible spoken version.")
            return
        }
        guard let context = reply["context_id"].id, let request = reply["request_id"].id,
              voicePolicy.consume(VoiceReplyIdentity(context: context, request: request), foreground: isForeground,
                                  context: snapshot["voice_context_id"].id, request: snapshot["voice_request_id"].id) else { return }
        voiceCaptureGeneration = nil
        speechOutput.speak(reply["text"].text, localeIdentifier: voiceLocale)
    }
    func startDictation(localeIdentifier: String) {
        guard persistenceReady, snapshot["pending"].isNull, snapshot["approval"].isNull, !dictationStatus.isBusy else {
            error = DictationFailure.busy.localizedDescription; return
        }
        disableConversation()
        dictationOriginal = snapshot["draft"].text
        dictation.start(localeIdentifier: localeIdentifier)
    }
    func stopDictation() { dictation.stop() }
    func cancelDictation() { if conversationEnabled { disableConversation() } else { dictation.cancel() } }
    private func applyDictatedText(_ text: String) throws -> Task<Bool, Never> {
        guard isForeground else { throw DictationFailure.interrupted }
        if let epoch = voiceCaptureGeneration {
            guard let context = voicePolicy.context, voicePolicy.accepts(context: context, generation: epoch),
                  snapshot["voice_context_id"].id == context, persistenceReady else { throw DictationFailure.interrupted }
            let transcript = try VoiceConversationPolicy.validatedTranscript(text)
            let action = try JSONValue.object(["type": .string("submit_voice"), "context_id": .unsigned(context), "text": .string(transcript)]).encoded()
            return accept(bridge.dispatchJson(actionJson: action))
        }
        let draft = try DictationPolicy.completedDraft(original: dictationOriginal, current: snapshot["draft"].text, transcript: text)
        guard persistenceReady else { throw DictationFailure.storageUnavailable }
        let action = try JSONValue.object(["type": .string("set_draft"), "text": .string(draft)]).encoded()
        // Aucun submit : seul le bouton Envoyer peut émettre ce brouillon.
        return accept(bridge.dispatchJson(actionJson: action))
    }
    func cancelRequest() {
        guard let id = snapshot["pending"]["request_id"].id else { return }
        generation &+= 1; requestTask?.cancel(); requestTask = nil
        requestStartTime = nil
        let response = transportStartedID == id
            ? bridge.failRequest(requestId: id, message: "Transport cancelled; outcome unknown / Transport annulé, résultat inconnu")
            : failBeforeSend(id: id, message: "Cancelled before transport / Annulée avant émission")
        transportStartedID = nil
        _ = accept(response)
    }
    func ghostActionAccept(id: String) { dispatch("ghost_action_accept", ["id": .string(id)]) }
    func ghostActionDismiss(id: String) { dispatch("ghost_action_dismiss", ["id": .string(id)]) }

    func undoAction(journalId: UInt64) {
        dispatch("undo", ["journal_id": .unsigned(journalId)])
    }
    func forgetFact(rowId: String) {
        dispatch("forget_memory", ["row_id": .string(rowId)])
    }
    func setAutonomyCrank(domain: String, crank: Int, maxCostCents: UInt64? = nil, maxActions: UInt32? = nil, daysValid: UInt32? = nil) {
        var fields: [String: JSONValue] = [
            "domain": .string(domain),
            "crank": .unsigned(UInt64(crank))
        ]
        if let maxCostCents { fields["max_cost_cents"] = .unsigned(maxCostCents) }
        if let maxActions { fields["max_actions"] = .unsigned(UInt64(maxActions)) }
        if let daysValid { fields["days_valid"] = .unsigned(UInt64(daysValid)) }
        dispatch("set_autonomy_crank", fields)
    }
    func requestWhy(targetId: String, targetType: String) {
        dispatch("request_why", ["target_id": .string(targetId), "target_type": .string(targetType)])
    }
    func closeWhy() {
        dispatch("close_why")
    }
    func runAgent(name: String, inputJson: String) {
        dispatch("run_agent", ["name": .string(name), "input_json": .string(inputJson)])
    }
    func loadJournal(limit: Int = 50) {
        dispatch("load_journal", ["limit": .unsigned(UInt64(limit))])
    }
    func loadMemory(query: String? = nil, limit: Int = 50) {
        var fields: [String: JSONValue] = ["limit": .unsigned(UInt64(limit))]
        if let query, !query.isEmpty { fields["query"] = .string(query) }
        dispatch("load_memory", fields)
    }
    func rememberFact(text: String) {
        dispatch("remember", ["text": .string(text)])
    }
    func loadAgents() {
        dispatch("load_agents")
    }
    func dismissPrivacyGuard() {
        dispatch("dismiss_privacy_guard")
    }
    func startGuestPass(name: String, power: String, durationSecs: UInt32 = 3600) {
        dispatch("start_guest_pass", [
            "name": .string(name),
            "power": .string(power),
            "duration_secs": .unsigned(UInt64(durationSecs))
        ])
    }
    func tickGuestPass(elapsedSecs: UInt32) {
        dispatch("tick_guest_pass", ["elapsed_secs": .unsigned(UInt64(elapsedSecs))])
    }
    func revokeGuestPass() {
        dispatch("revoke_guest_pass")
    }
    func connectFromPairingQr(_ rawQrJson: String) {
        do {
            let parsed = try EndpointPolicy.parsePairingQr(rawQrJson)
            connect(endpoint: parsed.endpoint, token: parsed.token, useSaved: false)
        } catch {
            self.error = String(localized: "Invalid pairing QR code.")
        }
    }

    private func cancelUnsentRequest() {
        guard let id = snapshot["pending"]["request_id"].id, transportStartedID != id else { return }
        requestStartTime = nil
        _ = accept(failBeforeSend(id: id, message: "Interrupted before transport / Interrompue avant émission"))
    }
    @discardableResult private func accept(_ json: String) -> Task<Bool, Never> {
        guard let result = try? JSONValue.parse(json), !result["snapshot"].isNull else {
            error = String(localized: "Invalid core response."); return Task { false }
        }
        snapshot = result["snapshot"]; error = result["error"].text
        updateVoiceReply()
        guard stateLoaded else { return Task { false } }
        let frame = result["request"]
        let epoch = generation
        let sessionID = snapshot["gateway_session"].text
        let checkpoint = enqueueSave(bridge.draftJson())
        if frame.isNull {
            requestStartTime = nil
        } else {
            requestStartTime = Date()
        }
        return Task {
            let saved = await checkpoint.value
            guard !frame.isNull, let id = frame["id"].id else { return saved && result["ok"] == .bool(true) }
            // Une interruption/reconnexion pendant fsync annule l'émission.
            guard generation == epoch, snapshot["pending"]["request_id"].id == id else { return false }
            guard saved else {
                requestStartTime = nil
                let failed = failBeforeSend(id: id, message: "Checkpoint unavailable; request was not sent / Sauvegarde indisponible ; requête non envoyée")
                if let value = try? JSONValue.parse(failed) { snapshot = value["snapshot"] }
                return false
            }
            guard let credentials else {
                requestStartTime = nil
                return await accept(failBeforeSend(id: id, message: "Connect to a hub first; request was not sent / Connectez d’abord un hub ; requête non envoyée")).value
            }
            let isChat = frame["body"]["Request"]["message"]["HudSubmit"] != .null
            requestTask = Task {
                do {
                    try Task.checkCancellation()
                    guard generation == epoch, snapshot["pending"]["request_id"].id == id else { return }
                    transportStartedID = id
                    if isChat {
                        try await gateway.sendStream(credentials: credentials, frame: frame.encoded(), sessionID: sessionID) { [weak self] chunk in
                            guard let self, self.generation == epoch else { return }
                            Task { @MainActor in
                                let res = self.bridge.receiveStreamFrame(requestId: id, frameJson: chunk)
                                if let parsed = try? JSONValue.parse(res), !parsed["snapshot"].isNull {
                                    self.snapshot = parsed["snapshot"]
                                    self.error = parsed["error"].text
                                }
                            }
                        }
                        try Task.checkCancellation()
                        if generation == epoch {
                            requestTask = nil; transportStartedID = nil; requestStartTime = nil
                            updateVoiceReply()
                            _ = enqueueSave(bridge.draftJson())
                        }
                    } else {
                        let response = try await gateway.send(credentials: credentials, frame: frame.encoded(), sessionID: sessionID)
                        try Task.checkCancellation()
                        if generation == epoch {
                            requestTask = nil; transportStartedID = nil; requestStartTime = nil
                            _ = accept(bridge.receiveJson(requestId: id, responseJson: response))
                        }
                    }
                } catch is CancellationError { }
                catch {
                    if generation == epoch {
                        requestTask = nil; transportStartedID = nil; requestStartTime = nil
                        _ = accept(bridge.failRequest(requestId: id, message: "Gateway request failed; no automatic retry / Requête interrompue, aucune relance automatique"))
                    }
                }
            }
            return true
        }
    }
    private func failBeforeSend(id: UInt64, message: String) -> String {
        let action = JSONValue.object(["type": .string("fail_before_send"), "request_id": .unsigned(id), "message": .string(message)])
        guard let json = try? action.encoded() else { return bridge.snapshotJson() }
        return bridge.dispatchJson(actionJson: json)
    }
}
