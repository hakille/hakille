package dev.hakille.mobile

import java.io.IOException
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test

/** Teste la barrière réellement utilisée par le transport, sans simuler une preuve disque. */
class DurableDraftTest {
    @Test fun requestWaitsForSuccessfulPersistence() = runBlocking {
        val saved = CompletableDeferred<Unit>()
        val events = mutableListOf<String>()
        val request = async(start = CoroutineStart.UNDISPATCHED) {
            afterDurableDraft(persist = { events += "écriture"; saved.await(); events += "durable" }) {
                events += "envoi"; "réponse"
            }
        }
        assertEquals(listOf("écriture"), events)
        saved.complete(Unit)
        assertEquals("réponse", request.await())
        assertEquals(listOf("écriture", "durable", "envoi"), events)
    }

    @Test fun storageFailureNeverCallsTransport() = runBlocking {
        var sent = false
        try {
            afterDurableDraft(persist = { throw IOException("disque plein") }) { sent = true }
            fail("L’échec doit être propagé")
        } catch (_: IOException) { }
        assertFalse(sent)
    }

    @Test fun cancellationWhileWritingNeverCallsTransport() = runBlocking {
        var sent = false
        val started = CompletableDeferred<Unit>()
        val writing = CompletableDeferred<Unit>()
        val request = launch(start = CoroutineStart.UNDISPATCHED) {
            afterDurableDraft(persist = { started.complete(Unit); writing.await() }) { sent = true }
        }
        started.await(); request.cancelAndJoin(); writing.complete(Unit)
        assertFalse(sent)
    }

    @Test fun cancellationImmediatelyAfterDurableWriteStillPreventsSending() = runBlocking {
        var sent = false
        val request = launch(start = CoroutineStart.UNDISPATCHED) {
            afterDurableDraft(persist = { currentCoroutineContext().cancel() }) { sent = true }
        }
        request.join()
        assertFalse(sent)
    }
}
