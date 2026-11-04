import Foundation

/// Politique pure : aucun accès micro, aucune requête réseau, aucun envoi métier.
enum DictationPhase: Equatable {
    case idle, authorizing, waitingForForeground, preparing, listening, finishing, saving, completed, cancelled, failed
    var isBusy: Bool {
        switch self {
        case .authorizing, .waitingForForeground, .preparing, .listening, .finishing, .saving: return true
        default: return false
        }
    }
}

struct DictationStatus: Equatable {
    var phase: DictationPhase = .idle
    var message = ""
    var isBusy: Bool { phase.isBusy }
}

enum DictationFailure: Error, Equatable, LocalizedError {
    case inactive, busy, unsupportedLanguage, localRecognitionUnavailable
    case speechDenied, speechRestricted, microphoneDenied, audioUnavailable
    case recognitionFailed, interrupted, noSpeech, finalizationTimeout
    case draftChanged, draftTooLong, storageUnavailable, audioSessionFailure

    var errorDescription: String? {
        switch self {
        case .inactive: return String(localized: "Open the app before starting dictation.")
        case .busy: return String(localized: "Finish the current operation before dictating.")
        case .unsupportedLanguage: return String(localized: "Speech recognition does not support the selected language on this device.")
        case .localRecognitionUnavailable: return String(localized: "On-device speech recognition is unavailable for this language. No network recognition will be used.")
        case .speechDenied: return String(localized: "Speech permission was denied. Enable it in iOS Settings to dictate.")
        case .speechRestricted: return String(localized: "Speech recognition is restricted on this device.")
        case .microphoneDenied: return String(localized: "Microphone permission was denied. Enable it in iOS Settings to dictate.")
        case .audioUnavailable: return String(localized: "The microphone could not provide audio. Check the input device and try again.")
        case .recognitionFailed: return String(localized: "On-device recognition failed. No text was submitted.")
        case .interrupted: return String(localized: "Dictation was interrupted. Its unfinished text was discarded.")
        case .noSpeech: return String(localized: "No speech was recognized. The draft was not changed.")
        case .finalizationTimeout: return String(localized: "Recognition did not finish in time. The draft was not changed.")
        case .draftChanged: return String(localized: "The draft changed during dictation. Its contents were preserved.")
        case .draftTooLong: return String(localized: "The dictated text exceeds the draft limit. The draft was not changed.")
        case .storageUnavailable: return String(localized: "The dictated draft could not be saved. No message was sent.")
        case .audioSessionFailure: return String(localized: "The microphone was stopped, but the audio session could not be restored.")
        }
    }
}

struct DictationPolicy {
    static let maximumCaptureSeconds: UInt64 = 30
    static let finalizationSeconds: UInt64 = 8
    static let maximumDraftBytes = 8192
    private(set) var phase: DictationPhase = .idle
    private(set) var generation: UUID?

    mutating func begin(foreground: Bool) throws -> UUID {
        guard foreground else { throw DictationFailure.inactive }
        guard !phase.isBusy else { throw DictationFailure.busy }
        let token = UUID()
        generation = token; phase = .authorizing
        return token
    }
    func accepts(_ token: UUID) -> Bool { generation == token && phase.isBusy }
    mutating func transition(_ next: DictationPhase, token: UUID) -> Bool {
        guard accepts(token) else { return false }
        phase = next; return true
    }
    mutating func finish(token: UUID) -> Bool {
        guard accepts(token), phase == .listening || phase == .finishing else { return false }
        generation = nil; phase = .completed; return true
    }
    mutating func cancel(failed: Bool = false) {
        generation = nil; phase = failed ? .failed : .cancelled
    }
    /// L'inactivité des dialogues de permission n'autorise jamais la capture.
    /// Un vrai passage en arrière-plan invalide également leurs callbacks.
    mutating func leaveForeground(background: Bool) -> Bool {
        guard phase.isBusy else { return false }
        if !background && (phase == .authorizing || phase == .waitingForForeground) { return false }
        cancel(); return true
    }
    static func requireLocalSupport(supported: Bool, available: Bool) throws {
        guard supported && available else { throw DictationFailure.localRecognitionUnavailable }
    }
    static func completedDraft(original: String, current: String, transcript: String) throws -> String {
        guard current == original else { throw DictationFailure.draftChanged }
        let text = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw DictationFailure.noSpeech }
        let separator = original.isEmpty || original.last?.isWhitespace == true ? "" : "\n"
        let combined = original + separator + text
        guard !combined.contains("\0"), combined.utf8.count <= maximumDraftBytes else { throw DictationFailure.draftTooLong }
        return combined
    }
}
