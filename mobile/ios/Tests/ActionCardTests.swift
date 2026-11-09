import XCTest
@testable import Hakille

final class ActionCardTests: XCTestCase {

    func testThreeBadgesOrderAndFormatting() throws {
        let jsonStr = """
        {
            "id": 101,
            "verb": "Transférer",
            "object": "document.pdf",
            "reason": "Demande utilisateur",
            "cost": {"micros": 0, "currency": "EUR"},
            "data_exit": "Local",
            "reversible": true,
            "provenance": "Hakille"
        }
        """
        let card = try JSONValue.parse(jsonStr)
        let (cost, data, rev) = ActionCardHelper.pastilles(for: card)

        // Badge 1: Cost
        XCTAssertTrue(cost.contains("0.00") && cost.contains("EUR"), "Pastille 1 must contain formatted cost: \(cost)")
        // Badge 2: Data Destination
        XCTAssertTrue(data.contains("hub") || data.contains("Local"), "Pastille 2 must contain data destination: \(data)")
        // Badge 3: Reversibility
        XCTAssertEqual(rev, "− annulable", "Pastille 3 must be − annulable")
    }

    func testReversibilityNeverColorAlone() throws {
        let reversibleCard = try JSONValue.parse("{\"reversible\": true}")
        let finalCard = try JSONValue.parse("{\"reversible\": false}")
        let emptyCard = try JSONValue.parse("{}")

        XCTAssertEqual(ActionCardHelper.reversibility(for: reversibleCard).rawValue, "− annulable")
        XCTAssertEqual(ActionCardHelper.reversibility(for: finalCard).rawValue, "+ définitif")
        XCTAssertEqual(ActionCardHelper.reversibility(for: emptyCard).rawValue, "+ définitif")
    }

    func testSingleDecisionOpenConstraint() throws {
        let card1 = try JSONValue.parse("{\"id\": 1, \"verb\": \"A\"}")
        let card2 = try JSONValue.parse("{\"id\": 2, \"verb\": \"B\"}")
        let card3 = try JSONValue.parse("{\"id\": 3, \"verb\": \"C\"}")

        let allCards = [card1, card2, card3]
        let filtered = ActionCardHelper.filterSingleDecision(allCards)

        XCTAssertEqual(filtered.count, 1, "Single decision constraint must return at most 1 card")
        XCTAssertEqual(filtered.first?["id"].int, 1)

        let emptyFiltered = ActionCardHelper.filterSingleDecision([])
        XCTAssertTrue(emptyFiltered.isEmpty)
    }

    func testProvenanceBandExtraction() throws {
        let hakilleCard = try JSONValue.parse("{\"provenance\": \"Hakille\"}")
        let externalCard = try JSONValue.parse("{\"provenance\": \"External\"}")
        let skillCard = try JSONValue.parse("{\"provenance\": {\"VerifiedSkill\": {\"publisher\": \"Acme\", \"name\": \"Weather\"}}}")

        XCTAssertEqual(ActionCardHelper.provenanceBand(for: hakilleCard), .hakille)
        XCTAssertEqual(ActionCardHelper.provenanceBand(for: externalCard), .external)
        let skillBand = ActionCardHelper.provenanceBand(for: skillCard)
        XCTAssertEqual(skillBand, .verifiedSkill(publisher: "Acme", name: "Weather"))
        XCTAssertEqual(skillBand.label, "skill vérifié (Acme · Weather)")
    }
}
