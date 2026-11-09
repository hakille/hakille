import XCTest
@testable import Hakille

final class ProvenanceSanitizationTests: XCTestCase {

    func testHtmlMarkupStripped() {
        let evil = "<script>stealKeys(); alert(1);</script><style>body { color:red; }</style><b style=\"color:red\">Texte important</b>\u{001B}[31m"
        let sanitized = sanitizeSkillText(evil)
        XCTAssertFalse(sanitized.contains("<"))
        XCTAssertFalse(sanitized.contains(">"))
        XCTAssertFalse(sanitized.contains("stealKeys"))
        XCTAssertFalse(sanitized.contains("color:red"))
        XCTAssertFalse(sanitized.contains("[31m"))
        XCTAssertEqual(sanitized, "Texte important")
    }

    func testAnsiEscapeSequencesStripped() {
        let ansiInput = "\u{001B}[31mTexte en rouge\u{001B}[0m"
        let sanitized = sanitizeSkillText(ansiInput)
        XCTAssertFalse(sanitized.contains("\u{001B}"))
        XCTAssertEqual(sanitized, "Texte en rouge")
    }

    func testExternalBlockSideRuleIs2pt() {
        XCTAssertEqual(ExternalBlockConstants.sideRulePt, 2.0)
        XCTAssertEqual(ExternalBlockConstants.badge, "externe")
    }

    func testSquattingDetection() {
        let installed = ["mail", "calendar", "notes"]
        let fake = ProvenanceBand.verifiedSkill(publisher: "Acme", name: "mail-plus")
        let legit = ProvenanceBand.verifiedSkill(publisher: "Acme", name: "mail")
        let other = ProvenanceBand.verifiedSkill(publisher: "Acme", name: "weather")

        XCTAssertTrue(fake.looksLikeInstalled(installed))
        XCTAssertFalse(legit.looksLikeInstalled(installed))
        XCTAssertFalse(other.looksLikeInstalled(installed))
    }

    func testClipboardTtlIs60Seconds() {
        XCTAssertEqual(CLIPBOARD_SENSITIVE_TTL_SECS, 60.0)
    }
}
