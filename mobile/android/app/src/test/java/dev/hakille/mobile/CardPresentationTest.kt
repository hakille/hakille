package dev.hakille.mobile
import java.util.Locale
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
class CardPresentationTest {
    @Test fun microUnitsAndDecisionMetadataStayVisible() {
        assertEquals("0.000001 EUR", CardPresentation.money(Json.parseToJsonElement("{\"micros\":1,\"currency\":\"EUR\"}"), Locale.US))
        assertEquals("1.25 EUR", CardPresentation.money(Json.parseToJsonElement("{\"micros\":1250000,\"currency\":\"EUR\"}"), Locale.US))
        assertEquals("local", CardPresentation.destination(JsonPrimitive("Local"), "local", "cloud", "unknown"))
        assertEquals("cloud : provider", CardPresentation.destination(Json.parseToJsonElement("{\"Cloud\":{\"provider\":\"provider\"}}"), "local", "cloud", "unknown"))
        assertEquals("verified : publisher / skill", CardPresentation.provenance(Json.parseToJsonElement("{\"VerifiedSkill\":{\"publisher\":\"publisher\",\"name\":\"skill\"}}"), "external", "verified", "unknown"))
        assertEquals("unknown", CardPresentation.destination(JsonPrimitive("missing"), "local", "cloud", "unknown"))
    }
}
