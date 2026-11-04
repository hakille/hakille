package dev.hakille.mobile

import org.junit.Assert.*
import org.junit.Test

/** Politiques pures : ces tests ne revendiquent pas de sortie audio matérielle. */
class VoiceTurnPolicyTest {
    @Test fun optInIsRequiredAndRepliesAreConsumedExactlyOnce() {
        val policy = VoiceTurnPolicy()
        assertFalse(policy.awaitReply(1uL))
        assertNull(policy.finish(1uL, true, "Bonjour"))
        policy.enable(true)
        assertTrue(policy.awaitReply(1uL))
        assertFalse(policy.awaitReply(2uL))
        assertNull(policy.finish(2uL, true, "Autre réponse"))
        assertEquals("Bonjour", policy.finish(1uL, true, "Bonjour"))
        assertNull(policy.finish(1uL, true, "Doublon"))
    }
    @Test fun cancelledAndFailedRepliesCannotBeSpokenOrReplayed() {
        val policy = VoiceTurnPolicy(); policy.enable(true)
        assertTrue(policy.awaitReply(7uL)); policy.cancel()
        assertNull(policy.finish(7uL, true, "Ancienne session"))
        assertTrue(policy.awaitReply(8uL))
        assertNull(policy.finish(8uL, false, "Échec"))
        assertNull(policy.finish(8uL, true, "Tardif"))
        assertTrue(policy.awaitReply(9uL)); policy.enable(false)
        assertNull(policy.finish(9uL, true, "Arrière-plan"))
        policy.enable(true)
        assertNull(policy.finish(9uL, true, "Ancien mode"))
    }
    @Test fun finalBlankTextNeverProducesSpeech() {
        val policy = VoiceTurnPolicy(); policy.enable(true)
        policy.awaitReply(3uL)
        assertNull(policy.finish(3uL, true, " \n\t"))
        assertTrue(policy.awaitReply(4uL))
        assertEquals("日本語", policy.finish(4uL, true, "日本語"))
    }
    @Test fun networkAndNotInstalledVoicesAreRejectedWithoutFallback() {
        val voices = listOf(
            LocalVoice("network-fr", "fr", "FR", true, true),
            LocalVoice("missing-fr", "fr", "FR", false, false),
            LocalVoice("offline-en", "en", "US", false, true),
        )
        assertNull(chooseLocalVoice(voices, "fr", "FR"))
        assertEquals("offline-en", chooseLocalVoice(voices, "en", "US"))
        assertNull(chooseLocalVoice(emptyList(), "en", "US"))
    }
    @Test fun installedOfflineVoiceUsesRequestedLanguageAndPrefersRegion() {
        val voices = listOf(LocalVoice("ca-fr", "fr", "CA", false, true),
            LocalVoice("fr-fr", "fr", "FR", false, true), LocalVoice("en-us", "en", "US", false, true))
        assertEquals("fr-fr", chooseLocalVoice(voices, "fr", "FR"))
        assertEquals("ca-fr", chooseLocalVoice(voices, "fr", "CA"))
        assertEquals("ca-fr", chooseLocalVoice(voices, "fr", "BE"))
        assertNull(chooseLocalVoice(voices, "de", "DE"))
    }
}
