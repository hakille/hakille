package dev.hakille.mobile

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat

/** Aucun appel à createSpeechRecognizer : seul le moteur sur appareil est autorisé. */
class AndroidDictationPlatform(private val context: Context) : DictationPlatform {
    private val handler = Handler(Looper.getMainLooper())
    override fun unavailable(): DictationIssue? {
        if (Build.VERSION.SDK_INT < 31) return DictationIssue.ANDROID_VERSION
        return try { if (SpeechRecognizer.isOnDeviceRecognitionAvailable(context)) null else DictationIssue.ENGINE_MISSING }
        catch (_: Exception) { DictationIssue.ENGINE_MISSING }
    }
    override fun hasPermission() = ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
    override fun create(events: DictationEvents): DictationEngine {
        check(Looper.myLooper() == Looper.getMainLooper())
        if (Build.VERSION.SDK_INT < 31) error("La dictée sur l’appareil nécessite Android 12")
        check(hasPermission())
        return OnDeviceEngine(context, events)
    }
    override fun timeout(milliseconds: Long, action: () -> Unit): () -> Unit {
        val runnable = Runnable(action)
        handler.postDelayed(runnable, milliseconds)
        return { handler.removeCallbacks(runnable) }
    }
}

@RequiresApi(31)
private class OnDeviceEngine(context: Context, events: DictationEvents) : DictationEngine {
    private val recognizer = SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
    init {
        recognizer.setRecognitionListener(object : RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) = events.ready()
            override fun onBeginningOfSpeech() { }
            override fun onRmsChanged(rmsdB: Float) { }
            override fun onBufferReceived(buffer: ByteArray?) { }
            override fun onEndOfSpeech() = events.speechEnded()
            override fun onResults(results: Bundle?) = events.result(results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull())
            override fun onPartialResults(partialResults: Bundle?) { }
            override fun onEvent(eventType: Int, params: Bundle?) { }
            override fun onError(error: Int) = events.error(when (error) {
                SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> DictationIssue.PERMISSION_REVOKED
                SpeechRecognizer.ERROR_RECOGNIZER_BUSY, SpeechRecognizer.ERROR_TOO_MANY_REQUESTS -> DictationIssue.BUSY
                SpeechRecognizer.ERROR_NO_MATCH, SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> DictationIssue.NO_MATCH
                SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED, SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE -> DictationIssue.LANGUAGE
                SpeechRecognizer.ERROR_AUDIO -> DictationIssue.AUDIO
                else -> DictationIssue.ENGINE_FAILURE
            })
        })
    }
    override fun start(language: String) {
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)
            .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, false)
            .putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1)
            .putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
        recognizer.startListening(intent)
    }
    override fun stop() = recognizer.stopListening()
    override fun cancel() = recognizer.cancel()
    override fun destroy() = recognizer.destroy()
}
