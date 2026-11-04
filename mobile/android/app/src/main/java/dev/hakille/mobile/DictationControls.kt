package dev.hakille.mobile

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.util.Locale

@Composable fun DictationControls(model: MobileModel, enabled: Boolean) {
    val state by model.dictationStatus.collectAsStateWithLifecycle()
    var language by rememberSaveable { mutableStateOf("system") }
    var menu by remember { mutableStateOf(false) }
    // Ne jamais réaffecter un ticket avant réception du résultat de permission.
    var permissionTicket by remember { mutableStateOf<Long?>(null) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        val ticket = permissionTicket
        permissionTicket = null
        if (ticket != null) model.dictation.permissionResult(ticket, granted)
    }
    DisposableEffect(model) { onDispose { model.dictation.cancel() } }
    val unavailable = model.dictation.unavailable()
    val message = state.issue ?: if (state.phase == DictationPhase.IDLE) unavailable else null
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.dictation_explanation), style = MaterialTheme.typography.bodySmall)
        Text(stringResource(if (message != null) issueResource(message) else phaseResource(state.phase)),
            color = if (state.phase == DictationPhase.ERROR) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
        Box {
            OutlinedButton(onClick = { menu = true }, enabled = !state.active && permissionTicket == null) {
                Text(stringResource(when (language) { "fr-FR" -> R.string.dictation_french; "en-US" -> R.string.dictation_english; else -> R.string.dictation_system_language }))
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                listOf("system" to R.string.dictation_system_language, "fr-FR" to R.string.dictation_french, "en-US" to R.string.dictation_english).forEach { (tag, label) ->
                    DropdownMenuItem(text = { Text(stringResource(label)) }, onClick = { language = tag; menu = false })
                }
            }
        }
        if (state.active) {
            if (state.phase in setOf(DictationPhase.STARTING, DictationPhase.LISTENING)) {
                Button(onClick = model.dictation::stop) { Text(stringResource(R.string.dictation_finish)) }
            }
            OutlinedButton(onClick = model.dictation::cancel) { Text(stringResource(R.string.dictation_cancel)) }
        } else {
            Button(enabled = enabled && unavailable == null && permissionTicket == null, onClick = {
                val tag = if (language == "system") Locale.getDefault().toLanguageTag() else language
                val ticket = model.dictation.requestStart(tag)
                if (ticket != null) {
                    permissionTicket = ticket
                    try { launcher.launch(Manifest.permission.RECORD_AUDIO) }
                    catch (_: Exception) { permissionTicket = null; model.dictation.cancel() }
                }
            }) { Text(stringResource(R.string.dictation_start)) }
        }
    }
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
