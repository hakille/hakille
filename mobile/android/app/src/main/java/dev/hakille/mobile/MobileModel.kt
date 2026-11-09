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
    private val _requestStartTimeMs = MutableStateFlow<Long?>(null)
    val requestStartTimeMs = _requestStartTimeMs.asStateFlow()
    private val _savedEndpoint = MutableStateFlow("")
    val savedEndpoint = _savedEndpoint.asStateFlow()
    val devices = DeviceCapabilitiesController(application, viewModelScope)

    private val voicePolicy = VoiceTurnPolicy()
    private var voiceLanguage = java.util.Locale.getDefault().toLanguageTag()
    private val _voiceConversationEnabled = MutableStateFlow(false)
    val voiceConversationEnabled = _voiceConversationEnabled.asStateFlow()
    private val _voiceDictationStatus = MutableStateFlow(DictationStatus())
    val voiceDictationStatus = _voiceDictationStatus.asStateFlow()
    private val _speechStatus = MutableStateFlow(LocalSpeechStatus())
    val speechStatus = _speechStatus.asStateFlow()
    private val _voiceNotice = MutableStateFlow("")
    val voiceNotice = _voiceNotice.asStateFlow()
    private val speaker = AndroidLocalSpeaker(application) { _speechStatus.value = it }

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
        prepare = { stopVoiceTurn(); true },
    )
    val voiceDictation = DictationSession(
        AndroidDictationPlatform(application), context = ::voiceContext,
        commitDraft = { expected, text ->
            if (voiceContext() != expected || expected.voiceContextId == null) false
            else {
                val accepted = dispatch("submit_voice") {
                    put("context_id", JsonPrimitive(expected.voiceContextId.toBigInteger())); put("text", text)
                }
                val id = _snapshot.value.objectOrNull("pending")?.text("request_id")?.toULongOrNull()
                if (accepted && id != null) voicePolicy.awaitReply(id) else false
            }
        },
        changed = {
            _voiceDictationStatus.value = it
            if (it.phase in setOf(DictationPhase.ERROR, DictationPhase.CANCELLED)) {
                voicePolicy.cancel()
                if (!_restoring.value && !restoreBlocked) accept(bridge.dispatchJson("{\"type\":\"cancel_voice\"}"))
            }
        },
        prepare = {
            dictation.cancel(); speaker.stop(false); voicePolicy.cancel()
            _voiceConversationEnabled.value && credentials != null && foregroundActive && dispatch("begin_voice")
        },
    )
    private fun voiceContext(): DictationContext {
        val base = dictationContext()
        val id = _snapshot.value["voice_context_id"]?.jsonPrimitive?.contentOrNull?.takeUnless { it == "null" }
        return base.copy(allowed = base.allowed && _voiceConversationEnabled.value && id != null,
            voiceContextId = id)
    }
    fun enableVoiceConversation(enabled: Boolean) {
        stopVoiceTurn()
        voicePolicy.enable(enabled)
        _voiceConversationEnabled.value = enabled
        if (enabled) _voiceNotice.value = ""
    }
    fun startVoiceTurn(language: String): Long? {
        voiceLanguage = language
        return voiceDictation.requestStart(language)
    }
    fun stopVoicePlayback() { speaker.stop(); voicePolicy.cancel() }
    fun stopVoiceTurn() {
        val voiceRequest = _snapshot.value.text("voice_request_id").toULongOrNull()
        if (voiceRequest != null && _snapshot.value.objectOrNull("pending")?.text("request_id")?.toULongOrNull() == voiceRequest) {
            generation++; operation?.cancel(); operation = null
        }
        voiceDictation.cancel(); speaker.stop(false); voicePolicy.cancel()
        if (!_restoring.value && !restoreBlocked) accept(bridge.dispatchJson("{\"type\":\"cancel_voice\"}"))
    }
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
        devices.clear()
        enableVoiceConversation(false)
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
        if (type !in setOf("begin_voice", "submit_voice", "cancel_voice", "set_foreground")) stopVoiceTurn()
        if (type != "set_foreground") dictation.cancel()
        if (type == "set_draft") draftRevision++
        val action = buildJsonObject { put("type", type); values() }
        return accept(bridge.dispatchJson(action.toString()))
    }

    fun foreground(active: Boolean) {
        if (!active && voiceDictation.status.phase == DictationPhase.PERMISSION) {
            _voiceNotice.value = getApplication<Application>().getString(R.string.voice_permission_resume)
        }
        foregroundActive = active
        if (!active) devices.pause()
        if (!active) enableVoiceConversation(false) else voiceDictation.checkPermission()
        if (!active) dictation.cancel() else dictation.checkPermission()
        if (!active) { generation++; operation?.cancel(); operation = null; configurationJob?.cancel() }
        if (!_restoring.value) accept(bridge.dispatchJson(buildJsonObject { put("type", "set_foreground"); put("active", active) }.toString()))
    }
    fun cancelRequest() {
        stopVoiceTurn()
        dictation.cancel()
        _requestStartTimeMs.value = null
        val pending = _snapshot.value.objectOrNull("pending") ?: return
        generation++; operation?.cancel(); operation = null
        accept(bridge.failRequest(pending.getValue("request_id").jsonPrimitive.content.toULong(), "Transport cancelled; outcome unknown / Transport annulé, résultat inconnu"))
    }

    fun ghostActionAccept(id: String): Boolean = dispatch("ghost_action_accept") { put("id", id) }
    fun ghostActionDismiss(id: String): Boolean = dispatch("ghost_action_dismiss") { put("id", id) }

    fun undoAction(journalId: Long): Boolean = dispatch("undo") { put("journal_id", JsonPrimitive(journalId)) }
    fun forgetFact(rowId: String): Boolean = dispatch("forget_memory") { put("row_id", rowId) }
    fun setAutonomyCrank(domain: String, crank: Int, maxCostCents: Long? = null, maxActions: Int? = null, daysValid: Int? = null): Boolean =
        dispatch("set_autonomy_crank") {
            put("domain", domain)
            put("crank", JsonPrimitive(crank))
            if (maxCostCents != null) put("max_cost_cents", JsonPrimitive(maxCostCents))
            if (maxActions != null) put("max_actions", JsonPrimitive(maxActions))
            if (daysValid != null) put("days_valid", JsonPrimitive(daysValid))
        }
    fun requestWhy(targetId: String, targetType: String): Boolean =
        dispatch("request_why") {
            put("target_id", targetId)
            put("target_type", targetType)
        }
    fun closeWhy(): Boolean = dispatch("close_why")
    fun runAgent(name: String, inputJson: String): Boolean =
        dispatch("run_agent") {
            put("name", name)
            put("input_json", inputJson)
        }
    fun loadJournal(limit: Int = 50): Boolean = dispatch("load_journal") { put("limit", JsonPrimitive(limit)) }
    fun loadMemory(query: String? = null, limit: Int = 50): Boolean =
        dispatch("load_memory") {
            if (query != null) put("query", query)
            put("limit", JsonPrimitive(limit))
        }
    fun rememberFact(text: String): Boolean = dispatch("remember") { put("text", text) }
    fun loadAgents(): Boolean = dispatch("load_agents")
    fun dismissPrivacyGuard(): Boolean = dispatch("dismiss_privacy_guard")
    fun startGuestPass(name: String, power: String, durationSecs: Int = 3600): Boolean =
        dispatch("start_guest_pass") {
            put("name", name)
            put("power", power)
            put("duration_secs", JsonPrimitive(durationSecs))
        }
    fun tickGuestPass(elapsedSecs: Int): Boolean =
        dispatch("tick_guest_pass") {
            put("elapsed_secs", JsonPrimitive(elapsedSecs))
        }
    fun revokeGuestPass(): Boolean = dispatch("revoke_guest_pass")
    fun connectFromPairingQr(rawQrJson: String) {
        try {
            val parsed = EndpointPolicy.parsePairingQr(rawQrJson)
            connect(parsed.endpoint, parsed.token, useSaved = false)
        } catch (_: Exception) {
            _error.value = "Invalid pairing QR / QR d'appairage invalide"
        }
    }

    private fun accept(json: String, clearStorageError: Boolean = true): Boolean {
        val result = Json.parseToJsonElement(json).jsonObject
        _snapshot.value = result.getValue("snapshot").jsonObject
        _error.value = result.text("error")
        val ok = result["ok"]?.jsonPrimitive?.booleanOrNull == true
        val frame = result.objectOrNull("request")
        if (frame == null) {
            _requestStartTimeMs.value = null
            queuePersistence(clearStorageError)
            return ok
        }
        _requestStartTimeMs.value = System.currentTimeMillis()
        val configuration = credentials
        val id = frame.getValue("id").jsonPrimitive.content.toULong()
        if (configuration == null) {
            _requestStartTimeMs.value = null
            failBeforeSend(id, "Connect to a hub first / Connectez d’abord un hub"); return false
        }
        val epoch = generation
        val durableIntent = bridge.draftJson()
        val version = ++persistenceVersion
        val session = _snapshot.value.text("gateway_session").ifEmpty { null }
        val isStreamingCandidate = frame.objectOrNull("body")?.objectOrNull("Request")
            ?.objectOrNull("message")?.let { it.containsKey("HudSubmit") } == true
        operation = viewModelScope.launch {
            var sent = false
            try {
                if (isStreamingCandidate) {
                    afterDurableDraft(persist = {
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
                        gateway.sendStream(configuration, frame.toString(), session) { chunk ->
                            if (epoch == generation) {
                                viewModelScope.launch(Dispatchers.Main) {
                                    val resJson = bridge.receiveStreamFrame(id, chunk)
                                    val streamResult = Json.parseToJsonElement(resJson).jsonObject
                                    _snapshot.value = streamResult.getValue("snapshot").jsonObject
                                    _error.value = streamResult.text("error")
                                }
                            }
                        }
                    }
                    if (epoch == generation) {
                        _requestStartTimeMs.value = null
                        val reply = _snapshot.value.objectOrNull("voice_reply")
                        val voiceRequest = _snapshot.value.text("voice_request_id").toULongOrNull()
                        val correlated = reply != null && reply.text("request_id").toULongOrNull() == id &&
                            reply.text("context_id") == _snapshot.value.text("voice_context_id") && voiceRequest == id
                        val expectedVoiceReply = voicePolicy.expects(id)
                        val text = voicePolicy.finish(id, correlated, reply?.text("text"))
                        if (text != null && foregroundActive && _voiceConversationEnabled.value) speaker.speak(text, voiceLanguage)
                        else if (expectedVoiceReply && voiceRequest == id && foregroundActive && _voiceConversationEnabled.value) {
                            val outcome = _snapshot.value.rows("outcomes").firstOrNull { it.text("request_id").toULongOrNull() == id }
                            val responseText = outcome?.objectOrNull("result")?.objectOrNull("HudReply")?.text("response")
                            if (outcome?.text("state") == "confirmed" && responseText != null && responseText.toByteArray(Charsets.UTF_8).size > 8192) {
                                _speechStatus.value = LocalSpeechStatus(LocalSpeechPhase.ERROR, LocalSpeechIssue.TEXT_TOO_LONG)
                            }
                        }
                        queuePersistence(clearStorageError = true)
                    }
                } else {
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
                    if (epoch == generation) {
                        _requestStartTimeMs.value = null
                        val accepted = accept(bridge.receiveJson(id, response))
                        val reply = _snapshot.value.objectOrNull("voice_reply")
                        val voiceRequest = _snapshot.value.text("voice_request_id").toULongOrNull()
                        val correlated = reply != null && reply.text("request_id").toULongOrNull() == id &&
                            reply.text("context_id") == _snapshot.value.text("voice_context_id") && voiceRequest == id
                        val expectedVoiceReply = voicePolicy.expects(id)
                        val text = voicePolicy.finish(id, accepted && correlated, reply?.text("text"))
                        if (text != null && foregroundActive && _voiceConversationEnabled.value) speaker.speak(text, voiceLanguage)
                        else if (expectedVoiceReply && accepted && voiceRequest == id && foregroundActive && _voiceConversationEnabled.value) {
                            val outcome = _snapshot.value.rows("outcomes").firstOrNull { it.text("request_id").toULongOrNull() == id }
                            val responseText = outcome?.objectOrNull("result")?.objectOrNull("HudReply")?.text("response")
                            if (outcome?.text("state") == "confirmed" && responseText != null && responseText.toByteArray(Charsets.UTF_8).size > 8192) {
                                _speechStatus.value = LocalSpeechStatus(LocalSpeechPhase.ERROR, LocalSpeechIssue.TEXT_TOO_LONG)
                            }
                        }
                    }
                }
            } catch (_: CancellationException) { /* Cancellation is reported explicitly by the initiating action. */ }
            catch (_: Exception) {
                if (epoch == generation) {
                    _requestStartTimeMs.value = null
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
        devices.clear()
        voiceDictation.cancel(); speaker.stop(false)
        dictation.cancel()
        super.onCleared()
    }
}
