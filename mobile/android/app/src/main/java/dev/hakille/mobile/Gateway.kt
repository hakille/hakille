package dev.hakille.mobile

import java.net.URI
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.IOException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** Credentials never enter a Rust snapshot, log, URL or exception message. */
class HubCredentials(val endpoint: String, val token: String) {
    override fun toString() = "HubCredentials(redacted)"
}

object EndpointPolicy {
    fun canonical(value: String): String {
        val uri = try { URI(value) } catch (_: Exception) { error("Invalid gateway URL / URL de passerelle invalide") }
        require(uri.rawUserInfo == null && uri.rawQuery == null && uri.rawFragment == null) {
            "URL credentials, query and fragment forbidden / Identifiants, paramètres et fragment interdits"
        }
        require(uri.rawPath.isNullOrEmpty() || uri.rawPath == "/") { "Use a base URL without a path / URL de base sans chemin requise" }
        require(uri.host != null && uri.port in -1..65535 && uri.port != 0) { "Invalid host or port / Hôte ou port invalide" }
        require(uri.scheme == "https" || (uri.scheme == "http" && uri.host == "127.0.0.1")) {
            "HTTPS required; HTTP allowed only on 127.0.0.1 / HTTPS requis hors boucle locale"
        }
        return URI(uri.scheme, null, uri.host.lowercase(), uri.port, null, null, null).toASCIIString()
    }

    fun validateToken(token: String) {
        require(token.length in 1..4096 && token.all { it.code in 33..126 }) {
            "Invalid gateway token / Jeton de passerelle invalide"
        }
    }

    fun validateHexBearerToken(token: String) {
        require(token.length == 64 && token.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) {
            "Invalid 64-hex gateway token / Jeton hexadécimal 64 caractères attendu"
        }
    }

    fun parsePairingQr(rawJson: String): HubCredentials {
        require(rawJson.isNotBlank() && rawJson.length <= 8192) {
            "Invalid QR payload size / Taille du QR d'appairage invalide"
        }
        val element = kotlinx.serialization.json.Json.parseToJsonElement(rawJson)
        val obj = element as? kotlinx.serialization.json.JsonObject
            ?: error("Invalid QR JSON object / Objet JSON d'appairage invalide")
        require(obj.keys == setOf("endpoint", "token")) {
            "Unexpected QR fields / Champs QR inattendus"
        }
        val rawEndpoint = (obj["endpoint"] as? kotlinx.serialization.json.JsonPrimitive)?.content?.trim().orEmpty()
        val rawToken = (obj["token"] as? kotlinx.serialization.json.JsonPrimitive)?.content?.trim().orEmpty()
        val canonicalEndpoint = canonical(rawEndpoint.trimEnd('/'))
        validateHexBearerToken(rawToken)
        return HubCredentials(canonicalEndpoint, rawToken)
    }
}

class Gateway {
    private val client = OkHttpClient.Builder()
        .followRedirects(false).followSslRedirects(false).retryOnConnectionFailure(false)
        .connectTimeout(10, TimeUnit.SECONDS).readTimeout(700, TimeUnit.SECONDS)
        .writeTimeout(30, TimeUnit.SECONDS).callTimeout(700, TimeUnit.SECONDS).build()

    suspend fun send(credentials: HubCredentials, frame: String, session: String?): String = suspendCancellableCoroutine { continuation ->
        val requestBytes = frame.toByteArray(Charsets.UTF_8)
        if (requestBytes.size > MAX_BYTES) {
            continuation.resumeWithException(IOException("Request too large / Requête trop volumineuse"))
            return@suspendCancellableCoroutine
        }
        val builder = Request.Builder().url(credentials.endpoint + "/v1/request")
            .header("Authorization", "Bearer ${credentials.token}")
            .post(requestBytes.toRequestBody("application/json".toMediaType()))
        if (!session.isNullOrEmpty()) builder.header("X-Hakille-Session", session)
        val call = client.newCall(builder.build())
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                if (continuation.isActive) continuation.resumeWithException(IOException("Gateway transport failed; outcome may be unknown / Transport interrompu, résultat potentiellement inconnu"))
            }
            override fun onResponse(call: Call, response: Response) {
                response.use {
                    try {
                        require(response.code !in 300..399) { "Redirect refused / Redirection refusée" }
                        val body = response.body ?: error("Empty gateway response / Réponse vide")
                        require(body.contentLength() <= MAX_BYTES) { "Gateway response too large / Réponse trop volumineuse" }
                        val output = java.io.ByteArrayOutputStream()
                        val buffer = ByteArray(8192)
                        val stream = body.byteStream()
                        while (output.size() <= MAX_BYTES) {
                            val count = stream.read(buffer, 0, minOf(buffer.size, MAX_BYTES + 1 - output.size()))
                            if (count < 0) break
                            output.write(buffer, 0, count)
                        }
                        val bytes = output.toByteArray()
                        require(bytes.size <= MAX_BYTES) { "Gateway response too large / Réponse trop volumineuse" }
                        // Both success and error envelopes go to Rust, including issue_unknown.
                        val payload = Charsets.UTF_8.newDecoder()
                            .onMalformedInput(java.nio.charset.CodingErrorAction.REPORT)
                            .onUnmappableCharacter(java.nio.charset.CodingErrorAction.REPORT)
                            .decode(java.nio.ByteBuffer.wrap(bytes)).toString()
                        if (continuation.isActive) continuation.resume(payload)
                    } catch (_: Exception) {
                        if (continuation.isActive) continuation.resumeWithException(IOException("Invalid gateway response; no retry / Réponse invalide, aucune relance"))
                    }
                }
            }
        })
    }

    suspend fun sendStream(
        credentials: HubCredentials,
        frame: String,
        session: String?,
        onFrame: (String) -> Unit
    ): Unit = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
        val requestBytes = frame.toByteArray(Charsets.UTF_8)
        if (requestBytes.size > MAX_BYTES) {
            throw IOException("Request too large / Requête trop volumineuse")
        }
        val builder = Request.Builder().url(credentials.endpoint + "/v1/request")
            .header("Authorization", "Bearer ${credentials.token}")
            .header("Accept", "text/event-stream")
            .post(requestBytes.toRequestBody("application/json".toMediaType()))
        if (!session.isNullOrEmpty()) builder.header("X-Hakille-Session", session)
        val call = client.newCall(builder.build())
        val response = call.execute()
        response.use { resp ->
            require(resp.code !in 300..399) { "Redirect refused / Redirection refusée" }
            if (resp.code !in 200..299) {
                val errorBody = resp.body?.string().orEmpty()
                throw IOException("Gateway error ${resp.code}: $errorBody")
            }
            val body = resp.body ?: throw IOException("Empty gateway response / Réponse vide")
            val contentType = resp.header("Content-Type").orEmpty()
            if (contentType.contains("text/event-stream")) {
                val reader = body.byteStream().bufferedReader(Charsets.UTF_8)
                var line: String? = reader.readLine()
                while (line != null) {
                    val trimmed = line.trim()
                    if (trimmed.startsWith("data:")) {
                        val data = trimmed.removePrefix("data:").trim()
                        if (data != "{}") {
                            onFrame(data)
                        }
                    } else if (trimmed.startsWith("event: done")) {
                        break
                    }
                    line = reader.readLine()
                }
            } else {
                val fullBody = body.string()
                onFrame(fullBody)
            }
        }
    }

    companion object { const val MAX_BYTES = 1024 * 1024 }
}
