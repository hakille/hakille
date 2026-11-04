package dev.hakille.mobile

import android.content.Context
import android.system.Os
import android.system.OsConstants
import android.util.AtomicFile
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

/** Brouillon et marqueurs d'incertitude du cœur, sans jeton ni payload de requête. */
class DraftStore(context: Context) {
    private val directory = context.filesDir
    private val file = AtomicFile(File(directory, "mobile-draft-v2.json"))

    fun load(): String? {
        val stream = try { file.openRead() } catch (failure: java.io.FileNotFoundException) {
            if (file.baseFile.exists() || File(directory, "mobile-draft-v2.json.bak").exists() || File(directory, "mobile-draft-v2.json.new").exists()) throw IOException("État local illisible", failure)
            return null
        }
        val bytes = stream.use { input ->
            val output = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (output.size() <= MAX_BYTES) {
                val count = input.read(buffer, 0, minOf(buffer.size, MAX_BYTES + 1 - output.size()))
                if (count < 0) break
                output.write(buffer, 0, count)
            }
            if (output.size() > MAX_BYTES) throw IOException("État local trop volumineux")
            output.toByteArray()
        }
        return Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
    }

    fun save(json: String) {
        val bytes = json.toByteArray(Charsets.UTF_8)
        if (bytes.size > MAX_BYTES) throw IOException("État local trop volumineux")
        var stream: FileOutputStream? = null
        try {
            stream = file.startWrite()
            stream.write(bytes); stream.flush(); stream.fd.sync()
            file.finishWrite(stream); stream = null
            // AtomicFile journalise certaines erreurs sans les lever : vérifier
            // la promotion, puis synchroniser le répertoire avant tout envoi.
            if (load() != json) throw IOException("État local non confirmé après écriture")
            val descriptor = Os.open(directory.absolutePath, OsConstants.O_RDONLY, 0)
            try { Os.fsync(descriptor) } finally { Os.close(descriptor) }
        } catch (error: Exception) {
            stream?.let { file.failWrite(it) }
            throw IOException("Échec de sauvegarde de l’état local", error)
        }
    }
    companion object { const val MAX_BYTES = 128 * 1024 }
}

/** Barrière commune : aucun envoi après échec ou annulation de l'écriture. */
internal suspend fun <T> afterDurableDraft(persist: suspend () -> Unit, send: suspend () -> T): T {
    persist()
    currentCoroutineContext().ensureActive()
    return send()
}
