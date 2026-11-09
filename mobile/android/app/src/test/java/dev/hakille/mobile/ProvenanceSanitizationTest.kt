package dev.hakille.mobile

import dev.hakille.mobile.ui.CLIPBOARD_SENSITIVE_TTL_SECS
import dev.hakille.mobile.ui.ExternalBlockConstants
import dev.hakille.mobile.ui.looksLikeInstalled
import dev.hakille.mobile.ui.sanitizeSkillText
import org.junit.Assert.*
import org.junit.Test

class ProvenanceSanitizationTest {

    @Test
    fun testHtmlMarkupStripped() {
        val evil = "<script>stealKeys(); alert(1);</script><style>body { color:red; }</style><b style=\"color:red\">Texte important</b>\u001B[31m"
        val sanitized = sanitizeSkillText(evil)
        assertFalse("Must not contain '<'", sanitized.contains("<"))
        assertFalse("Must not contain '>'", sanitized.contains(">"))
        assertFalse("Must not contain stealKeys", sanitized.contains("stealKeys"))
        assertFalse("Must not contain color:red", sanitized.contains("color:red"))
        assertFalse("Must not contain [31m", sanitized.contains("[31m"))
        assertEquals("Texte important", sanitized)
    }

    @Test
    fun testAnsiEscapeSequencesStripped() {
        val ansiInput = "\u001B[31mTexte en rouge\u001B[0m"
        val sanitized = sanitizeSkillText(ansiInput)
        assertFalse("Must not contain escape byte", sanitized.contains("\u001B"))
        assertEquals("Texte en rouge", sanitized)
    }

    @Test
    fun testCombinedHtmlAndAnsiStripped() {
        val combined = "<p>Message: \u001B[32mValidé\u001B[0m <span class=\"bad\">fin</span></p>"
        val sanitized = sanitizeSkillText(combined)
        assertEquals("Message: Validé fin", sanitized)
    }

    @Test
    fun testExternalBlockSideRuleIs2px() {
        assertEquals("External lateral border must be strictly 2px per spec §17", 2, ExternalBlockConstants.SIDE_RULE_PX)
        assertEquals("externe", ExternalBlockConstants.BADGE)
    }

    @Test
    fun testSquattingDetection() {
        val installed = listOf("mail", "calendar", "notes")

        // Lookalikes / substring variants must be flagged
        assertTrue("mail-plus must trigger squatting warning", looksLikeInstalled("mail-plus", installed))
        assertTrue("mycalendar must trigger squatting warning", looksLikeInstalled("mycalendar", installed))

        // Exact match (legitimate skill) should not trigger the squatting warning
        assertFalse("Exact match mail must not trigger warning", looksLikeInstalled("mail", installed))
        // Completely distinct skill should not trigger
        assertFalse("Distinct weather must not trigger warning", looksLikeInstalled("weather", installed))
    }

    @Test
    fun testClipboardTtlIs60Seconds() {
        assertEquals("Sensitive clipboard TTL must be 60 seconds", 60L, CLIPBOARD_SENSITIVE_TTL_SECS)
    }
}
