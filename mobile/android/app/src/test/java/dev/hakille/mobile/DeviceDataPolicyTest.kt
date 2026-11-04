package dev.hakille.mobile

import java.io.ByteArrayInputStream
import java.io.IOException
import java.io.InputStream
import org.junit.Assert.*
import org.junit.Test

/** Vérifie les données non fiables ; aucun capteur ou fournisseur réel simulé comme testé. */
class DeviceDataPolicyTest {
    @Test fun olderAndroidRejectsStreamingProvidersButKeepsRegularFiles() {
        for (api in 26..29) {
            requireSupportedDocumentDescriptor(api, regularFile = true)
            try {
                requireSupportedDocumentDescriptor(api, regularFile = false)
                fail("Flux accepté sans API de mode non bloquant publique")
            } catch (_: UnsupportedDocumentStreamException) { }
        }
        requireSupportedDocumentDescriptor(30, regularFile = false)
        requireSupportedDocumentDescriptor(35, regularFile = true)
    }
    @Test fun documentLimitReadsActualBytesEvenWhenMetadataWouldLie() {
        val exact = ByteArray(1024 * 1024) { (it % 127).toByte() }
        assertArrayEquals(exact, readLocalDocument(ByteArrayInputStream(exact)))
        try { readLocalDocument(ByteArrayInputStream(exact + byteArrayOf(1))); fail("Dépassement accepté") }
        catch (_: IOException) { }
        assertArrayEquals(byteArrayOf(), readLocalDocument(ByteArrayInputStream(byteArrayOf())))
    }
    @Test fun shortReadsAndZeroProgressAreHandledWithoutUnboundedLoop() {
        val content = "Texte 日本語 <script>non exécuté</script>".toByteArray()
        val short = object : ByteArrayInputStream(content) {
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int = super.read(buffer, offset, minOf(length, 1))
        }
        assertArrayEquals(content, readLocalDocument(short))
        val stuck = object : InputStream() {
            override fun read() = 0
            override fun read(buffer: ByteArray, offset: Int, length: Int) = 0
        }
        try { readLocalDocument(stuck); fail("Absence de progression acceptée") } catch (_: IOException) { }
    }
    @Test fun providerFailuresRemainFailures() {
        val broken = object : InputStream() { override fun read(): Int = throw IOException("Fournisseur déconnecté") }
        try { readLocalDocument(broken); fail("Erreur cachée") } catch (_: IOException) { }
    }
    @Test fun locationRejectsOldFutureNonFiniteAndImpossibleMeasurements() {
        assertTrue(validDeviceLocation(48.8, 2.3, 100f, 60_000))
        assertTrue(validDeviceLocation(-90.0, 180.0, 0f, 0))
        for (age in listOf(-1L, 60_001L)) assertFalse(validDeviceLocation(48.8, 2.3, 100f, age))
        for (latitude in listOf(Double.NaN, Double.POSITIVE_INFINITY, 91.0)) assertFalse(validDeviceLocation(latitude, 2.3, 100f, 0))
        assertFalse(validDeviceLocation(48.8, -181.0, 100f, 0))
        for (accuracy in listOf(Float.NaN, Float.POSITIVE_INFINITY, -1f)) assertFalse(validDeviceLocation(48.8, 2.3, accuracy, 0))
    }
}
