package dev.hakille.mobile.ui

import androidx.compose.animation.*
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import hakille.tokens.HakilleTokens as T
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlinx.serialization.json.*

/**
 * Ghost Action Projection:
 * Translucent anticipatory suggestion at focal point,
 * dismissible on swipe/look-away, confirmable on action.
 */
@Composable
fun GhostActionComponent(
    ghost: JsonObject?,
    onAccept: (String) -> Unit,
    onDismiss: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    if (ghost == null) return
    val id = ghost["id"]?.jsonPrimitive?.contentOrNull.orEmpty()
    val label = ghost["label"]?.jsonPrimitive?.contentOrNull.orEmpty()
    val confidence = ((ghost["confidence"]?.jsonPrimitive?.floatOrNull ?: 0.9f) * 100f).coerceIn(0f, 100f)

    if (id.isEmpty()) return

    var offsetX by remember(id) { mutableFloatStateOf(0f) }

    AnimatedVisibility(
        visible = id.isNotEmpty(),
        enter = fadeIn() + slideInVertically { it / 2 },
        exit = fadeOut() + slideOutHorizontally()
    ) {
        Card(
            modifier = modifier
                .fillMaxWidth()
                .offset { IntOffset(offsetX.roundToInt(), 0) }
                .draggable(
                    state = rememberDraggableState { delta ->
                        offsetX += delta
                    },
                    orientation = Orientation.Horizontal,
                    onDragStopped = {
                        if (abs(offsetX) > 200f) {
                            onDismiss(id)
                        } else {
                            offsetX = 0f
                        }
                    }
                )
                .testTag("ghost_action_card"),
            colors = CardDefaults.cardColors(
                containerColor = T.colorSurface1Dark.copy(alpha = 0.92f)
            ),
            border = BorderStroke(1.dp, T.colorBraiseDark.copy(alpha = 0.6f)),
            shape = RoundedCornerShape(12.dp)
        ) {
            Row(
                modifier = Modifier.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "Suggestion anticipée (${confidence.toInt()}%)",
                        style = MaterialTheme.typography.labelSmall,
                        color = T.colorBraiseDark
                    )
                    Text(
                        text = label,
                        style = MaterialTheme.typography.titleMedium,
                        color = T.colorTexte1Dark
                    )
                    Text(
                        text = "Glisser pour masquer • Appuyer pour valider",
                        style = MaterialTheme.typography.bodySmall,
                        color = T.colorTexte2Dark
                    )
                }
                Button(
                    onClick = { onAccept(id) },
                    colors = ButtonDefaults.buttonColors(
                        containerColor = T.colorBraiseDark,
                        contentColor = T.colorSurBraiseDark
                    ),
                    modifier = Modifier.testTag("ghost_action_accept")
                ) {
                    Text("Valider")
                }
            }
        }
    }
}

/**
 * Ergonomic Fatigue Alert Toast / Banner:
 * Gentle well-being recommendation toast for posture, gaze strain, or breaks.
 */
@Composable
fun FatigueAlertComponent(
    recommendation: String,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth().testTag("fatigue_alert_banner"),
        colors = CardDefaults.cardColors(containerColor = T.colorSurface1Dark),
        border = BorderStroke(1.dp, T.colorLueurDark),
        shape = RoundedCornerShape(12.dp)
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = "Bien-être & Ergonomie",
                    style = MaterialTheme.typography.labelSmall,
                    color = T.colorLueurDark
                )
                Text(
                    text = recommendation,
                    style = MaterialTheme.typography.bodyMedium,
                    color = T.colorTexte1Dark
                )
            }
            TextButton(
                onClick = onDismiss,
                colors = ButtonDefaults.textButtonColors(contentColor = T.colorTexte1Dark)
            ) {
                Text("Compris")
            }
        }
    }
}
