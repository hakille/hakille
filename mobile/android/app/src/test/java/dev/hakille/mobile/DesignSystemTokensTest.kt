package dev.hakille.mobile

import dev.hakille.mobile.ui.wireCountForSize
import hakille.tokens.HakilleTokens as T
import androidx.compose.ui.unit.dp
import org.junit.Assert.*
import org.junit.Test

class DesignSystemTokensTest {

    @Test
    fun testOledSurfaceHexValues() {
        // Pure Obsidian OLED surfaces conforming to color.toml
        assertEquals("#0B0C0F", T.colorSurface0DarkHex)
        assertEquals("#F6F4F0", T.colorSurface0LightHex)
        assertEquals("#121419", T.colorSurface1DarkHex)
        assertEquals("#FFFFFF", T.colorSurface1LightHex)
        assertEquals("#1A1D24", T.colorSurface2DarkHex)
        assertEquals("#EDEAE4", T.colorSurface2LightHex)

        // Hairline and Glow
        assertEquals("#262A33", T.colorLigneDarkHex)
        assertEquals("#DAD6CE", T.colorLigneLightHex)
        assertEquals("#F4EBDD", T.colorLueurDarkHex)
        assertEquals("#14161B", T.colorLueurLightHex)

        // Braise (#FF7A2F)
        assertEquals("#FF7A2F", T.colorBraiseDarkHex)
        assertEquals("#C2410C", T.colorBraiseLightHex)
        assertEquals("#FF7A2F47", T.colorBraiseHaloDarkHex)
    }

    @Test
    fun testWireCountsAndMotion() {
        assertEquals(21L, T.presenceWireCountFull)
        assertEquals(7L, T.presenceWireCountCompact)
        assertEquals(80L, T.presenceWireDecayMs)
        assertEquals(3.0, T.presenceWireGapPx, 1e-6)
        assertEquals(2.0, T.presenceWireWidthPx, 1e-6)

        // Test wireCount helper threshold (< 24dp)
        assertEquals(7, wireCountForSize(16.dp))
        assertEquals(7, wireCountForSize(23.dp))
        assertEquals(21, wireCountForSize(24.dp))
        assertEquals(21, wireCountForSize(176.dp))
    }

    @Test
    fun testMinimumTouchTargets() {
        assertEquals("Android minimum touch target must be 48dp", 48L, T.layoutTouchPtAndroid)
        assertEquals("iOS minimum touch target must be 44pt", 44L, T.layoutTouchPtIos)
    }

    @Test
    fun testHapticDuration() {
        assertEquals("Haptic tick must be 10ms", 10L, T.motionHapticMsTick)
    }

    @Test
    fun testGlassmorphicBlurTokens() {
        assertEquals(30L, T.blurHudAcrylic)
        assertEquals(24L, T.blurPanel)
        assertEquals(16L, T.blurGlow)
    }

    @Test
    fun testColorTokensValidColorSpace() {
        // Calling .copy() exercises ColorSpace lookup; if color space ID is invalid this throws
        val c = T.colorBraiseDark.copy(alpha = 0.5f)
        assertNotNull(c)
        val lueur = T.colorLueurDark.copy(alpha = 0.08f)
        assertNotNull(lueur)
    }
}
