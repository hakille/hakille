import AVFoundation
import Foundation
import Speech
import UIKit

/// Le callback audio ne touche ni SwiftUI ni un acteur : le verrou protège
/// l'append contre la fermeture de la requête, y compris un dernier buffer tardif.
private final class DictationAudioSink: @unchecked Sendable {
    private let lock = NSLock()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var receivedFrames = false
    init(_ request: SFSpeechAudioBufferRecognitionRequest) { self.request = request }
    func makeTap() -> AVAudioNodeTapBlock {
        { [self] buffer, _ in
            lock.lock(); defer { lock.unlock() }
            guard let request else { return }
            if buffer.frameLength > 0 { receivedFrames = true; request.append(buffer) }
        }
    }
    var hasAudio: Bool { lock.lock(); defer { lock.unlock() }; return receivedFrames }
    func endAudio() {
        lock.lock(); let current = request; request = nil; lock.unlock()
        current?.endAudio()
    }
}

/// Possède et ferme les ressources natives même si l'hôte disparaît.
private final class DictationResources {
    let engine = AVAudioEngine()
    var sink: DictationAudioSink?
    var recognitionTask: SFSpeechRecognitionTask?
    var tapInstalled = false
    var sessionActive = false
    @discardableResult func stopCapture() -> Bool {
        engine.stop()
        if tapInstalled { engine.inputNode.removeTap(onBus: 0); tapInstalled = false }
        sink?.endAudio()
        if sessionActive {
            do {
                try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
                sessionActive = false
            } catch { return false }
        }
        return true
    }
    @discardableResult func cancel() -> Bool {
        let restored = stopCapture()
        recognitionTask?.cancel(); recognitionTask = nil
        return restored
    }
    deinit { cancel() }
}

private final class DictationObservers {
    var tokens: [NSObjectProtocol] = []
    deinit { for token in tokens { NotificationCenter.default.removeObserver(token) } }
}

@MainActor final class NativeDictationController {
    private var policy = DictationPolicy()
    private var completionGeneration: UUID?
    private var capturePreparation: (() throws -> Void)?
    private var completionMessage = String(localized: "Dictation added to the draft. Review it before sending.")
    private var isForeground = false
    private var recognizer: SFSpeechRecognizer?
    private var resources: DictationResources?
    private var deadline: Task<Void, Never>?
    private var watchdog: Task<Void, Never>?
    private let observers = DictationObservers()
    private let onStatus: (DictationStatus) -> Void
    private let onFinal: (String) throws -> Task<Bool, Never>

    init(onStatus: @escaping (DictationStatus) -> Void, onFinal: @escaping (String) throws -> Task<Bool, Never>) {
        self.onStatus = onStatus; self.onFinal = onFinal
        installObservers()
    }
    func start(localeIdentifier: String, beforeCapture: (() throws -> Void)? = nil, completionMessage: String = String(localized: "Dictation added to the draft. Review it before sending.")) {
        do {
            let token = try policy.begin(foreground: isForeground)
            completionGeneration = nil
            capturePreparation = beforeCapture; self.completionMessage = completionMessage
            publish(String(localized: "Requesting speech and microphone permissions…"))
            guard let candidate = SFSpeechRecognizer(locale: Locale(identifier: localeIdentifier)) else { throw DictationFailure.unsupportedLanguage }
            guard candidate.supportsOnDeviceRecognition else { throw DictationFailure.localRecognitionUnavailable }
            recognizer = candidate
            Self.requestSpeechAuthorization { [weak self] status in self?.speechAuthorized(status, token: token) }
        } catch { fail(error) }
    }
    func foreground(_ active: Bool, background: Bool) {
        isForeground = active
        if active, policy.phase == .waitingForForeground, let token = policy.generation { beginCapture(token: token) }
        else if !active, policy.leaveForeground(background: background) { stopResources(); publish(DictationFailure.interrupted.localizedDescription) }
    }
    func cancel() {
        if completionGeneration != nil {
            // Le texte final est déjà ajouté ; seule sa sauvegarde attend.
            // Invalider son annonce tardive, sans prétendre annuler ce texte.
            completionGeneration = nil
            publish(String(localized: "Dictation finished. Review the draft before sending."))
            return
        }
        guard policy.phase.isBusy else { return }
        policy.cancel(); let restored = stopResources()
        publish(restored ? String(localized: "Dictation cancelled. The draft was not changed.") : DictationFailure.audioSessionFailure.localizedDescription)
    }
    func stop() {
        guard policy.phase == .listening, let token = policy.generation else { return }
        finishCapture(token: token)
    }
    private func speechAuthorized(_ status: Int, token: UUID) {
        guard policy.accepts(token) else { return }
        switch SFSpeechRecognizerAuthorizationStatus(rawValue: status) {
        case .authorized:
            Self.requestMicrophoneAuthorization { [weak self] allowed in self?.microphoneAuthorized(allowed, token: token) }
        case .restricted: fail(DictationFailure.speechRestricted)
        default: fail(DictationFailure.speechDenied)
        }
    }
    private func microphoneAuthorized(_ allowed: Bool, token: UUID) {
        guard policy.accepts(token) else { return }
        guard allowed else { fail(DictationFailure.microphoneDenied); return }
        if isForeground && UIApplication.shared.applicationState == .active { beginCapture(token: token) }
        else {
            _ = policy.transition(.waitingForForeground, token: token)
            publish(String(localized: "Permissions granted. Waiting for the app to be active…"))
        }
    }
    private func beginCapture(token: UUID) {
        guard policy.accepts(token), let recognizer else { return }
        guard isForeground, UIApplication.shared.applicationState == .active else {
            _ = policy.transition(.waitingForForeground, token: token)
            publish(String(localized: "Permissions granted. Waiting for the app to be active…"))
            return
        }
        do {
            try DictationPolicy.requireLocalSupport(supported: recognizer.supportsOnDeviceRecognition, available: recognizer.isAvailable)
            try capturePreparation?(); capturePreparation = nil
            _ = policy.transition(.preparing, token: token)
            let current = DictationResources(); resources = current
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: [.allowBluetooth])
            try session.setActive(true); current.sessionActive = true
            let input = current.engine.inputNode
            let format = input.outputFormat(forBus: 0)
            guard format.sampleRate.isFinite, format.sampleRate > 0, format.channelCount > 0 else { throw DictationFailure.audioUnavailable }
            let request = SFSpeechAudioBufferRecognitionRequest()
            // Obligatoire : aucune nouvelle tentative avec reconnaissance réseau.
            request.requiresOnDeviceRecognition = true
            request.shouldReportPartialResults = false
            let sink = DictationAudioSink(request); current.sink = sink
            input.installTap(onBus: 0, bufferSize: 1024, format: format, block: sink.makeTap())
            current.tapInstalled = true
            current.recognitionTask = recognizer.recognitionTask(with: request, resultHandler: Self.resultHandler { [weak self] text, final, failed in
                self?.receive(text: text, final: final, failed: failed, token: token)
            })
            current.engine.prepare(); try current.engine.start()
            _ = policy.transition(.listening, token: token)
            publish(String(localized: "Listening on this device — 30 seconds maximum."))
            deadline = Task { [weak self] in
                do { try await Task.sleep(nanoseconds: DictationPolicy.maximumCaptureSeconds * 1_000_000_000) } catch { return }
                guard let self, self.policy.accepts(token) else { return }
                self.finishCapture(token: token)
            }
            watchdog = Task { [weak self] in
                do { try await Task.sleep(nanoseconds: 3_000_000_000) } catch { return }
                guard let self, self.policy.accepts(token), self.policy.phase == .listening else { return }
                if self.resources?.sink?.hasAudio != true { self.fail(DictationFailure.audioUnavailable) }
            }
        } catch { fail(error as? DictationFailure ?? .audioUnavailable) }
    }
    private func finishCapture(token: UUID) {
        guard policy.accepts(token), policy.phase == .listening else { return }
        deadline?.cancel(); watchdog?.cancel()
        _ = policy.transition(.finishing, token: token)
        guard resources?.stopCapture() == true else { fail(DictationFailure.audioSessionFailure); return }
        publish(String(localized: "Microphone stopped. Finishing local recognition…"))
        deadline = Task { [weak self] in
            do { try await Task.sleep(nanoseconds: DictationPolicy.finalizationSeconds * 1_000_000_000) } catch { return }
            guard let self, self.policy.accepts(token) else { return }
            self.fail(DictationFailure.finalizationTimeout)
        }
    }
    private func receive(text: String?, final: Bool, failed: Bool, token: UUID) {
        guard policy.accepts(token) else { return }
        guard !failed else { fail(DictationFailure.recognitionFailed); return }
        guard final, let text else { return }
        guard isForeground else { fail(DictationFailure.interrupted); return }
        guard policy.finish(token: token) else { return }
        guard stopResources() else { fail(DictationFailure.audioSessionFailure); return }
        do {
            // L'ajout au brouillon est synchrone, la barrière disque est hors
            // MainActor. Aucun nouveau callback ne peut réappliquer ce texte.
            let saved = try onFinal(text)
            completionGeneration = token
            onStatus(DictationStatus(phase: .saving, message: String(localized: "Saving the dictated draft…")))
            Task { [weak self] in
                let successful = await saved.value
                guard let self, self.policy.phase == .completed, self.completionGeneration == token else { return }
                self.completionGeneration = nil
                if successful { self.publish(self.completionMessage) }
                else { self.fail(DictationFailure.storageUnavailable) }
            }
        } catch { fail(error as? DictationFailure ?? .storageUnavailable) }
    }

    private func fail(_ error: Error) {
        completionGeneration = nil
        policy.cancel(failed: true)
        let restored = stopResources()
        publish(restored ? error.localizedDescription : DictationFailure.audioSessionFailure.localizedDescription)
    }
    @discardableResult private func stopResources() -> Bool {
        deadline?.cancel(); deadline = nil; watchdog?.cancel(); watchdog = nil; capturePreparation = nil
        let restored = resources?.cancel() ?? true
        resources = nil; recognizer = nil
        return restored
    }
    private func publish(_ message: String) { onStatus(DictationStatus(phase: policy.phase, message: message)) }
    private func interrupt() { if policy.phase.isBusy { fail(DictationFailure.interrupted) } }
    private func installObservers() {
        let center = NotificationCenter.default
        for name in [AVAudioSession.interruptionNotification, AVAudioSession.mediaServicesWereLostNotification, AVAudioSession.mediaServicesWereResetNotification] {
            observers.tokens.append(center.addObserver(forName: name, object: nil, queue: nil, using: Self.interruptionHandler { [weak self] in self?.interrupt() }))
        }
        observers.tokens.append(center.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: nil, using: Self.routeHandler { [weak self] in self?.interrupt() }))
    }
    // Fabriques non isolées : les closures ObjC sont appelées sur les files
    // audio/Speech, jamais avec une précondition implicite MainActor.
    nonisolated private static func requestSpeechAuthorization(_ receive: @escaping @MainActor @Sendable (Int) -> Void) {
        SFSpeechRecognizer.requestAuthorization { status in
            let raw = status.rawValue
            Task { @MainActor in receive(raw) }
        }
    }
    nonisolated private static func requestMicrophoneAuthorization(_ receive: @escaping @MainActor @Sendable (Bool) -> Void) {
        AVAudioSession.sharedInstance().requestRecordPermission { allowed in Task { @MainActor in receive(allowed) } }
    }
    nonisolated private static func resultHandler(_ receive: @escaping @MainActor @Sendable (String?, Bool, Bool) -> Void) -> (SFSpeechRecognitionResult?, Error?) -> Void {
        { result, error in
            let final = result?.isFinal ?? false
            let text = final ? result?.bestTranscription.formattedString : nil
            let failed = error != nil
            Task { @MainActor in receive(text, final, failed) }
        }
    }
    nonisolated private static func interruptionHandler(_ receive: @escaping @MainActor @Sendable () -> Void) -> @Sendable (Notification) -> Void {
        { notification in
            if notification.name == AVAudioSession.interruptionNotification,
               let raw = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
               raw != AVAudioSession.InterruptionType.began.rawValue { return }
            Task { @MainActor in receive() }
        }
    }
    nonisolated private static func routeHandler(_ receive: @escaping @MainActor @Sendable () -> Void) -> @Sendable (Notification) -> Void {
        { notification in
            guard let raw = notification.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
                  raw == AVAudioSession.RouteChangeReason.oldDeviceUnavailable.rawValue else { return }
            Task { @MainActor in receive() }
        }
    }
}
