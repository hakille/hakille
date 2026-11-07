package dev.hakille.mobile

import android.Manifest
import android.annotation.SuppressLint
import android.app.Application
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.net.Uri
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.provider.AlarmClock
import android.provider.OpenableColumns
import android.provider.Settings
import androidx.core.content.ContextCompat
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.security.MessageDigest
import java.util.concurrent.atomic.AtomicReference

/** Capacités ponctuelles déclenchées par leurs boutons ; aucun lien vers le transport hub. */
data class LocalDocument(val name: String, val mime: String, val size: Int, val sha256: String, val preview: String?)
data class DevicePosition(val latitude: Double, val longitude: Double, val accuracy: Float, val measuredAtNanos: Long, val mock: Boolean)
data class DeviceCapabilitiesState(val busy: Boolean = false, val message: String = "", val photo: Bitmap? = null,
    val document: LocalDocument? = null, val position: DevicePosition? = null)

class DeviceCapabilitiesController(private val app: Application, private val scope: CoroutineScope) {
    private val _state = MutableStateFlow(DeviceCapabilitiesState())
    val state = _state.asStateFlow()
    private val handler = Handler(Looper.getMainLooper())
    private var generation = 0L
    private var external: Long? = null
    private var work: Job? = null
    private var signal: CancellationSignal? = null
    private var timeout: Runnable? = null
    private var listener: LocationListener? = null
    private val readerMutex = Mutex()
    private val activeDescriptor = AtomicReference<ParcelFileDescriptor?>(null)
    private val locations = app.getSystemService(LocationManager::class.java)
    private fun message(id: Int) = app.getString(id)

    fun beginExternal(): Long {
        clear()
        val ticket = generation
        external = ticket
        _state.value = DeviceCapabilitiesState(busy = true, message = message(R.string.device_waiting))
        timeout = Runnable { if (external == ticket) { clear(); _state.value = DeviceCapabilitiesState(message = message(R.string.device_timeout)) } }
            .also { handler.postDelayed(it, 300_000) }
        return ticket
    }
    fun acceptPhoto(ticket: Long, photo: Bitmap?) {
        if (!consumeExternal(ticket)) return
        val valid = photo?.takeIf { it.width > 0 && it.height > 0 && it.width.toLong() * it.height <= 4_000_000 }
        _state.value = DeviceCapabilitiesState(photo = valid, message = message(if (valid != null) R.string.device_photo_ready else R.string.device_cancelled))
    }
    fun acceptDocument(ticket: Long, uri: Uri?) {
        if (!consumeExternal(ticket)) return
        if (uri == null || uri.scheme != "content") { _state.value = DeviceCapabilitiesState(message = message(R.string.device_cancelled)); return }
        _state.value = DeviceCapabilitiesState(busy = true, message = message(R.string.device_reading))
        val cancellation = CancellationSignal().also { signal = it }
        val deadline = SystemClock.elapsedRealtime() + 10_000
        timeout = Runnable { if (generation == ticket) { clear(); _state.value = DeviceCapabilitiesState(message = message(R.string.device_timeout)) } }
            .also { handler.postDelayed(it, 10_000) }
        work = scope.launch {
            try {
                val document = withContext(Dispatchers.IO) { readerMutex.withLock {
                    val resolver = app.contentResolver
                    var name = "document"
                    resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null, cancellation)?.use { cursor ->
                        if (cursor.moveToFirst()) {
                            val size = cursor.getColumnIndex(OpenableColumns.SIZE)
                            if (size >= 0 && !cursor.isNull(size) && cursor.getLong(size) > 1024 * 1024) throw java.io.IOException("Document supérieur à 1 Mio")
                            val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                            if (index >= 0) name = cursor.getString(index).orEmpty().take(512)
                        }
                    }
                    val mime = resolver.getType(uri).orEmpty().take(256)
                    val data = resolver.openFileDescriptor(uri, "r", cancellation)?.use { file ->
                        activeDescriptor.set(file)
                        try {
                            cancellation.throwIfCanceled()
                            readLocalDocument(PollingDocumentInputStream(file, cancellation, deadline))
                        } finally { activeDescriptor.compareAndSet(file, null) }
                    } ?: throw java.io.IOException("Document illisible")
                    val hash = MessageDigest.getInstance("SHA-256").digest(data).joinToString("") { "%02x".format(it) }
                    val preview = if (mime.startsWith("text/") || mime == "application/json") {
                        data.take(4096).toByteArray().toString(Charsets.UTF_8).filter { !it.isISOControl() || it == '\n' || it == '\t' }
                    } else null
                    LocalDocument(name, mime, data.size, hash, preview)
                } }
                if (ticket == generation) {
                    finishWork()
                    _state.value = DeviceCapabilitiesState(document = document, message = message(R.string.device_document_ready))
                }
            } catch (_: CancellationException) { }
            catch (_: UnsupportedDocumentStreamException) {
                if (ticket == generation) { finishWork(); _state.value = DeviceCapabilitiesState(message = message(R.string.device_stream_unavailable)) }
            }
            catch (_: Exception) {
                if (ticket == generation) { finishWork(); _state.value = DeviceCapabilitiesState(message = message(R.string.device_failed)) }
            }
        }
    }
    fun externalFailed(ticket: Long) { if (consumeExternal(ticket)) _state.value = DeviceCapabilitiesState(message = message(R.string.device_unavailable)) }
    private fun consumeExternal(ticket: Long): Boolean {
        if (external != ticket || generation != ticket) return false
        external = null; timeout?.let(handler::removeCallbacks); timeout = null
        return true
    }

    @SuppressLint("MissingPermission")
    fun locate() {
        clear()
        val fine = ContextCompat.checkSelfPermission(app, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
        val coarse = ContextCompat.checkSelfPermission(app, Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
        if (!fine && !coarse) { _state.value = DeviceCapabilitiesState(message = message(R.string.device_permission_denied)); return }
        val ticket = generation
        try {
            val enabled = locations?.getProviders(true).orEmpty()
            val providers = listOf(LocationManager.NETWORK_PROVIDER, LocationManager.GPS_PROVIDER)
                .filter { it in enabled && (it != LocationManager.GPS_PROVIDER || fine) }
            if (providers.isEmpty()) { _state.value = DeviceCapabilitiesState(message = message(R.string.device_location_unavailable)); return }
            _state.value = DeviceCapabilitiesState(busy = true, message = message(R.string.device_locating))
            val callback = object : LocationListener {
                override fun onLocationChanged(location: Location) {
                    if (ticket != generation) return
                    val elapsed = SystemClock.elapsedRealtimeNanos() - location.elapsedRealtimeNanos
                    if (elapsed < 0) return
                    val age = elapsed / 1_000_000
                    if (!location.hasAccuracy() || !validDeviceLocation(location.latitude, location.longitude, location.accuracy, age)) return
                    @Suppress("DEPRECATION") val mocked = location.isFromMockProvider
                    generation++; finishWork()
                    _state.value = DeviceCapabilitiesState(position = DevicePosition(location.latitude, location.longitude, location.accuracy,
                        location.elapsedRealtimeNanos, mocked), message = message(R.string.device_location_ready))
                }
                override fun onProviderEnabled(provider: String) { }
                override fun onProviderDisabled(provider: String) { }
                @Deprecated("Callback Android historique") override fun onStatusChanged(provider: String?, status: Int, extras: android.os.Bundle?) { }
            }
            listener = callback
            timeout = Runnable { if (ticket == generation) { clear(); _state.value = DeviceCapabilitiesState(message = message(R.string.device_timeout)) } }
                .also { handler.postDelayed(it, 20_000) }
            providers.forEach { locations?.requestLocationUpdates(it, 0L, 0f, callback, Looper.getMainLooper()) }
        } catch (_: SecurityException) { clear(); _state.value = DeviceCapabilitiesState(message = message(R.string.device_permission_denied)) }
        catch (_: Exception) { clear(); _state.value = DeviceCapabilitiesState(message = message(R.string.device_failed)) }
    }
    fun permissionDenied() { clear(); _state.value = DeviceCapabilitiesState(message = message(R.string.device_permission_denied)) }
    fun hasLocationPermission(): Boolean = ContextCompat.checkSelfPermission(app, Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED ||
        ContextCompat.checkSelfPermission(app, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
    fun beginLocationPermission(): Long { clear(); return generation }
    fun locationPermissionResult(ticket: Long, granted: Boolean) {
        if (ticket != generation) return
        _state.value = DeviceCapabilitiesState(message = message(if (granted && hasLocationPermission()) R.string.device_permission_ready else R.string.device_permission_denied))
    }
    fun openApp(context: Context, allowed: AllowedDeviceApp) {
        val intent = when (allowed) {
            AllowedDeviceApp.SETTINGS -> Intent(Settings.ACTION_SETTINGS)
            AllowedDeviceApp.CLOCK -> Intent(AlarmClock.ACTION_SHOW_ALARMS)
            AllowedDeviceApp.CALENDAR -> Intent.makeMainSelectorActivity(Intent.ACTION_MAIN, Intent.CATEGORY_APP_CALENDAR)
            AllowedDeviceApp.MAPS -> Intent.makeMainSelectorActivity(Intent.ACTION_MAIN, Intent.CATEGORY_APP_MAPS)
            AllowedDeviceApp.DOCUMENTATION -> Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/hakille/hakille"))
                .addCategory(Intent.CATEGORY_BROWSABLE)
        }
        try {
            @Suppress("DEPRECATION") val resolved = context.packageManager.resolveActivity(intent, PackageManager.MATCH_DEFAULT_ONLY)?.activityInfo
                ?: throw IllegalStateException("Application absente")
            if (!resolved.exported || resolved.packageName == context.packageName) throw IllegalStateException("Application refusée")
            intent.selector = null
            intent.component = ComponentName(resolved.packageName, resolved.name)
            context.startActivity(intent)
            _state.value = _state.value.copy(message = message(R.string.device_app_opened))
        } catch (_: Exception) { _state.value = _state.value.copy(message = message(R.string.device_unavailable)) }
    }
    /** Le sélecteur/caméra externe peut répondre après la pause ; pas les travaux capteurs. */
    fun pause() {
        if (external == null) clear()
        else _state.value = DeviceCapabilitiesState(busy = true, message = message(R.string.device_waiting))
    }
    fun clear() {
        generation++; external = null; work?.cancel(); work = null
        finishWork()
        _state.value = DeviceCapabilitiesState()
    }
    private fun finishWork() {
        timeout?.let(handler::removeCallbacks); timeout = null
        signal?.cancel(); signal = null
        activeDescriptor.getAndSet(null)?.let { try { it.close() } catch (_: Exception) { } }
        listener?.let { try { locations?.removeUpdates(it) } catch (_: Exception) { } }; listener = null
    }
}
