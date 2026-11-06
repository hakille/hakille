package dev.hakille.mobile

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import java.util.Locale

/** Aucune voix réseau, aucune installation automatique, aucun redémarrage ASR. */
class AndroidLocalSpeaker(private val context: Context, private val changed: (LocalSpeechStatus) -> Unit) {
    private val handler = Handler(Looper.getMainLooper())
    private val audio = context.getSystemService(AudioManager::class.java)
    private var engine: TextToSpeech? = null
    private var generation = 0L
    private var timer: Runnable? = null
    private var focus: AudioFocusRequest? = null

    fun speak(text: String, languageTag: String) {
        stop(false)
        if (text.isBlank() || text.length > TextToSpeech.getMaxSpeechInputLength()) {
            changed(LocalSpeechStatus(LocalSpeechPhase.ERROR, LocalSpeechIssue.TEXT_TOO_LONG)); return
        }
        val ticket = generation
        changed(LocalSpeechStatus(LocalSpeechPhase.INITIALIZING))
        armTimeout(ticket, 10_000)
        try {
            engine = TextToSpeech(context) { result -> handler.post {
                if (ticket != generation) return@post
                val active = engine
                if (result != TextToSpeech.SUCCESS || active == null) { fail(LocalSpeechIssue.ENGINE); return@post }
                try {
                    val locale = Locale.forLanguageTag(languageTag)
                    val available = active.voices.orEmpty()
                    val selected = chooseLocalVoice(available.map { LocalVoice(it.name, it.locale.language, it.locale.country,
                        it.isNetworkConnectionRequired, !it.features.orEmpty().contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED)) }, locale.language, locale.country)
                    val voice = available.firstOrNull { it.name == selected }
                    if (voice == null || active.setVoice(voice) != TextToSpeech.SUCCESS) { fail(LocalSpeechIssue.VOICE_UNAVAILABLE); return@post }
                    val attributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANT)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
                    val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
                        .setAudioAttributes(attributes).setOnAudioFocusChangeListener({ change ->
                            if (ticket == generation && change != AudioManager.AUDIOFOCUS_GAIN) stop()
                        }, handler).build()
                    focus = request
                    if (audio == null || audio.requestAudioFocus(request) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
                        fail(LocalSpeechIssue.AUDIO_FOCUS); return@post
                    }
                    active.setAudioAttributes(attributes)
                    active.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                        override fun onStart(id: String?) { handler.post { if (ticket == generation) changed(LocalSpeechStatus(LocalSpeechPhase.SPEAKING)) } }
                        override fun onDone(id: String?) { handler.post { if (ticket == generation) { stop(false); changed(LocalSpeechStatus(LocalSpeechPhase.FINISHED)) } } }
                        @Deprecated("Callback Android historique")
                        override fun onError(id: String?) { handler.post { if (ticket == generation) fail(LocalSpeechIssue.ENGINE) } }
                    })
                    armTimeout(ticket, 120_000)
                    if (active.speak(text, TextToSpeech.QUEUE_FLUSH, null, ticket.toString()) != TextToSpeech.SUCCESS) fail(LocalSpeechIssue.ENGINE)
                } catch (_: Exception) { fail(LocalSpeechIssue.ENGINE) }
            } }
        } catch (_: Exception) { fail(LocalSpeechIssue.ENGINE) }
    }
    fun stop(announce: Boolean = true) {
        generation++
        timer?.let(handler::removeCallbacks); timer = null
        val old = engine; engine = null
        try { old?.stop() } catch (_: Exception) { }
        try { old?.shutdown() } catch (_: Exception) { }
        focus?.let { try { audio?.abandonAudioFocusRequest(it) } catch (_: Exception) { } }; focus = null
        changed(LocalSpeechStatus(if (announce) LocalSpeechPhase.CANCELLED else LocalSpeechPhase.IDLE))
    }
    private fun armTimeout(ticket: Long, millis: Long) {
        timer?.let(handler::removeCallbacks)
        timer = Runnable { if (ticket == generation) fail(LocalSpeechIssue.TIMEOUT) }.also { handler.postDelayed(it, millis) }
    }
    private fun fail(issue: LocalSpeechIssue) { stop(false); changed(LocalSpeechStatus(LocalSpeechPhase.ERROR, issue)) }
}
