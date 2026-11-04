import Foundation

enum CardPresentation {
    static func money(_ value: JSONValue, locale: Locale = .current) -> String {
        guard let micros = Decimal(string: value["micros"].text, locale: Locale(identifier: "en_US_POSIX")) else { return "?" }
        let formatter = NumberFormatter(); formatter.locale = locale; formatter.numberStyle = .decimal
        formatter.minimumFractionDigits = 2; formatter.maximumFractionDigits = 6
        let amount = formatter.string(from: NSDecimalNumber(decimal: micros / 1_000_000)) ?? "?"
        return "\(amount) \(value["currency"].text)"
    }
    static func destination(_ value: JSONValue) -> String {
        if value == .string("Local") { return String(localized: "Remains on your hub") }
        let provider = value["Cloud"]["provider"].text
        if !provider.isEmpty { return String(localized: "Cloud provider") + ": " + provider }
        return String(localized: "Unrecognized metadata — check before approving")
    }
    static func provenance(_ value: JSONValue) -> String {
        if value == .string("Hakille") { return "Hakille" }
        if value == .string("External") { return String(localized: "External origin") }
        let skill = value["VerifiedSkill"]
        if !skill.isNull { return String(localized: "Verified skill") + ": " + skill["publisher"].text + " / " + skill["name"].text }
        return String(localized: "Unrecognized metadata — check before approving")
    }
}
