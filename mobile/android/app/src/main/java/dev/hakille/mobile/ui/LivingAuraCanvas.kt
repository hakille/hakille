package dev.hakille.mobile.ui

import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.hakille.mobile.LocalHakilleDark
import dev.hakille.mobile.R
import dev.hakille.mobile.tokenColor
import hakille.tokens.HakilleTokens as T
import kotlin.math.cos
import kotlin.math.sin

/**
 * Presence states conforming to crates/hakille-ui-protocol/src/lib.rs:23.
 */
enum class PresenceState {
    Idle,
    Listening,
    Acting,
    Speaking,
    DecisionRequired,
    Urgent
}

/**
 * Streaming Aura Ring update DTO from hub daemon.
 * Reflects continuous sovereign confidence tau in [0.0, 1.0].
 */
data class AuraRingUpdateDto(
    val tau: Float,
    val state_name: String,
    val primary_hex: String,
    val glow_hex: String,
    val pulse_period_ms: Long = 3000L,
    val scale_factor: Float = 1.0f,
    val timestamp_ms: Long = 0L
) {
    val isHighTrust: Boolean get() = tau >= 0.90f || state_name == "EmeraldHighTrust"
    val isAmbient: Boolean get() = (tau in 0.40f..<0.90f) || state_name == "AmberAmbient"
    val isStepUp: Boolean get() = state_name == "VioletStepUp"
    val isLocked: Boolean get() = state_name == "RedLocked"
}

/**
 * Helper to compute kora wire count based on viewport size.
 * Full: 21 wires; Small (< 24dp): 7 wires (crates/hakille-design/src/brand.rs).
 */
fun wireCountForSize(sizeDp: Dp): Int =
    if (sizeDp < 24.dp) T.presenceWireCountCompact.toInt() else T.presenceWireCountFull.toInt()

/**
 * Living Presence Aura Canvas.
 * Renders the 21 kora wires (7 compact wires below 24dp) and dynamic halo
 * reacting genuinely to PresenceState and streaming AuraRingUpdateDto.
 * Zero simulated success: No fake waveform or artificial FFT audio visualization.
 */
@Composable
fun LivingAuraCanvas(
    presenceState: PresenceState = PresenceState.Idle,
    auraRingDto: AuraRingUpdateDto? = null,
    sizeDp: Dp = 176.dp,
    modifier: Modifier = Modifier
) {
    val isDark = LocalHakilleDark.current
    val wires = wireCountForSize(sizeDp)
    val isCompact = sizeDp < 24.dp

    // Breathing pulse animation cycle: ~3000ms in idle (5x T600)
    val infiniteTransition = rememberInfiniteTransition(label = "living_aura_breath")
    val pulsePhase by infiniteTransition.animateFloat(
        initialValue = 0.0f,
        targetValue = 1.0f,
        animationSpec = infiniteRepeatable(
            animation = tween(
                durationMillis = (auraRingDto?.pulse_period_ms?.toInt() ?: 3000).coerceAtLeast(600),
                easing = LinearEasing
            ),
            repeatMode = RepeatMode.Restart
        ),
        label = "pulse_phase"
    )

    // Compute chromatic ring and halo colors
    val ringColor: Color
    val haloColor: Color
    if (auraRingDto != null) {
        ringColor = tokenColor(auraRingDto.primary_hex)
        haloColor = runCatching { tokenColor(auraRingDto.glow_hex) }.getOrDefault(ringColor.copy(alpha = 0.45f))
    } else {
        when (presenceState) {
            PresenceState.Idle -> {
                ringColor = if (isDark) T.colorLueurDark else T.colorLueurLight
                haloColor = ringColor.copy(alpha = 0.08f)
            }
            PresenceState.Listening -> {
                ringColor = if (isDark) T.colorLueurDark else T.colorLueurLight
                haloColor = ringColor.copy(alpha = 0.28f)
            }
            PresenceState.Acting -> {
                ringColor = if (isDark) T.colorSuccesDark else T.colorSuccesLight
                haloColor = ringColor.copy(alpha = 0.25f)
            }
            PresenceState.Speaking -> {
                ringColor = if (isDark) T.colorLueurDark else T.colorLueurLight
                haloColor = ringColor.copy(alpha = 0.35f)
            }
            PresenceState.DecisionRequired -> {
                // Braise is strictly authorized for decision-ring attention indicator
                ringColor = if (isDark) T.colorBraiseDark else T.colorBraiseLight
                haloColor = if (isDark) T.colorBraiseHaloDark else T.colorBraiseHaloLight
            }
            PresenceState.Urgent -> {
                ringColor = if (isDark) T.colorErreurDark else T.colorErreurLight
                haloColor = ringColor.copy(alpha = 0.35f)
            }
        }
    }

    val stateDescription = when (presenceState) {
        PresenceState.Idle -> "au repos"
        PresenceState.Listening -> "à l'écoute"
        PresenceState.Acting -> "en action"
        PresenceState.Speaking -> "parle"
        PresenceState.DecisionRequired -> "décision requise"
        PresenceState.Urgent -> "attention urgente"
    }

    Box(
        modifier = modifier
            .size(sizeDp)
            .semantics {
                contentDescription = "Présence Hakille : $stateDescription"
                liveRegion = LiveRegionMode.Polite
            },
        contentAlignment = Alignment.Center
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val center = Offset(size.width / 2f, size.height / 2f)
            val baseRadius = (size.minDimension / 2f) - 4.dp.toPx()

            // Subtle breathing modulation factor
            val breath = sin(pulsePhase * 2.0 * Math.PI).toFloat() * 0.03f
            val dynamicScale = (auraRingDto?.scale_factor ?: 1.0f) + breath
            val activeRadius = baseRadius * dynamicScale

            // Draw outer ambient halo bloom
            drawCircle(
                color = haloColor,
                radius = activeRadius + 2.dp.toPx(),
                center = center
            )

            // Draw main presence / trust ring
            val strokeWidth = when {
                isCompact -> 1.dp.toPx()
                presenceState == PresenceState.Listening || presenceState == PresenceState.Speaking -> 2.dp.toPx()
                presenceState == PresenceState.DecisionRequired -> 2.dp.toPx()
                else -> 1.dp.toPx()
            }
            drawCircle(
                color = ringColor,
                radius = activeRadius,
                center = center,
                style = Stroke(width = strokeWidth)
            )

            // Draw inner subtle hairline ring
            if (!isCompact) {
                drawCircle(
                    color = (if (isDark) T.colorLigneDark else T.colorLigneLight).copy(alpha = 0.6f),
                    radius = activeRadius - 10.dp.toPx(),
                    center = center,
                    style = Stroke(width = 1.dp.toPx())
                )
            }

            // Draw Kora Wires (21 full, 7 compact)
            // Radiating outward or positioned along bottom arc conforming to brand identity
            val wireWidthPx = T.presenceWireWidthPx.toFloat() * density
            val maxHeightPx = T.presenceWireMaxHeightPx.toFloat() * density

            if (!isCompact && sizeDp >= 48.dp) {
                val wireRadius = activeRadius - 16.dp.toPx()
                val angleStep = (2.0 * Math.PI / wires).toFloat()

                for (i in 0 until wires) {
                    val angle = i * angleStep - (Math.PI / 2.0).toFloat()
                    val wirePhase = (pulsePhase + (i.toFloat() / wires)) % 1.0f
                    val wireMod = when (presenceState) {
                        PresenceState.Speaking -> (sin(wirePhase * 2.0 * Math.PI).toFloat().coerceAtLeast(0f)) * 0.6f + 0.4f
                        PresenceState.Listening -> 0.4f + (0.2f * sin(wirePhase * 2.0 * Math.PI).toFloat())
                        PresenceState.Acting -> 0.3f + (0.3f * cos(wirePhase * 2.0 * Math.PI).toFloat())
                        else -> 0.25f // Quiet resting wire height
                    }
                    val length = (maxHeightPx * wireMod).coerceIn(4.dp.toPx(), maxHeightPx)
                    val startX = center.x + cos(angle) * (wireRadius - length)
                    val startY = center.y + sin(angle) * (wireRadius - length)
                    val endX = center.x + cos(angle) * wireRadius
                    val endY = center.y + sin(angle) * wireRadius

                    drawLine(
                        color = ringColor.copy(alpha = if (presenceState == PresenceState.Speaking) 0.85f else 0.4f),
                        start = Offset(startX, startY),
                        end = Offset(endX, endY),
                        strokeWidth = wireWidthPx,
                        cap = StrokeCap.Round
                    )
                }
            }
        }

        // Central sovereign mark graphic for standard sizes (>= 64dp)
        if (sizeDp >= 64.dp) {
            val markSize = (sizeDp * 0.77f)
            Image(
                painter = painterResource(
                    if (isDark) R.drawable.hakille_mark_dark else R.drawable.hakille_mark_light
                ),
                contentDescription = null,
                modifier = Modifier.size(markSize)
            )
        }
    }
}
