package dev.hakille.mobile.ui

import android.os.SystemClock
import android.view.HapticFeedbackConstants
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.hakille.mobile.LocalHakilleDark
import hakille.tokens.HakilleTokens as T
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlin.math.abs

/**
 * 600 ms Hold-to-Confirm constant.
 * Backed by Rust sovereign state machine (crates/hakille-design/src/motion.rs).
 */
const val HOLD_TO_CONFIRM_MS: Long = 600L

/**
 * Pure helper for progress computation and clamping in tests and rendering.
 */
fun computeHoldProgress(elapsedMs: Long, totalMs: Long = HOLD_TO_CONFIRM_MS): Float {
    if (elapsedMs <= 0L) return 0.0f
    if (elapsedMs >= totalMs) return 1.0f
    return (elapsedMs.toFloat() / totalMs.toFloat()).coerceIn(0.0f, 1.0f)
}

/**
 * Tactile 600 ms Hold-to-Confirm Button.
 * Strictly reserved Braise (#FF7A2F) progress ring and halo.
 * 10 ms haptic tick at 0 ms (press start) and 600 ms (threshold reached).
 * Immediate reset and cancellation on early release (< 600 ms), drag-off (> 20dp),
 * or app backgrounding/lifecycle pause.
 * Includes accessible two-step confirmation for TalkBack / Switch Access users.
 */
@Composable
fun HoldToConfirmButton(
    onConfirmed: () -> Unit,
    onCancelled: () -> Unit = {},
    onHoldingChanged: (Boolean) -> Unit = {},
    label: String = "Maintenir 600 ms pour valider",
    confirmedLabel: String = "Action validée",
    enabled: Boolean = true,
    modifier: Modifier = Modifier
) {
    val isDark = LocalHakilleDark.current
    val view = LocalView.current
    val scope = rememberCoroutineScope()
    val lifecycleOwner = LocalLifecycleOwner.current

    // Strictly authorized Braise tokens for validation & hold progress ring
    val braiseColor = if (isDark) T.colorBraiseDark else T.colorBraiseLight
    val braiseHalo = if (isDark) T.colorBraiseHaloDark else T.colorBraiseHaloLight
    val onBraiseColor = if (isDark) T.colorSurBraiseDark else T.colorSurBraiseLight

    var isHolding by remember { mutableStateOf(false) }
    var isConfirmed by remember { mutableStateOf(false) }
    var accessibleArmed by remember { mutableStateOf(false) }
    var progress by remember { mutableFloatStateOf(0.0f) }
    var holdJob by remember { mutableStateOf<Job?>(null) }

    fun cancelHold() {
        if (isHolding) {
            isHolding = false
            onHoldingChanged(false)
            progress = 0.0f
            holdJob?.cancel()
            holdJob = null
            accessibleArmed = false
            onCancelled()
        }
    }

    // Abort hold on lifecycle pause (e.g. app backgrounding, incoming call)
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_PAUSE || event == Lifecycle.Event.ON_STOP) {
                cancelHold()
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
            holdJob?.cancel()
        }
    }

    val accessibilityDesc = when {
        isConfirmed -> confirmedLabel
        accessibleArmed -> "Confirmation armée. Toucher pour valider définitivement."
        else -> "Validation sécurisée en 2 étapes pour lecteur d'écran, ou maintien continu de 600 millisecondes."
    }

    val cornerRadius = T.layoutRadiusControl.toInt().dp
    val buttonShape = RoundedCornerShape(cornerRadius)

    Box(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp) // Minimum 48dp strict target compliant (56dp standard)
            .clip(buttonShape)
            .background(if (isHolding) braiseHalo else Color.Transparent)
            .semantics {
                role = Role.Button
                contentDescription = accessibilityDesc
                onClick {
                    if (!enabled || isConfirmed) return@onClick false
                    if (!accessibleArmed) {
                        accessibleArmed = true
                        view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    } else {
                        accessibleArmed = false
                        isConfirmed = true
                        progress = 1.0f
                        view.performHapticFeedback(HapticFeedbackConstants.CONFIRM)
                        onConfirmed()
                    }
                    true
                }
            }
            .pointerInput(enabled, isConfirmed) {
                if (!enabled || isConfirmed) return@pointerInput
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = true)
                    val startTime = SystemClock.elapsedRealtime()
                    isHolding = true
                    onHoldingChanged(true)
                    progress = 0.0f

                    // 10 ms haptic tick at 0 ms (press start)
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)

                    var completed = false
                    val job = scope.launch {
                        while (isActive) {
                            val elapsed = SystemClock.elapsedRealtime() - startTime
                            progress = computeHoldProgress(elapsed)
                            if (elapsed >= HOLD_TO_CONFIRM_MS) {
                                completed = true
                                isConfirmed = true
                                isHolding = false
                                onHoldingChanged(false)
                                // 10 ms haptic tick at 600 ms (threshold confirmed)
                                view.performHapticFeedback(HapticFeedbackConstants.CONFIRM)
                                onConfirmed()
                                break
                            }
                            delay(16L) // ~60 FPS sampling
                        }
                    }
                    holdJob = job

                    // Monitor touch movement and release
                    var pointerCancelled = false
                    var totalDragX = 0f
                    var totalDragY = 0f
                    val dragThreshold = 20.dp.toPx()

                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id }
                        if (change == null || !change.pressed) {
                            // Touch released
                            break
                        }
                        val posChange = change.positionChange()
                        totalDragX += posChange.x
                        totalDragY += posChange.y
                        if (abs(totalDragX) > dragThreshold || abs(totalDragY) > dragThreshold) {
                            // Dragged out of boundary (> 20dp) -> consume and cancel immediately
                            change.consume()
                            pointerCancelled = true
                            break
                        }
                    }

                    job.cancel()
                    holdJob = null

                    if (!completed) {
                        // Released before 600 ms or dragged off -> cancel immediately
                        isHolding = false
                        onHoldingChanged(false)
                        progress = 0.0f
                        onCancelled()
                    }

                    // Prevent re-arming while the displaced finger is still pressed
                    if (pointerCancelled) {
                        waitForUpOrCancellation()
                    }
                }
            },
        contentAlignment = Alignment.Center
    ) {
        // Progress Ring & Border Canvas
        Canvas(modifier = Modifier.fillMaxSize()) {
            val strokeWidth = 2.dp.toPx()
            val baseLineColor = if (isDark) T.colorLigneDark else T.colorLigneLight

            // Draw base resting/subtle border
            drawRoundRect(
                color = if (isHolding) braiseColor.copy(alpha = 0.4f) else baseLineColor,
                topLeft = Offset(strokeWidth / 2f, strokeWidth / 2f),
                size = Size(size.width - strokeWidth, size.height - strokeWidth),
                cornerRadius = androidx.compose.ui.geometry.CornerRadius(cornerRadius.toPx()),
                style = Stroke(width = strokeWidth)
            )

            // Draw progressive Braise (#FF7A2F) perimeter when holding
            if (progress > 0.0f) {
                val perimeter = 2 * (size.width + size.height)
                val drawnDistance = perimeter * progress
                // Draw bottom and perimeter progressive line in Braise
                drawLine(
                    color = braiseColor,
                    start = Offset(0f, size.height - strokeWidth / 2f),
                    end = Offset(size.width * progress, size.height - strokeWidth / 2f),
                    strokeWidth = strokeWidth * 2f,
                    cap = StrokeCap.Round
                )
            }
        }

        Row(
            modifier = Modifier.padding(horizontal = 24.dp, vertical = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            // Circular Braise progress indicator
            if (isHolding || progress > 0.0f) {
                Canvas(modifier = Modifier.size(20.dp)) {
                    val radius = (size.minDimension / 2f) - 2.dp.toPx()
                    drawCircle(
                        color = braiseColor.copy(alpha = 0.3f),
                        radius = radius,
                        style = Stroke(width = 2.dp.toPx())
                    )
                    drawArc(
                        color = braiseColor,
                        startAngle = -90f,
                        sweepAngle = 360f * progress,
                        useCenter = false,
                        style = Stroke(width = 2.5f.toPx(), cap = StrokeCap.Round)
                    )
                }
            }

            Text(
                text = when {
                    isConfirmed -> confirmedLabel
                    accessibleArmed -> "Toucher pour valider (2/2)"
                    isHolding -> "Maintien : ${(progress * 100).toInt()}%"
                    else -> label
                },
                style = MaterialTheme.typography.titleSmall,
                color = if (isHolding) braiseColor else MaterialTheme.colorScheme.onSurface
            )
        }
    }
}
