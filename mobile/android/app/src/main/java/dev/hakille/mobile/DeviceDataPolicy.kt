package dev.hakille.mobile

import java.io.InputStream
import java.io.ByteArrayOutputStream

/** La taille annoncée par un fournisseur SAF ne remplace jamais cette borne. */
fun readLocalDocument(input: InputStream, limit: Int = 1024 * 1024): ByteArray {
    require(limit in 1..1024 * 1024)
    val output = ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while (true) {
        val count = input.read(buffer, 0, minOf(buffer.size, limit - output.size() + 1))
        if (count < 0) return output.toByteArray()
        if (count == 0) throw java.io.IOException("Le fournisseur ne progresse pas")
        if (output.size() + count > limit) throw java.io.IOException("Document supérieur à 1 Mio")
        output.write(buffer, 0, count)
    }
}
fun validDeviceLocation(latitude: Double, longitude: Double, accuracy: Float, ageMillis: Long): Boolean =
    latitude.isFinite() && longitude.isFinite() && latitude in -90.0..90.0 && longitude in -180.0..180.0 &&
        accuracy.isFinite() && accuracy >= 0 && ageMillis in 0..60_000

enum class AllowedDeviceApp { SETTINGS, CLOCK, CALENDAR, MAPS, DOCUMENTATION }

class UnsupportedDocumentStreamException : java.io.IOException("Ce fournisseur de flux nécessite Android 11 ou supérieur")

/** Avant API 30, aucun fcntl avec argument n'est exposé par le SDK public. */
fun requireSupportedDocumentDescriptor(apiLevel: Int, regularFile: Boolean) {
    if (apiLevel < 30 && !regularFile) throw UnsupportedDocumentStreamException()
}
