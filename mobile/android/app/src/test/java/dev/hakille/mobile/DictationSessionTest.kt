package dev.hakille.mobile

import org.junit.Assert.*
import org.junit.Test

/** Vérifications de contrôle avec callbacks synthétiques ; aucun microphone testé ici. */
class DictationSessionTest {
    private class Engine(val events: DictationEvents) : DictationEngine {
        var starts = 0; var stops = 0; var cancels = 0; var destroys = 0
        var language = ""; var failStart = false; var errorOnCancel = false
        override fun start(language: String) { starts++; this.language = language; if (failStart) throw SecurityException() }
        override fun stop() { stops++ }
        override fun cancel() { cancels++; if (errorOnCancel) events.error(DictationIssue.ENGINE_FAILURE) }
        override fun destroy() { destroys++ }
    }
    private class Platform : DictationPlatform {
        var issue: DictationIssue? = null; var permitted = true; var failCreate = false; var failStart = false
        val engines = mutableListOf<Engine>(); val timers = mutableListOf<() -> Unit>(); var timerMillis = 0L
        override fun unavailable() = issue
        override fun hasPermission() = permitted
        override fun create(events: DictationEvents): DictationEngine {
            val engine = Engine(events).also { it.failStart = failStart }; engines += engine
            if (failCreate) events.error(DictationIssue.ENGINE_FAILURE)
            return engine
        }
        override fun timeout(milliseconds: Long, action: () -> Unit): () -> Unit {
            timerMillis = milliseconds; timers += action
            return { } // Les tests réémettent volontairement les anciens timers.
        }
    }
    private class Harness {
        val platform = Platform()
        var context = DictationContext(1, 0, "", true)
        val commits = mutableListOf<String>()
        val states = mutableListOf<DictationStatus>()
        val session = DictationSession(platform, { context }, { expected, text ->
            if (context != expected) false else { commits += text; true }
        }, states::add)
        val engine get() = platform.engines.last()
    }

    @Test fun unavailableNeverCreatesRecognizerOrRequestsPermission() {
        for (issue in listOf(DictationIssue.ANDROID_VERSION, DictationIssue.ENGINE_MISSING)) {
            val h = Harness(); h.platform.issue = issue; h.platform.permitted = false
            assertNull(h.session.requestStart("fr-FR"))
            assertEquals(issue, h.session.status.issue)
            assertTrue(h.platform.engines.isEmpty())
        }
    }
    @Test fun permissionAndReadyAreSeparateFromAnExplicitStart() {
        val h = Harness(); h.platform.permitted = false
        val token = h.session.requestStart("fr-FR")!!
        assertEquals(DictationPhase.PERMISSION, h.session.status.phase)
        assertTrue(h.platform.engines.isEmpty())
        h.platform.permitted = true; h.session.permissionResult(token, true)
        assertEquals(DictationPhase.STARTING, h.session.status.phase)
        assertEquals("fr-FR", h.engine.language)
        h.engine.events.ready()
        assertEquals(DictationPhase.LISTENING, h.session.status.phase)
        assertEquals(30_000L, h.platform.timerMillis)
    }
    @Test fun deniedAndStalePermissionsNeverStartCapture() {
        val h = Harness(); h.platform.permitted = false
        val denied = h.session.requestStart("en-US")!!
        h.session.permissionResult(denied, false)
        assertEquals(DictationIssue.PERMISSION_DENIED, h.session.status.issue)
        val stale = h.session.requestStart("fr-FR")!!
        h.session.cancel() // Rotation ou passage en arrière-plan.
        h.platform.permitted = true
        h.session.permissionResult(stale, true)
        assertTrue(h.platform.engines.isEmpty())
        assertFalse(h.session.status.active)
    }
    @Test fun finalUnicodeResultAppendsOnlyOnceAndDestroysCapture() {
        val h = Harness(); h.context = h.context.copy(draft = "Mon brouillon :")
        h.session.requestStart("fr-FR"); val engine = h.engine
        engine.errorOnCancel = true
        engine.events.ready(); engine.events.result("  Éléphant 日本語 🙂  ")
        assertEquals(listOf("Mon brouillon : Éléphant 日本語 🙂"), h.commits)
        assertEquals(DictationPhase.FINISHED, h.session.status.phase)
        assertEquals(1, engine.cancels); assertEquals(1, engine.destroys)
        engine.events.result("doublon"); engine.events.error(DictationIssue.AUDIO)
        assertEquals(1, h.commits.size)
        assertEquals(DictationPhase.FINISHED, h.session.status.phase)
    }
    @Test fun staleCallbacksAndTimersCannotAffectANewSession() {
        val h = Harness(); h.session.requestStart("fr-FR"); val old = h.engine
        val oldTimer = h.platform.timers.last()
        h.session.cancel(); h.session.requestStart("en-US"); val current = h.engine
        current.events.ready(); old.events.result("ancien"); old.events.error(DictationIssue.AUDIO); oldTimer()
        assertEquals(DictationPhase.LISTENING, h.session.status.phase)
        assertEquals(0, current.destroys); assertTrue(h.commits.isEmpty())
        current.events.result("nouveau")
        assertEquals(listOf("nouveau"), h.commits)
    }
    @Test fun editingEvenBackToTheSameDraftOrChangingHubInvalidatesResults() {
        for (change in listOf<(DictationContext) -> DictationContext>({ it.copy(revision = 1) }, { it.copy(connection = 2) }, { it.copy(allowed = false) }, { it.copy(draft = "saisie") })) {
            val h = Harness(); h.session.requestStart("fr-FR"); h.context = change(h.context)
            h.engine.events.result("ne pas remplacer")
            assertTrue(h.commits.isEmpty()); assertEquals(1, h.engine.destroys)
            assertEquals(DictationIssue.CONTEXT_CHANGED, h.session.status.issue)
        }
    }
    @Test fun stoppingRequestsRealStopButWaitsForFinalResult() {
        val h = Harness(); h.session.requestStart("fr-FR"); h.engine.events.ready()
        h.session.stop(); assertEquals(1, h.engine.stops)
        assertEquals(DictationPhase.FINISHING, h.session.status.phase)
        h.engine.events.ready(); assertEquals(DictationPhase.FINISHING, h.session.status.phase)
        h.engine.events.speechEnded(); assertEquals(DictationPhase.PROCESSING, h.session.status.phase)
        assertTrue(h.commits.isEmpty())
        h.engine.events.result("terminé"); assertEquals(listOf("terminé"), h.commits)
    }
    @Test fun timeoutAndRevocationStopAndDestroyWithoutDraftMutation() {
        val timed = Harness(); timed.session.requestStart("fr-FR"); timed.platform.timers.last()()
        assertEquals(DictationIssue.TIMEOUT, timed.session.status.issue)
        assertEquals(1, timed.engine.cancels); assertEquals(1, timed.engine.destroys)
        timed.engine.events.result("tardif"); assertTrue(timed.commits.isEmpty())
        val revoked = Harness(); revoked.session.requestStart("fr-FR"); revoked.platform.permitted = false
        revoked.session.checkPermission()
        assertEquals(DictationIssue.PERMISSION_REVOKED, revoked.session.status.issue)
        assertEquals(1, revoked.engine.destroys)
    }
    @Test fun emptyAndOversizeResultsAreNotSilentlyTruncated() {
        for (text in listOf("", "  ", "é".repeat(4097))) {
            val h = Harness(); h.session.requestStart("fr-FR"); h.engine.events.result(text)
            assertTrue(h.commits.isEmpty()); assertEquals(DictationPhase.ERROR, h.session.status.phase)
            assertEquals(1, h.engine.destroys)
        }
    }
    @Test fun failureDuringCreationOrStartCannotLeakARecognizer() {
        val create = Harness(); create.platform.failCreate = true; create.session.requestStart("fr-FR")
        assertEquals(0, create.engine.starts); assertEquals(1, create.engine.destroys)
        val start = Harness(); start.platform.failStart = true; start.session.requestStart("fr-FR")
        assertEquals(DictationIssue.PERMISSION_REVOKED, start.session.status.issue)
        assertEquals(1, start.engine.cancels); assertEquals(1, start.engine.destroys)
    }
}
