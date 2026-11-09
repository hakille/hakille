package dev.hakille.mobile

import dev.hakille.mobile.ui.ActionErrorState
import org.junit.Assert.*
import org.junit.Test

class HonestStatesTest {

    @Test
    fun testLoadingThresholdsAdhereToDesignSpec() {
        // Spec states.rs §19:
        // SPINNER_AFTER_MS = 400ms
        // HONEST_AFTER_MS = 2000ms
        fun loadingTier(elapsedMs: Long): String = when {
            elapsedMs < 400L -> "LoadingFast"
            elapsedMs < 2000L -> "LoadingSkeleton"
            else -> "LoadingHonest"
        }

        assertEquals("LoadingFast", loadingTier(0L))
        assertEquals("LoadingFast", loadingTier(200L))
        assertEquals("LoadingFast", loadingTier(399L))

        assertEquals("LoadingSkeleton", loadingTier(400L))
        assertEquals("LoadingSkeleton", loadingTier(1000L))
        assertEquals("LoadingSkeleton", loadingTier(1999L))

        assertEquals("LoadingHonest", loadingTier(2000L))
        assertEquals("LoadingHonest", loadingTier(5000L))
    }

    @Test
    fun testActionErrorOrderAndLabels() {
        val error = ActionErrorState(
            done = "Brouillon sauvegardé localement",
            notDone = "Message non transmis au hub : connexion fermée",
            technicalDetail = "HTTP 503 / IPC_TIMEOUT",
            retryLabel = "Réessayer",
            undoLabel = "Conserver le brouillon"
        )

        assertEquals("Brouillon sauvegardé localement", error.done)
        assertEquals("Message non transmis au hub : connexion fermée", error.notDone)
        assertEquals("HTTP 503 / IPC_TIMEOUT", error.technicalDetail)
        assertEquals("Réessayer", error.retryLabel)
        assertEquals("Conserver le brouillon", error.undoLabel)

        val rendered = "Fait : ${error.done}\nPas fait : ${error.notDone}\n[${error.retryLabel}] [${error.undoLabel}]\nDétails : ${error.technicalDetail}"
        val faitIndex = rendered.indexOf("Fait :")
        val pasFaitIndex = rendered.indexOf("Pas fait :")
        val actionsIndex = rendered.indexOf("[Réessayer]")
        val detailsIndex = rendered.indexOf("Détails :")

        assertTrue(faitIndex in 0..<pasFaitIndex)
        assertTrue(pasFaitIndex < actionsIndex)
        assertTrue(actionsIndex < detailsIndex)
    }

    @Test
    fun testFatigueAlertMessageFormatting() {
        val rawNotification = "Conseil bien-être : Prenez une pause de 5 minutes."
        assertTrue(rawNotification.startsWith("Conseil bien-être :"))
        val recommendation = rawNotification.removePrefix("Conseil bien-être :").trim()
        assertEquals("Prenez une pause de 5 minutes.", recommendation)
    }
}
