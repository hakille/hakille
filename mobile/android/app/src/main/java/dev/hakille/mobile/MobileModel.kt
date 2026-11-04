package dev.hakille.mobile

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.*
import uniffi.hakille_mobile.MobileBridge

fun JsonObject.text(key: String): String = (get(key) as? JsonPrimitive)?.contentOrNull.orEmpty()
fun JsonObject.objectOrNull(key: String): JsonObject? = get(key) as? JsonObject
fun JsonObject.rows(key: String): List<JsonObject> = (get(key) as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()

class MobileModel(application: Application) : AndroidViewModel(application) {
    private var bridge = MobileBridge()
    private val store = CredentialStore(application)
    private val gateway = Gateway()
    private val drafts = DraftStore(application)
    private val draftMutex = Mutex()
    private var foregroundActive = true
    private var restoreBlocked = false
    private var persistenceVersion = 0L
    private val _restoring = MutableStateFlow(true)
    val restoring = _restoring.asStateFlow()
    private val _localStateAvailable = MutableStateFlow(false)
    val localStateAvailable = _localStateAvailable.asStateFlow()
    private val _persistenceError = MutableStateFlow("")
    val persistenceError = _persistenceError.asStateFlow()
    private var credentials: HubCredentials? = null
    private var operation: Job? = null
    private var configurationJob: Job? = null
    private val credentialMutex = Mutex()
    private var generation = 0L
    private var draftRevision = 0L
    private val _snapshot = MutableStateFlow(Json.parseToJsonElement(bridge.snapshotJson()).jsonObject)
    val snapshot = _snapshot.asStateFlow()
    private val _error = MutableStateFlow("")
    val error = _error.asStateFlow()
    private val _savedEndpoint = MutableStateFlow("")
    val savedEndpoint = _savedEndpoint.asStateFlow()

    private val _dictationStatus = MutableStateFlow(DictationStatus())
    val dictationStatus = _dictationStatus.asStateFlow()
    val dictation = DictationSession(
        AndroidDictationPlatform(application),
        context = ::dictationContext,
        commitDraft = { expected, text ->
            if (dictationContext() != expected) false
            else dispatch("set_draft") { put("text", text) }
        },
        changed = { _dictationStatus.value = it },
    )
    private fun dictationContext() = DictationContext(
        generation, draftRevision, _snapshot.value.text("draft"),
        !_restoring.value && !restoreBlocked && foregroundActive &&
            _snapshot.value["foreground"]?.jsonPrimitive?.booleanOrNull == true &&
            _snapshot.value.objectOrNull("pending") == null && _snapshot.value.rows("cards").isEmpty(),
    )

    init {
        viewModelScope.launch {
            try {
                val saved = draftMutex.withLock { withContext(Dispatchers.IO) { drafts.load() } }
                if (saved != null) {
                    val result = Json.parseToJsonElement(bridge.restoreDraftJson(saved)).jsonObject
                    if (result["ok"]?.jsonPrimitive?.booleanOrNull != true) error("État local refusé par le cœur")
                    _snapshot.value = result.getValue("snapshot").jsonObject
                }
            } catch (_: Exception) {
                restoreBlocked = true
                _persistenceError.value = getApplication<Application>().getString(R.string.draft_restore_failed)
            } finally {
                _restoring.value = false
                _localStateAvailable.value = !restoreBlocked
                accept(bridge.dispatchJson(buildJsonObject { put("type", "set_foreground"); put("active", foregroundActive) }.toString()))
            }
        }
        val epoch = generation
        viewModelScope.launch {
            try {
                val endpoint = credentialMutex.withLock { withContext(Dispatchers.IO) { store.load()?.endpoint.orEmpty() } }
                if (epoch == generation) _savedEndpoint.value = endpoint
            } catch (_: Exception) {
                if (epoch == generation) _error.value = "Saved credentials unavailable; reconnect / Identifiants enregistrés indisponibles"
            }
        }
    }

    private suspend fun persist(json: String) {
        if (restoreBlocked) throw java.io.IOException("État local non restauré")
        draftMutex.withLock { withContext(Dispatchers.IO) { drafts.save(json) } }
    }

    private fun queuePersistence(clearStorageError: Boolean = true) {
        if (_restoring.value || restoreBlocked) return
        val payload = bridge.draftJson()
        val version = ++persistenceVersion
        viewModelScope.launch {
            try {
                persist(payload)
                if (clearStorageError && version == persistenceVersion) _persistenceError.value = ""
            } catch (_: CancellationException) { }
            catch (_: Exception) {
                if (version == persistenceVersion) _persistenceError.value = getApplication<Application>().getString(R.string.draft_save_failed)
            }
        }
    }

    private fun resetConnection(): Long {
        dictation.cancel(); draftRevision++
        generation++; configurationJob?.cancel(); operation?.cancel(); operation = null; credentials = null
        // Une déconnexion interrompt l’ancien tour sans effacer son incertitude.
        bridge.dispatchJson("{\"type\":\"set_foreground\",\"active\":false}")
        val preserved = bridge.draftJson()
        bridge = MobileBridge()
        val restored = Json.parseToJsonElement(bridge.restoreDraftJson(preserved)).jsonObject
        _snapshot.value = restored.getValue("snapshot").jsonObject
        accept(bridge.dispatchJson(buildJsonObject { put("type", "set_foreground"); put("active", foregroundActive) }.toString()))
        return generation
    }

    fun connect(endpoint: String, token: String, useSaved: Boolean = false) {
        if (_restoring.value || restoreBlocked) return
        val epoch = resetConnection()
        configurationJob = viewModelScope.launch {
            try {
                val candidate = if (useSaved) credentialMutex.withLock { withContext(Dispatchers.IO) { store.load() } }
                    ?: error("No saved hub / Aucun hub enregistré")
                    else HubCredentials(EndpointPolicy.canonical(endpoint.trim()), token.trim())
                val checked = HubCredentials(EndpointPolicy.canonical(candidate.endpoint), candidate.token)
                EndpointPolicy.validateToken(checked.token)
                if (epoch != generation) return@launch
                credentialMutex.withLock { withContext(Dispatchers.IO) { store.save(checked) } }
                ensureActive()
                if (epoch != generation) return@launch
                credentials = checked; _savedEndpoint.value = checked.endpoint; _error.value = ""
                dispatch("set_hub") { put("hub_id", checked.endpoint) }
                dispatch("refresh_status")
            } catch (_: CancellationException) { /* Replaced or disconnected explicitly. */ }
            catch (_: Exception) {
                if (epoch == generation) _error.value = "Connection configuration failed; check URL, token and secure storage / Connexion refusée : vérifier URL, jeton et stockage sécurisé"
            }
        }
    }

    fun disconnect() {
        if (_restoring.value) return
        val epoch = resetConnection()
        _savedEndpoint.value = ""
        viewModelScope.launch {
            try { credentialMutex.withLock { withContext(Dispatchers.IO) { store.clear() } } }
            catch (_: CancellationException) { }
            catch (_: Exception) { if (epoch == generation) _error.value = "Could not erase credentials / Effacement des identifiants impossible" }
        }
    }

    fun dispatch(type: String, values: JsonObjectBuilder.() -> Unit = {}): Boolean {
        if (_restoring.value || restoreBlocked) return false
        if (type != "set_foreground") dictation.cancel()
        if (type == "set_draft") draftRevision++
        val action = buildJsonObject { put("type", type); values() }
        return accept(bridge.dispatchJson(action.toString()))
    }

    fun foreground(active: Boolean) {
        foregroundActive = active
        if (!active) dictation.cancel() else dictation.checkPermission()
        if (!active) { generation++; operation?.cancel(); operation = null; configurationJob?.cancel() }
        if (!_restoring.value) accept(bridge.dispatchJson(buildJsonObject { put("type", "set_foreground"); put("active", active) }.toString()))
    }
    fun cancelRequest() {
        dictation.cancel()
        val pending = _snapshot.value.objectOrNull("pending") ?: return
        generation++; operation?.cancel(); operation = null
        accept(bridge.failRequest(pending.getValue("request_id").jsonPrimitive.content.toULong(), "Transport cancelled; outcome unknown / Transport annulé, résultat inconnu"))
    }

    private fun accept(json: String, clearStorageError: Boolean = true): Boolean {
        val result = Json.parseToJsonElement(json).jsonObject
        _snapshot.value = result.getValue("snapshot").jsonObject
        _error.value = result.text("error")
        val ok = result["ok"]?.jsonPrimitive?.booleanOrNull == true
        val frame = result.objectOrNull("request")
        if (frame == null) { queuePersistence(clearStorageError); return ok }
        val configuration = credentials
        val id = frame.getValue("id").jsonPrimitive.content.toULong()
        if (configuration == null) {
            failBeforeSend(id, "Connect to a hub first / Connectez d’abord un hub"); return false
        }
        val epoch = generation
        val durableIntent = bridge.draftJson()
        val version = ++persistenceVersion
        val session = _snapshot.value.text("gateway_session").ifEmpty { null }
        operation = viewModelScope.launch {
            var sent = false
            try {
                val response = afterDurableDraft(persist = {
                    try {
                        persist(durableIntent)
                        if (version == persistenceVersion) _persistenceError.value = ""
                    } catch (cancelled: CancellationException) { throw cancelled }
                    catch (failure: Exception) {
                        _persistenceError.value = getApplication<Application>().getString(R.string.draft_save_failed)
                        throw failure
                    }
                }) {
                    if (epoch != generation) throw CancellationException()
                    sent = true
                    gateway.send(configuration, frame.toString(), session)
                }
                if (epoch == generation) accept(bridge.receiveJson(id, response))
            } catch (_: CancellationException) { /* Cancellation is reported explicitly by the initiating action. */ }
            catch (_: Exception) {
                if (epoch == generation) {
                    if (sent) accept(bridge.failRequest(id, "Gateway request failed; no automatic retry / Requête interrompue, aucune relance automatique"))
                    else failBeforeSend(id, getApplication<Application>().getString(R.string.draft_request_not_sent), clearStorageError = false)
                }
            }
        }
        return ok
    }
    private fun failBeforeSend(id: ULong, message: String, clearStorageError: Boolean = true) {
        accept(bridge.dispatchJson(buildJsonObject {
            put("type", "fail_before_send"); put("request_id", JsonPrimitive(id.toString().toBigInteger())); put("message", message)
        }.toString()), clearStorageError)
    }

    override fun onCleared() {
        dictation.cancel()
        super.onCleared()
    }
}
