package dev.hakille.mobile

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.util.Locale

@OptIn(ExperimentalLayoutApi::class)
@Composable fun DictationControls(model: MobileModel, enabled: Boolean, onWrite: () -> Unit, onJournal: () -> Unit, onDevice: () -> Unit) {
    val draftState by model.dictationStatus.collectAsStateWithLifecycle()
    val conversation by model.voiceConversationEnabled.collectAsStateWithLifecycle()
    val voiceState by model.voiceDictationStatus.collectAsStateWithLifecycle()
    val speech by model.speechStatus.collectAsStateWithLifecycle()
    val voiceNotice by model.voiceNotice.collectAsStateWithLifecycle()
    val snapshot by model.snapshot.collectAsStateWithLifecycle()
    val state = if (conversation) voiceState else draftState
    val session = if (conversation) model.voiceDictation else model.dictation
    var language by rememberSaveable { mutableStateOf("system") }
    var menu by remember { mutableStateOf(false) }
    var consent by remember { mutableStateOf(false) }
    var details by remember { mutableStateOf(false) }
    var permissionTicket by remember { mutableStateOf<Long?>(null) }
    var permissionForVoice by remember { mutableStateOf(false) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        val ticket = permissionTicket
        permissionTicket = null
        if (ticket != null) (if (permissionForVoice) model.voiceDictation else model.dictation).permissionResult(ticket, granted)
    }
    // L’hôte annule sur navigation ou pause ; le recyclage de cet item lors du défilement ne doit rien interrompre.
    val unavailable = session.unavailable()
    val issue = state.issue ?: if (state.phase == DictationPhase.IDLE) unavailable else null
    val speechActive = speech.phase == LocalSpeechPhase.SPEAKING || speech.phase == LocalSpeechPhase.INITIALIZING
    val hasHub = snapshot.text("hub_id").isNotEmpty()
    val hasDraft = snapshot.text("draft").isNotEmpty()
    val canStart = enabled && unavailable == null && permissionTicket == null && !speechActive && (!conversation || (hasHub && !hasDraft))
    val statusLabel = when {
        conversation && speech.phase == LocalSpeechPhase.SPEAKING -> R.string.ui_speech_playing
        conversation && speech.phase == LocalSpeechPhase.INITIALIZING -> R.string.ui_speech_preparing
        conversation && speech.phase == LocalSpeechPhase.ERROR && speech.issue == LocalSpeechIssue.TEXT_TOO_LONG -> R.string.ui_speech_too_long
        conversation && speech.phase == LocalSpeechPhase.ERROR -> R.string.ui_speech_failed
        issue != null -> issueResource(issue)
        conversation && state.phase == DictationPhase.FINISHED && speech.phase == LocalSpeechPhase.FINISHED -> R.string.ui_speech_finished
        conversation && state.phase == DictationPhase.FINISHED -> R.string.ui_voice_transcribed
        else -> phaseResource(state.phase)
    }
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(stringResource(R.string.ui_presence_title), style = MaterialTheme.typography.headlineMedium, textAlign = TextAlign.Center, modifier = Modifier.semantics { heading() })
        PresenceMark(listening = state.phase == DictationPhase.LISTENING)
        Text(stringResource(statusLabel), style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center,
            color = if (state.phase == DictationPhase.ERROR || (conversation && speech.phase == LocalSpeechPhase.ERROR)) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
        if (voiceNotice.isNotEmpty()) StateNotice(voiceNotice)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(selected = !conversation, onClick = { model.enableVoiceConversation(false) },
                enabled = !state.active && permissionTicket == null, label = { Text(stringResource(R.string.ui_draft_mode)) })
            FilterChip(selected = conversation, onClick = { if (!conversation) consent = true },
                enabled = !state.active && permissionTicket == null, label = { Text(stringResource(R.string.ui_conversation_mode)) })
        }
        if (conversation && !hasHub) Text(stringResource(R.string.ui_voice_needs_hub), style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
        if (conversation && hasDraft) Text(stringResource(R.string.ui_voice_keeps_draft), style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
        when {
            speechActive && conversation -> Button(onClick = model::stopVoicePlayback, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) { Text(stringResource(R.string.ui_stop_playback)) }
            state.active -> {
                if (state.phase in setOf(DictationPhase.STARTING, DictationPhase.LISTENING)) {
                    Button(onClick = session::stop, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) { Text(stringResource(R.string.dictation_finish)) }
                }
                OutlinedButton(onClick = { if (conversation) model.stopVoiceTurn() else session.cancel() }) { Text(stringResource(R.string.dictation_cancel)) }
            }
            else -> Button(enabled = canStart, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp), onClick = {
                val tag = if (language == "system") Locale.getDefault().toLanguageTag() else language
                val ticket = if (conversation) model.startVoiceTurn(tag) else model.dictation.requestStart(tag)
                if (ticket != null) {
                    permissionForVoice = conversation; permissionTicket = ticket
                    try { launcher.launch(Manifest.permission.RECORD_AUDIO) }
                    catch (_: Exception) { permissionTicket = null; session.cancel() }
                }
            }) { Text(stringResource(if (conversation) R.string.ui_speak else R.string.dictation_start)) }
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = { model.dictation.cancel(); model.stopVoiceTurn(); onWrite() }, modifier = Modifier.testTag("voice_write")) { Text(stringResource(R.string.ui_write)) }
            TextButton(onClick = onJournal) { Text(stringResource(R.string.journal)) }
            TextButton(onClick = onDevice) { Text(stringResource(R.string.ui_device)) }
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.Center) {
            Box {
                TextButton(onClick = { menu = true }, enabled = !state.active && !speechActive && permissionTicket == null) {
                    Text(stringResource(when (language) { "fr-FR" -> R.string.dictation_french; "en-US" -> R.string.dictation_english; else -> R.string.dictation_system_language }), style = MaterialTheme.typography.labelMedium)
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    listOf("system" to R.string.dictation_system_language, "fr-FR" to R.string.dictation_french, "en-US" to R.string.dictation_english).forEach { (tag, label) ->
                        DropdownMenuItem(text = { Text(stringResource(label)) }, onClick = { language = tag; menu = false })
                    }
                }
            }
            TextButton(onClick = { details = true }) { Text(stringResource(R.string.ui_voice_details), style = MaterialTheme.typography.labelMedium) }
        }
    }
    if (consent) AlertDialog(onDismissRequest = { consent = false },
        title = { Text(stringResource(R.string.ui_voice_consent_title)) }, text = { Text(stringResource(R.string.ui_voice_consent), modifier = Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState())) },
        confirmButton = { TextButton(onClick = { consent = false; model.enableVoiceConversation(true) }) { Text(stringResource(R.string.ui_voice_enable)) } },
        dismissButton = { TextButton(onClick = { consent = false }) { Text(stringResource(R.string.ui_not_now)) } })
    if (details) AlertDialog(onDismissRequest = { details = false }, title = { Text(stringResource(R.string.ui_voice_details)) },
        text = { Text(stringResource(if (conversation) R.string.ui_voice_consent else R.string.dictation_explanation), modifier = Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState())) },
        confirmButton = { TextButton(onClick = { details = false }) { Text(stringResource(R.string.ui_close)) } })
}
private fun phaseResource(phase: DictationPhase): Int = when (phase) {
    DictationPhase.IDLE -> R.string.dictation_idle
    DictationPhase.PERMISSION -> R.string.dictation_permission
    DictationPhase.STARTING -> R.string.dictation_starting
    DictationPhase.LISTENING -> R.string.dictation_listening
    DictationPhase.FINISHING -> R.string.dictation_finishing
    DictationPhase.PROCESSING -> R.string.dictation_processing
    DictationPhase.FINISHED -> R.string.dictation_finished
    DictationPhase.CANCELLED -> R.string.dictation_cancelled
    DictationPhase.ERROR -> R.string.dictation_engine_error
}
private fun issueResource(issue: DictationIssue): Int = when (issue) {
    DictationIssue.ANDROID_VERSION -> R.string.dictation_android_version
    DictationIssue.ENGINE_MISSING -> R.string.dictation_engine_missing
    DictationIssue.PERMISSION_DENIED -> R.string.dictation_permission_denied
    DictationIssue.PERMISSION_REVOKED -> R.string.dictation_permission_revoked
    DictationIssue.BUSY -> R.string.dictation_busy
    DictationIssue.AUDIO -> R.string.dictation_audio_error
    DictationIssue.LANGUAGE -> R.string.dictation_language_error
    DictationIssue.NO_MATCH -> R.string.dictation_no_match
    DictationIssue.TIMEOUT -> R.string.dictation_timeout
    DictationIssue.CONTEXT_CHANGED -> R.string.dictation_context_changed
    DictationIssue.RESULT_INVALID -> R.string.dictation_result_invalid
    DictationIssue.ENGINE_FAILURE -> R.string.dictation_engine_error
}
