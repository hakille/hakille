package dev.hakille.mobile

import dev.hakille.mobile.ui.GuestPassHelper
import dev.hakille.mobile.ui.PrivacyGuardDebounceFilter
import dev.hakille.mobile.ui.PrivacyGuardHelper
import java.io.ByteArrayInputStream
import java.io.IOException
import org.junit.Assert.*
import org.junit.Test

class PairingPrivacyAndGuestTest {

    @Test
    fun testQrPairingParsesValid64HexAndRejectsInvalidSchemesOrTokens() {
        val validHex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        val qrJson = """{"endpoint":"https://Hub.Example.COM:8443/","token":"$validHex"}"""
        val creds = EndpointPolicy.parsePairingQr(qrJson)
        assertEquals("https://hub.example.com:8443", creds.endpoint)
        assertEquals(validHex, creds.token)
        assertEquals("HubCredentials(redacted)", creds.toString())

        // Reject 63-char hex
        val shortHex = validHex.take(63)
        assertTrue(runCatching {
            EndpointPolicy.parsePairingQr("""{"endpoint":"https://hub.example.com","token":"$shortHex"}""")
        }.isFailure)

        // Reject non-hex
        val badHex = "g" + validHex.drop(1)
        assertTrue(runCatching {
            EndpointPolicy.parsePairingQr("""{"endpoint":"https://hub.example.com","token":"$badHex"}""")
        }.isFailure)

        // Reject HTTP on non-loopback
        assertTrue(runCatching {
            EndpointPolicy.parsePairingQr("""{"endpoint":"http://192.168.1.50:8080","token":"$validHex"}""")
        }.isFailure)
    }

    @Test
    fun testCameraScaleAndDimensionBounds1536Px() {
        assertEquals(1536, MAX_CAMERA_LONG_EDGE_PX)
        assertEquals(8 * 1024 * 1024, MAX_CAMERA_PHOTO_BYTES)
        assertEquals(4_000_000L, MAX_CAMERA_TOTAL_PIXELS)

        assertEquals(1536 to 1152, scaledCameraDimensions(2000, 1500))
        assertEquals(1024 to 768, scaledCameraDimensions(1024, 768))
        assertNull(scaledCameraDimensions(0, 100))
        assertNull(scaledCameraDimensions(3000, 2000)) // 6 MP > 4 MP
    }

    @Test
    fun testDocumentBound8MiBAndLocationFreshness15sAndMockRejection() {
        assertEquals(8 * 1024 * 1024, MAX_DOCUMENT_BYTES)
        assertEquals(4000, MAX_PREVIEW_CHARS)

        val twoMegabytes = ByteArray(2 * 1024 * 1024) { 0x41 }
        assertEquals(
            twoMegabytes.size,
            readLocalDocument(ByteArrayInputStream(twoMegabytes), MAX_DOCUMENT_BYTES).size
        )

        // Fresh non-mock GPS fix (3s old) accepted
        assertTrue(validDeviceLocation(48.8566, 2.3522, 12f, 3_000L, isMock = false))
        // Stale GPS fix (16s old) rejected
        assertFalse(validDeviceLocation(48.8566, 2.3522, 12f, 16_000L, isMock = false))
        // Mock provider GPS fix rejected
        assertFalse(validDeviceLocation(48.8566, 2.3522, 5f, 1_000L, isMock = true))
    }

    @Test
    fun testPrivacyGuardShoulderSurfingThresholdAndDebouncing() {
        assertFalse(PrivacyGuardHelper.shouldTriggerVeil(0))
        assertFalse(PrivacyGuardHelper.shouldTriggerVeil(1))
        assertTrue(PrivacyGuardHelper.shouldTriggerVeil(2))

        val filter = PrivacyGuardDebounceFilter(requiredConsecutive = 3)
        assertFalse(filter.update(1))
        assertFalse(filter.update(2))
        assertFalse(filter.update(1))
        assertFalse(filter.update(2))
        assertFalse(filter.update(2))
        assertTrue(filter.update(2))
    }

    @Test
    fun testGuestPassRingFraction1PxInvariantAndCapabilityConfinement() {
        assertEquals(1, GuestPassHelper.GUEST_RING_PX)
        assertEquals(1.0f, GuestPassHelper.computeRingFraction(0, 3600), 1e-6f)
        assertEquals(0.5f, GuestPassHelper.computeRingFraction(1800, 3600), 1e-6f)
        assertEquals(0.0f, GuestPassHelper.computeRingFraction(3600, 3600), 1e-6f)
        assertTrue(GuestPassHelper.isExpired(3600, 3600))
        assertTrue(GuestPassHelper.isExpired(0, 0))

        assertTrue(GuestPassHelper.isAuthorizedForGuest("wifi.connect", expired = false, "wifi.connect"))
        assertFalse(GuestPassHelper.isAuthorizedForGuest("wifi.connect", expired = false, "wallet.spend"))
        assertFalse(GuestPassHelper.isAuthorizedForGuest("wifi.connect", expired = false, "memory.forget"))
        assertFalse(GuestPassHelper.isAuthorizedForGuest("wifi.connect", expired = true, "wifi.connect"))
    }
}
