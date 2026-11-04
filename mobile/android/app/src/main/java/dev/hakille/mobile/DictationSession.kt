package dev.hakille.mobile

/** État natif de la dictée, distinct du microphone du cœur Rust. */
enum class DictationPhase { IDLE, PERMISSION, STARTING, LISTENING, FINISHING, PROCESSING, FINISHED, CANCELLED, ERROR }
enum class DictationIssue { ANDROID_VERSION, ENGINE_MISSING, PERMISSION_DENIED, PERMISSION_REVOKED, BUSY, AUDIO, LANGUAGE, NO_MATCH, TIMEOUT, CONTEXT_CHANGED, RESULT_INVALID, ENGINE_FAILURE }
data class DictationStatus(val phase: DictationPhase = DictationPhase.IDLE, val issue: DictationIssue? = null) {
    val active: Boolean get() = phase in setOf(DictationPhase.PERMISSION, DictationPhase.STARTING, DictationPhase.LISTENING, DictationPhase.FINISHING, DictationPhase.PROCESSING)
}
data class DictationContext(val connection: Long, val revision: Long, val draft: String, val allowed: Boolean)

interface DictationEngine {
    fun start(language: String)
    fun stop()
    fun cancel()
    fun destroy()
}
interface DictationEvents {
    fun ready()
    fun speechEnded()
    fun result(text: String?)
    fun error(issue: DictationIssue)
}
interface DictationPlatform {
    fun unavailable(): DictationIssue?
    fun hasPermission(): Boolean
    fun create(events: DictationEvents): DictationEngine
    fun timeout(milliseconds: Long, action: () -> Unit): () -> Unit
}

/** Toutes les transitions et tous les appels hôtes ont lieu sur le thread UI.
 * Les tests injectent un moteur factice pour vérifier les décisions, jamais
 * pour revendiquer une transcription ou un accès matériel réellement testé.
 */
class DictationSession(
    private val platform: DictationPlatform,
    private val context: () -> DictationContext,
    private val commitDraft: (DictationContext, String) -> Boolean,
    private val changed: (DictationStatus) -> Unit,
) {
    var status = DictationStatus(); private set
    private var generation = 0L
    private var engine: DictationEngine? = null
    private var cancelTimeout: (() -> Unit)? = null
    private var captured: DictationContext? = null
    private var language = ""

    fun unavailable(): DictationIssue? = platform.unavailable()

    /** Retourne uniquement le ticket de la demande de permission à afficher. */
    fun requestStart(languageTag: String): Long? {
        cancel()
        val issue = platform.unavailable()
        if (issue != null) { update(DictationStatus(DictationPhase.ERROR, issue)); return null }
        val current = context()
        if (!current.allowed) { update(DictationStatus(DictationPhase.ERROR, DictationIssue.CONTEXT_CHANGED)); return null }
        generation++; captured = current; language = languageTag
        if (!platform.hasPermission()) {
            update(DictationStatus(DictationPhase.PERMISSION))
            return generation
        }
        startEngine(generation)
        return null
    }

    fun permissionResult(ticket: Long, granted: Boolean) {
        if (ticket != generation || status.phase != DictationPhase.PERMISSION) return
        if (!granted || !platform.hasPermission()) { fail(DictationIssue.PERMISSION_DENIED); return }
        if (!validContext()) { fail(DictationIssue.CONTEXT_CHANGED); return }
        startEngine(ticket)
    }

    fun stop() {
        if (status.phase !in setOf(DictationPhase.STARTING, DictationPhase.LISTENING)) return
        update(DictationStatus(DictationPhase.FINISHING))
        try { engine?.stop() } catch (_: SecurityException) { fail(DictationIssue.PERMISSION_REVOKED) }
        catch (_: Exception) { fail(DictationIssue.ENGINE_FAILURE) }
    }

    fun cancel() {
        val active = status.active
        cleanup()
        if (active) update(DictationStatus(DictationPhase.CANCELLED))
    }

    fun checkPermission() {
        if (status.active && status.phase != DictationPhase.PERMISSION && !platform.hasPermission()) fail(DictationIssue.PERMISSION_REVOKED)
    }

    private fun validContext(): Boolean {
        val expected = captured ?: return false
        val current = context()
        return current.allowed && current == expected
    }

    private fun accepts(ticket: Long): Boolean {
        if (ticket != generation || !status.active) return false
        if (!platform.hasPermission()) { fail(DictationIssue.PERMISSION_REVOKED); return false }
        if (!validContext()) { fail(DictationIssue.CONTEXT_CHANGED); return false }
        return true
    }

    private fun startEngine(ticket: Long) {
        update(DictationStatus(DictationPhase.STARTING))
        try {
            val created = platform.create(object : DictationEvents {
                override fun ready() {
                    if (accepts(ticket) && status.phase == DictationPhase.STARTING) update(DictationStatus(DictationPhase.LISTENING))
                }
                override fun speechEnded() {
                    if (accepts(ticket)) update(DictationStatus(DictationPhase.PROCESSING))
                }
                override fun error(issue: DictationIssue) { if (accepts(ticket)) fail(issue) }
                override fun result(text: String?) {
                    if (!accepts(ticket)) return
                    val expected = captured ?: return
                    val result = text?.trim().orEmpty()
                    if (result.isEmpty() || result.any { it.isISOControl() && it != '\n' && it != '\t' }) { fail(DictationIssue.NO_MATCH); return }
                    val separator = if (expected.draft.isEmpty() || expected.draft.last().isWhitespace()) "" else " "
                    val draft = expected.draft + separator + result
                    if (draft.toByteArray(Charsets.UTF_8).size > MAX_DRAFT_BYTES) { fail(DictationIssue.RESULT_INVALID); return }
                    cleanup()
                    val accepted = try { commitDraft(expected, draft) } catch (_: Exception) { false }
                    update(if (accepted) DictationStatus(DictationPhase.FINISHED) else DictationStatus(DictationPhase.ERROR, DictationIssue.CONTEXT_CHANGED))
                }
            })
            if (ticket != generation || !status.active) {
                try { created.cancel() } finally { created.destroy() }
                return
            }
            engine = created
            cancelTimeout = platform.timeout(30_000) { if (ticket == generation && status.active) fail(DictationIssue.TIMEOUT) }
            engine?.start(language)
        } catch (_: SecurityException) { fail(DictationIssue.PERMISSION_REVOKED) }
        catch (_: Exception) { fail(DictationIssue.ENGINE_FAILURE) }
    }

    private fun fail(issue: DictationIssue) { cleanup(); update(DictationStatus(DictationPhase.ERROR, issue)) }
    private fun cleanup() {
        // Invalider avant cancel/destroy : certains moteurs rappellent onError immédiatement.
        generation++; captured = null
        cancelTimeout?.invoke(); cancelTimeout = null
        val previous = engine; engine = null
        status = DictationStatus()
        try { previous?.cancel() } catch (_: Exception) { }
        finally { try { previous?.destroy() } catch (_: Exception) { } }
    }
    private fun update(value: DictationStatus) { status = value; changed(value) }
    companion object { const val MAX_DRAFT_BYTES = 8192 }
}
