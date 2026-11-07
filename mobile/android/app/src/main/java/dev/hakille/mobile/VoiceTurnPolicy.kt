package dev.hakille.mobile

/** Une activation explicite autorise des tours ponctuels ; aucune relance micro. */
class VoiceTurnPolicy {
    var enabled = false; private set
    private var pending: ULong? = null
    fun enable(value: Boolean) { enabled = value; pending = null }
    fun cancel() { pending = null }
    fun expects(id: ULong): Boolean = enabled && pending == id
    fun awaitReply(id: ULong): Boolean {
        if (!enabled || pending != null) return false
        pending = id
        return true
    }
    fun finish(id: ULong, confirmed: Boolean, reply: String?): String? {
        if (!enabled || pending != id) return null
        pending = null
        return reply?.takeIf { confirmed && it.isNotBlank() }
    }
}

enum class LocalSpeechPhase { IDLE, INITIALIZING, SPEAKING, FINISHED, CANCELLED, ERROR }
enum class LocalSpeechIssue { ENGINE, VOICE_UNAVAILABLE, TEXT_TOO_LONG, AUDIO_FOCUS, TIMEOUT }
data class LocalSpeechStatus(val phase: LocalSpeechPhase = LocalSpeechPhase.IDLE, val issue: LocalSpeechIssue? = null)

/** Politique indépendante des classes Android, également utilisée par le moteur réel. */
data class LocalVoice(val name: String, val language: String, val country: String, val networkRequired: Boolean, val installed: Boolean)
fun chooseLocalVoice(voices: List<LocalVoice>, language: String, country: String): String? = voices
    .filter { !it.networkRequired && it.installed && it.language == language }
    .sortedWith(compareBy<LocalVoice> { it.country != country }.thenBy { it.name })
    .firstOrNull()?.name
