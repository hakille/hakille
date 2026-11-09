package dev.hakille.mobile

import dev.hakille.mobile.ui.HOLD_TO_CONFIRM_MS
import dev.hakille.mobile.ui.computeHoldProgress
import org.junit.Assert.*
import org.junit.Test

class HoldToConfirmTimingTest {

    @Test
    fun testRequiredHoldDurationIs600ms() {
        assertEquals("Hold duration must be strictly 600ms per motion.toml and Rust core", 600L, HOLD_TO_CONFIRM_MS)
    }

    @Test
    fun testProgressMathClamped() {
        assertEquals(0.0f, computeHoldProgress(0L), 1e-6f)
        assertEquals(0.25f, computeHoldProgress(150L), 1e-6f)
        assertEquals(0.5f, computeHoldProgress(300L), 1e-6f)
        assertEquals(0.75f, computeHoldProgress(450L), 1e-6f)
        assertEquals(1.0f, computeHoldProgress(600L), 1e-6f)
        // Values beyond 600ms must clamp to 1.0
        assertEquals(1.0f, computeHoldProgress(601L), 1e-6f)
        assertEquals(1.0f, computeHoldProgress(1200L), 1e-6f)
    }

    @Test
    fun testEarlyReleaseIsStrictlyIncomplete() {
        // At 599ms (1ms early), progress must not reach 1.0
        val progress599 = computeHoldProgress(599L)
        assertTrue("599ms hold must be less than 1.0", progress599 < 1.0f)
        assertEquals(599f / 600f, progress599, 1e-6f)
    }

    @Test
    fun testNegativeElapsedHandledGracefully() {
        assertEquals(0.0f, computeHoldProgress(-10L), 1e-6f)
    }
}
