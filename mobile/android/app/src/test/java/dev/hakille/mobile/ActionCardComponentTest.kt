package dev.hakille.mobile

import dev.hakille.mobile.ui.ActionCardHelper
import dev.hakille.mobile.ui.CardReversibility
import dev.hakille.mobile.ui.ProvenanceBand
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.*
import org.junit.Test

class ActionCardComponentTest {

    @Test
    fun testThreeBadgesOrderAndFormatting() {
        val cardJson = """
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
        """.trimIndent()
        val card = Json.parseToJsonElement(cardJson).jsonObject
        val (cost, data, rev) = ActionCardHelper.pastilles(card)

        // Badge 1: Cost
        assertTrue("Pastille 1 must contain formatted cost", cost.contains("0.00") && cost.contains("EUR"))
        // Badge 2: Data Destination
        assertEquals("Local", data)
        // Badge 3: Reversibility
        assertEquals("− annulable", rev)
    }

    @Test
    fun testReversibilityLabelsNeverColorAlone() {
        val reversibleCard = Json.parseToJsonElement("""{"reversible": true}""").jsonObject
        val finalCard = Json.parseToJsonElement("""{"reversible": false}""").jsonObject
        val unspecifiedCard = Json.parseToJsonElement("""{}""").jsonObject

        assertEquals("− annulable", ActionCardHelper.reversibility(reversibleCard).label)
        assertEquals("+ définitif", ActionCardHelper.reversibility(finalCard).label)
        assertEquals("+ définitif", ActionCardHelper.reversibility(unspecifiedCard).label)
    }

    @Test
    fun testSingleDecisionOpenConstraint() {
        val card1 = Json.parseToJsonElement("""{"id": 1, "verb": "A"}""").jsonObject
        val card2 = Json.parseToJsonElement("""{"id": 2, "verb": "B"}""").jsonObject
        val card3 = Json.parseToJsonElement("""{"id": 3, "verb": "C"}""").jsonObject

        val allCards = listOf(card1, card2, card3)
        val filtered = ActionCardHelper.filterSingleDecision(allCards)

        assertEquals("Single decision constraint must return at most 1 card", 1, filtered.size)
        assertEquals(card1, filtered[0])

        val emptyFiltered = ActionCardHelper.filterSingleDecision(emptyList())
        assertTrue(emptyFiltered.isEmpty())
    }

    @Test
    fun testProvenanceBandExtraction() {
        val hakilleCard = Json.parseToJsonElement("""{"provenance": "Hakille"}""").jsonObject
        val externalCard = Json.parseToJsonElement("""{"provenance": "External"}""").jsonObject
        val skillCard = Json.parseToJsonElement("""{"provenance": {"VerifiedSkill": {"publisher": "Acme", "name": "Weather"}}}""").jsonObject

        assertTrue(ActionCardHelper.provenanceBand(hakilleCard) is ProvenanceBand.Hakille)
        assertTrue(ActionCardHelper.provenanceBand(externalCard) is ProvenanceBand.External)
        val skillBand = ActionCardHelper.provenanceBand(skillCard)
        assertTrue(skillBand is ProvenanceBand.VerifiedSkill)
        assertEquals("skill vérifié (Acme · Weather)", (skillBand as ProvenanceBand.VerifiedSkill).label())
    }
}
