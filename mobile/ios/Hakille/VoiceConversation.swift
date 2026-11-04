import AVFoundation
import Foundation
import UIKit

struct VoiceReplyIdentity: Equatable, Sendable {
    let context: UInt64
    let request: UInt64
}

/// Ne lit jamais « le dernier message » : seule la réponse dédiée, corrélée
/// au tour vocal courant, est éligible à la synthèse système.
struct VoiceConversationPolicy {
    private(set) var enabled = false
    private(set) var generation = UUID()
    private(set) var context: UInt64?
    private(set) var consumedReply: VoiceReplyIdentity?
    mutating func enable() { enabled = true; generation = UUID(); context = nil; consumedReply = nil }
    mutating func disable() { enabled = false; generation = UUID(); context = nil; consumedReply = nil }
    mutating func clearTurn() { context = nil; consumedReply = nil }
    mutating func arm(context: UInt64) { self.context = context; consumedReply = nil }
    func accepts(context: UInt64, generation: UUID) -> Bool { enabled && self.generation == generation && self.context == context }
    mutating func consume(_ identity: VoiceReplyIdentity, foreground: Bool, context: UInt64?, request: UInt64?) -> Bool {
        guard enabled, foreground, self.context == identity.context, context == identity.context,
              request == identity.request, consumedReply != identity else { return false }
        consumedReply = identity; return true
    }
    static func validatedTranscript(_ text: String) throws -> String {
        try DictationPolicy.completedDraft(original: "", current: "", transcript: text)
    }
}

enum SpeechOutputEvent: Sendable { case started, finished, cancelled }

private final class SpeechOutputDelegate: NSObject, AVSpeechSynthesizerDelegate {
    let utterance: AVSpeechUtterance
    let token: UUID
    let receive: @MainActor @Sendable (UUID, SpeechOutputEvent) -> Void
    init(utterance: AVSpeechUtterance, token: UUID, receive: @escaping @MainActor @Sendable (UUID, SpeechOutputEvent) -> Void) {
        self.utterance = utterance; self.token = token; self.receive = receive
    }
    private func forward(_ utterance: AVSpeechUtterance, _ event: SpeechOutputEvent) {
        guard utterance === self.utterance else { return }
        let token = token; let receive = receive
        Task { @MainActor in receive(token, event) }
    }
    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didStart utterance: AVSpeechUtterance) { forward(utterance, .started) }
    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) { forward(utterance, .finished) }
    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) { forward(utterance, .cancelled) }
}

private final class SpeechOutputResources {
    let synthesizer = AVSpeechSynthesizer()
    var delegate: SpeechOutputDelegate?
    var sessionActive = false
    @discardableResult func stop() -> Bool {
        synthesizer.stopSpeaking(at: .immediate); synthesizer.delegate = nil; delegate = nil
        if sessionActive {
            do {
                try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
                sessionActive = false
            } catch { return false }
        }
        return true
    }
    deinit { stop() }
}
private final class SpeechOutputObservers {
    var token: NSObjectProtocol?
    deinit { if let token { NotificationCenter.default.removeObserver(token) } }
}

@MainActor final class NativeSpeechOutput {
    private var resources: SpeechOutputResources?
    private var token: UUID?
    private var timeout: Task<Void, Never>?
    private var startupTimeout: Task<Void, Never>?
    private var started = false
    private let observers = SpeechOutputObservers()
    private let onChange: (Bool, String) -> Void
    init(onChange: @escaping (Bool, String) -> Void) {
        self.onChange = onChange
        observers.token = NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: nil,
            using: Self.interruptionHandler { [weak self] in
                guard self?.token != nil else { return }
                self?.stop(message: String(localized: "Spoken reply interrupted. Read the reply on screen."))
            })
    }
    func speak(_ text: String, localeIdentifier: String) {
        stop()
        guard UIApplication.shared.applicationState == .active else { return }
        guard !text.isEmpty, text.utf8.count <= 8192 else {
            onChange(false, String(localized: "This reply cannot be read aloud. Read it on screen.")); return
        }
        let language = localeIdentifier.replacingOccurrences(of: "_", with: "-")
        let voices = AVSpeechSynthesisVoice.speechVoices()
        guard let voice = voices.first(where: { $0.language.caseInsensitiveCompare(language) == .orderedSame })
            ?? voices.first(where: { $0.language.split(separator: "-").first == language.split(separator: "-").first }) else {
            onChange(false, String(localized: "No installed system voice supports this language.")); return
        }
        do {
            let output = SpeechOutputResources(); resources = output
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .spokenAudio, options: .duckOthers)
            try AVAudioSession.sharedInstance().setActive(true); output.sessionActive = true
            let current = UUID(); token = current; started = false
            let utterance = AVSpeechUtterance(string: text); utterance.voice = voice
            let callbacks = SpeechOutputDelegate(utterance: utterance, token: current) { [weak self] token, event in self?.receive(token, event) }
            output.delegate = callbacks; output.synthesizer.delegate = callbacks
            onChange(false, String(localized: "Preparing the installed system voice…"))
            output.synthesizer.speak(utterance)
            startupTimeout = Task { [weak self] in
                do { try await Task.sleep(nanoseconds: 8_000_000_000) } catch { return }
                guard let self, self.token == current, !self.started else { return }
                self.stop(message: String(localized: "System speech output is unavailable. Read the reply on screen."))
            }
            timeout = Task { [weak self] in
                do { try await Task.sleep(nanoseconds: 120_000_000_000) } catch { return }
                guard let self, self.token == current else { return }
                self.stop(message: String(localized: "Spoken reply stopped after two minutes. Read the rest on screen."))
            }
        } catch { stop(message: String(localized: "System speech output is unavailable. Read the reply on screen.")) }
    }
    func stop(message: String = "") {
        token = nil; timeout?.cancel(); timeout = nil; startupTimeout?.cancel(); startupTimeout = nil; started = false
        let restored = resources?.stop() ?? true; resources = nil
        onChange(false, restored ? message : String(localized: "Speech stopped, but the audio session could not be restored."))
    }
    private func receive(_ current: UUID, _ event: SpeechOutputEvent) {
        guard token == current else { return }
        switch event {
        case .started:
            started = true; startupTimeout?.cancel(); startupTimeout = nil
            onChange(true, String(localized: "Reading the hub reply with the installed system voice."))
        case .finished: stop(message: String(localized: "Your turn. Tap Speak when you are ready."))
        case .cancelled: stop(message: String(localized: "Spoken reply interrupted. Read the reply on screen."))
        }
    }
    nonisolated private static func interruptionHandler(_ receive: @escaping @MainActor @Sendable () -> Void) -> @Sendable (Notification) -> Void {
        { notification in
            guard let raw = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                  raw == AVAudioSession.InterruptionType.began.rawValue else { return }
            Task { @MainActor in receive() }
        }
    }
}
