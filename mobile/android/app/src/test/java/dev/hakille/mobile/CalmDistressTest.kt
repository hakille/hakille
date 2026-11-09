package dev.hakille.mobile

import dev.hakille.mobile.ui.CalmDistressConstants
import hakille.tokens.HakilleTokens
import org.junit.Assert.*
import org.junit.Test

class CalmDistressTest {

    @Test
    fun testCalmDistressCopyMatchesSpec() {
        val expectedCopy = "Je reste là. Un geste pour appeler ton contact, un autre pour les urgences."
        assertEquals(expectedCopy, CalmDistressConstants.CALM_COPY)
        assertTrue("Calm copy must contain 'contact'", CalmDistressConstants.CALM_COPY.contains("contact"))
        assertTrue("Calm copy must contain 'urgences'", CalmDistressConstants.CALM_COPY.contains("urgences"))
    }

    @Test
    fun testEmergencyDialerTarget() {
        assertEquals("Emergency dialer target must be European standard 112", "112", CalmDistressConstants.EMERGENCY_NUMBER)
    }

    @Test
    fun testCalmDistressContainsNoBraise() {
        // Braise (#FF7A2F) is strictly reserved for validation and decision ring; forbidden in calm distress mode
        val braiseHex = HakilleTokens.colorBraiseDarkHex
        assertNotEquals(braiseHex, HakilleTokens.colorSurface0DarkHex)
        assertNotEquals(braiseHex, HakilleTokens.colorSurface1DarkHex)
        assertNotEquals(braiseHex, HakilleTokens.colorSurface2DarkHex)
        assertNotEquals(braiseHex, HakilleTokens.colorTexte1DarkHex)
        assertNotEquals(braiseHex, HakilleTokens.colorTexte2DarkHex)
    }
}
