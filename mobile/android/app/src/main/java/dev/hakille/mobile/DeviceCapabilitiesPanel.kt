package dev.hakille.mobile

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.SystemClock
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Alignment
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay

/** Le fournisseur sélectionné doit être local ; aucun droit persistant conservé. */
private class LocalDocumentContract : ActivityResultContracts.OpenDocument() {
    override fun createIntent(context: Context, input: Array<String>): Intent =
        super.createIntent(context, input).putExtra(Intent.EXTRA_LOCAL_ONLY, true)
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable fun DeviceCapabilitiesPanel(model: MobileModel) {
    val controller = model.devices
    val state by controller.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var photoTicket by remember { mutableStateOf<Long?>(null) }
    var fileTicket by remember { mutableStateOf<Long?>(null) }
    var permissionTicket by remember { mutableStateOf<Long?>(null) }
    var appConfirmation by remember { mutableStateOf<AllowedDeviceApp?>(null) }
    val photo = rememberLauncherForActivityResult(ActivityResultContracts.TakePicturePreview()) { bitmap ->
        photoTicket?.let { controller.acceptPhoto(it, bitmap) }; photoTicket = null
    }
    val document = rememberLauncherForActivityResult(LocalDocumentContract()) { uri ->
        fileTicket?.let { controller.acceptDocument(it, uri) }; fileTicket = null
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { granted ->
        // Aucun démarrage automatique après une boîte de permission ayant mis l'app en pause.
        val ticket = permissionTicket; permissionTicket = null
        if (ticket != null) controller.locationPermissionResult(ticket, granted.values.any { it })
    }
    DisposableEffect(controller) { onDispose { controller.clear() } }
    val canLaunch = !state.busy && photoTicket == null && fileTicket == null && permissionTicket == null
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(stringResource(R.string.ui_phone_local), style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (state.message.isNotEmpty()) StateNotice(state.message)
        QuietPanel {
            CapabilityTitle(stringResource(R.string.device_photo_title), stringResource(R.string.ui_photo_summary))
            Button(modifier = Modifier.testTag("device_photo_preview"), enabled = canLaunch, onClick = {
                val ticket = controller.beginExternal(); photoTicket = ticket
                try { photo.launch(null) } catch (_: Exception) { photoTicket = null; controller.externalFailed(ticket) }
            }) { Text(stringResource(R.string.device_photo_action)) }
            state.photo?.let { bitmap ->
                Image(bitmap.asImageBitmap(), stringResource(R.string.device_photo_preview), Modifier.fillMaxWidth().heightIn(max = 240.dp))
                Text(stringResource(R.string.device_photo_dimensions, bitmap.width, bitmap.height), style = MaterialTheme.typography.bodySmall)
            }
            CapabilityDetails { Text(stringResource(R.string.device_photo_note), style = MaterialTheme.typography.bodySmall) }
        }
        QuietPanel {
            CapabilityTitle(stringResource(R.string.device_file_title), stringResource(R.string.ui_file_summary))
            Button(modifier = Modifier.testTag("device_file_choose"), enabled = canLaunch, onClick = {
                val ticket = controller.beginExternal(); fileTicket = ticket
                try { document.launch(arrayOf("*/*")) } catch (_: Exception) { fileTicket = null; controller.externalFailed(ticket) }
            }) { Text(stringResource(R.string.device_file_action)) }
            state.document?.let { file ->
                SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(file.name, style = MaterialTheme.typography.titleMedium)
                    Text(stringResource(R.string.device_file_metadata, file.mime, file.size), style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                    file.preview?.let {
                        Text(stringResource(R.string.device_file_preview), style = MaterialTheme.typography.labelMedium)
                        Text(it, modifier = Modifier.heightIn(max = 240.dp).verticalScroll(rememberScrollState()))
                    }
                } }
            }
            CapabilityDetails {
                Text(stringResource(R.string.device_file_note), style = MaterialTheme.typography.bodySmall)
                state.document?.let { file -> SelectionContainer { Text("SHA-256: ${file.sha256}",
                    style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace)) } }
            }
        }
        QuietPanel {
            CapabilityTitle(stringResource(R.string.device_location_title), stringResource(R.string.ui_location_summary))
            Button(modifier = Modifier.testTag("device_location_once"), enabled = canLaunch, onClick = {
                if (controller.hasLocationPermission()) controller.locate()
                else {
                    val ticket = controller.beginLocationPermission(); permissionTicket = ticket
                    try { permission.launch(arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION, Manifest.permission.ACCESS_FINE_LOCATION)) }
                    catch (_: Exception) { permissionTicket = null; controller.permissionDenied() }
                }
            }) { Text(stringResource(R.string.device_location_action)) }
            state.position?.let { position ->
                var age by remember(position) { mutableLongStateOf(0) }
                LaunchedEffect(position) { while (true) { age = ((SystemClock.elapsedRealtimeNanos() - position.measuredAtNanos) / 1_000_000_000).coerceAtLeast(0); delay(1000) } }
                SelectionContainer { Text(stringResource(R.string.device_location_result, position.latitude, position.longitude, position.accuracy, age)) }
                if (position.mock) Text(stringResource(R.string.device_location_mock), color = MaterialTheme.colorScheme.error)
            }
            CapabilityDetails { Text(stringResource(R.string.device_location_note), style = MaterialTheme.typography.bodySmall) }
        }
        QuietPanel {
            CapabilityTitle(stringResource(R.string.device_apps_title), stringResource(R.string.ui_apps_summary))
            listOf(AllowedDeviceApp.SETTINGS to R.string.device_settings, AllowedDeviceApp.CLOCK to R.string.device_clock,
                AllowedDeviceApp.CALENDAR to R.string.device_calendar, AllowedDeviceApp.MAPS to R.string.device_maps,
                AllowedDeviceApp.DOCUMENTATION to R.string.device_documentation).forEach { (target, title) ->
                OutlinedButton(modifier = Modifier.fillMaxWidth().testTag("device_open_${target.name.lowercase()}"), enabled = canLaunch,
                    onClick = { appConfirmation = target }) { Text(stringResource(title)) }
            }
            CapabilityDetails { Text(stringResource(R.string.device_apps_note), style = MaterialTheme.typography.bodySmall) }
        }
        TextButton(onClick = controller::clear, modifier = Modifier.align(Alignment.Start)) { Text(stringResource(R.string.device_clear)) }
    }
    appConfirmation?.let { target ->
        AlertDialog(modifier = Modifier.semantics { testTagsAsResourceId = true }, onDismissRequest = { appConfirmation = null },
            title = { Text(stringResource(when (target) { AllowedDeviceApp.SETTINGS -> R.string.device_settings; AllowedDeviceApp.CLOCK -> R.string.device_clock; AllowedDeviceApp.CALENDAR -> R.string.device_calendar; AllowedDeviceApp.MAPS -> R.string.device_maps; AllowedDeviceApp.DOCUMENTATION -> R.string.device_documentation })) },
            text = { Text(stringResource(when (target) { AllowedDeviceApp.MAPS -> R.string.device_maps_confirmation; AllowedDeviceApp.DOCUMENTATION -> R.string.device_documentation_confirmation; else -> R.string.device_app_confirmation }), modifier = Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState())) },
            confirmButton = { TextButton(modifier = Modifier.testTag("device_app_confirm"), onClick = {
                appConfirmation = null; controller.openApp(context, target)
            }) { Text(stringResource(R.string.device_confirm)) } },
            dismissButton = { TextButton(modifier = Modifier.testTag("device_app_cancel"), onClick = { appConfirmation = null }) {
                Text(stringResource(R.string.device_cancel))
            } })
    }
}

@Composable private fun CapabilityTitle(title: String, summary: String) {
    Text(title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
    Text(summary, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable private fun CapabilityDetails(content: @Composable ColumnScope.() -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        TextButton(onClick = { expanded = !expanded }) {
            Text(stringResource(if (expanded) R.string.ui_hide_details else R.string.ui_capability_details), style = MaterialTheme.typography.labelMedium)
        }
        if (expanded) Column(verticalArrangement = Arrangement.spacedBy(8.dp), content = content)
    }
}
