package dev.hakille.mobile

import android.graphics.Bitmap
import java.io.ByteArrayOutputStream
import java.io.InputStream
import kotlin.math.roundToInt

const val MAX_DOCUMENT_BYTES: Int = 8 * 1024 * 1024
const val MAX_PREVIEW_CHARS: Int = 4000
const val MAX_CAMERA_LONG_EDGE_PX: Int = 1536
const val MAX_CAMERA_TOTAL_PIXELS: Long = 4_000_000L
const val MAX_CAMERA_PHOTO_BYTES: Int = 8 * 1024 * 1024

/** La taille annoncée par un fournisseur SAF ne remplace jamais cette borne. */
fun readLocalDocument(input: InputStream, limit: Int = 1024 * 1024): ByteArray {
    require(limit in 1..MAX_DOCUMENT_BYTES)
    val output = ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while (true) {
        val count = input.read(buffer, 0, minOf(buffer.size, limit - output.size() + 1))
        if (count < 0) return output.toByteArray()
        if (count == 0) throw java.io.IOException("Le fournisseur ne progresse pas")
        if (output.size() + count > limit) throw java.io.IOException("Document supérieur à la limite autorisée")
        output.write(buffer, 0, count)
    }
}

fun computeCameraScale(width: Int, height: Int): Float {
    val longEdge = maxOf(width, height)
    if (longEdge <= 0) return 0f
    return minOf(1f, MAX_CAMERA_LONG_EDGE_PX.toFloat() / longEdge.toFloat())
}

fun scaledCameraDimensions(width: Int, height: Int): Pair<Int, Int>? {
    if (width <= 0 || height <= 0) return null
    if (width.toLong() * height.toLong() > MAX_CAMERA_TOTAL_PIXELS) return null
    val scale = computeCameraScale(width, height)
    val targetW = (width * scale).roundToInt().coerceAtLeast(1)
    val targetH = (height * scale).roundToInt().coerceAtLeast(1)
    return targetW to targetH
}

fun downscaleCameraBitmap(photo: Bitmap): Bitmap? {
    val dims = scaledCameraDimensions(photo.width, photo.height) ?: return null
    val (targetW, targetH) = dims
    return if (targetW == photo.width && targetH == photo.height) photo
    else Bitmap.createScaledBitmap(photo, targetW, targetH, true)
}

fun validDeviceLocation(latitude: Double, longitude: Double, accuracy: Float, ageMillis: Long): Boolean =
    latitude.isFinite() && longitude.isFinite() && latitude in -90.0..90.0 && longitude in -180.0..180.0 &&
        accuracy.isFinite() && accuracy >= 0f && accuracy <= 40_100_000f && ageMillis in 0..60_000

fun validDeviceLocation(
    latitude: Double,
    longitude: Double,
    accuracy: Float,
    ageMillis: Long,
    isMock: Boolean
): Boolean =
    !isMock &&
        latitude.isFinite() && longitude.isFinite() &&
        latitude in -90.0..90.0 && longitude in -180.0..180.0 &&
        accuracy.isFinite() && accuracy >= 0f && accuracy <= 40_100_000f &&
        ageMillis in -2_000L..15_000L

enum class AllowedDeviceApp { SETTINGS, CLOCK, CALENDAR, MAPS, DOCUMENTATION }

class UnsupportedDocumentStreamException : java.io.IOException("Ce fournisseur de flux nécessite Android 11 ou supérieur")

/** Avant API 30, aucun fcntl avec argument n'est exposé par le SDK public. */
fun requireSupportedDocumentDescriptor(apiLevel: Int, regularFile: Boolean) {
    if (apiLevel < 30 && !regularFile) throw UnsupportedDocumentStreamException()
}
