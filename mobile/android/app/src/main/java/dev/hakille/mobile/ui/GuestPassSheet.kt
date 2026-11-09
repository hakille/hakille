package dev.hakille.mobile.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.hakille.mobile.LocalHakilleDark
import dev.hakille.mobile.minTouchTarget
import dev.hakille.mobile.text
import hakille.tokens.HakilleTokens as T
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.floatOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive

object GuestPassHelper {
    const val GUEST_RING_PX: Int = 1
    const val DEFAULT_DURATION_SECS: Int = 3600

    val FORBIDDEN_GUEST_ACTIONS = setOf(
        "memory.forget",
        "wallet.spend",
        "system.reboot",
        "autonomy.promote",
        "audit.clear"
    )

    fun computeRingFraction(elapsedSecs: Int, durationSecs: Int): Float {
        if (durationSecs <= 0) return 0f
        return (1f - elapsedSecs.toFloat() / durationSecs.toFloat()).coerceIn(0f, 1f)
    }

    fun isExpired(elapsedSecs: Int, durationSecs: Int): Boolean =
        durationSecs <= 0 || elapsedSecs >= durationSecs

    fun isAuthorizedForGuest(power: String, expired: Boolean, requestedAction: String): Boolean {
        if (expired) return false
        if (requestedAction in FORBIDDEN_GUEST_ACTIONS) return false
        return power == requestedAction
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun GuestPassPanel(
    guestPass: JsonObject?,
    busy: Boolean,
    onStartGuestPass: (name: String, power: String, durationSecs: Int) -> Unit,
    onRevokeGuestPass: () -> Unit,
    modifier: Modifier = Modifier
) {
    val isDark = LocalHakilleDark.current
    val containerColor = if (isDark) T.colorSurface1Dark else T.colorSurface1Light
    val outlineColor = if (isDark) T.colorLigneDark else T.colorLigneLight
    val lueurColor = if (isDark) T.colorLueurDark else T.colorLueurLight
    val textColor = if (isDark) T.colorTexte1Dark else T.colorTexte1Light
    val secondaryText = if (isDark) T.colorTexte2Dark else T.colorTexte2Light

    var guestName by remember { mutableStateOf("Invité") }
    var guestPower by remember { mutableStateOf("wifi.connect") }
    var durationSecs by remember { mutableIntStateOf(GuestPassHelper.DEFAULT_DURATION_SECS) }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .testTag("guest_pass_panel")
            .background(containerColor, RoundedCornerShape(T.layoutRadiusCard.toInt().dp))
            .border(1.dp, outlineColor, RoundedCornerShape(T.layoutRadiusCard.toInt().dp))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(
            text = "Pass Invité éphémère",
            style = MaterialTheme.typography.titleMedium,
            color = textColor,
            modifier = Modifier.semantics { heading() }
        )
        Text(
            text = "Délégation temporaire bornée (anneau 1 px). Toutes les échelles d'autonomie sont abaissées au cran 0 (Observer).",
            style = MaterialTheme.typography.bodySmall,
            color = secondaryText
        )

        val active = guestPass != null && guestPass["expired"]?.jsonPrimitive?.booleanOrNull != true
        if (guestPass != null && active) {
            val name = guestPass.text("name")
            val power = guestPass.text("power")
            val dur = guestPass["duration_secs"]?.jsonPrimitive?.intOrNull ?: GuestPassHelper.DEFAULT_DURATION_SECS
            val elapsed = guestPass["elapsed_secs"]?.jsonPrimitive?.intOrNull ?: 0
            val fraction = guestPass["ring_fraction"]?.jsonPrimitive?.floatOrNull
                ?: GuestPassHelper.computeRingFraction(elapsed, dur)
            val remainingMin = ((dur - elapsed).coerceAtLeast(0)) / 60

            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                Box(
                    modifier = Modifier.size(64.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Canvas(modifier = Modifier.fillMaxSize()) {
                        // Strict 1px hairline ring (GUEST_RING_PX = 1)
                        val strokePx = GuestPassHelper.GUEST_RING_PX.dp.toPx()
                        drawArc(
                            color = outlineColor,
                            startAngle = 0f,
                            sweepAngle = 360f,
                            useCenter = false,
                            style = Stroke(width = strokePx)
                        )
                        drawArc(
                            color = lueurColor,
                            startAngle = -90f,
                            sweepAngle = 360f * fraction.coerceIn(0f, 1f),
                            useCenter = false,
                            style = Stroke(width = strokePx, cap = StrokeCap.Round)
                        )
                    }
                    Text(
                        text = "${remainingMin}m",
                        style = MaterialTheme.typography.labelMedium,
                        color = textColor
                    )
                }
                Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(text = name, style = MaterialTheme.typography.titleSmall, color = textColor)
                    Text(text = "Pouvoir : $power", style = MaterialTheme.typography.bodySmall, color = secondaryText)
                    Text(text = "Autonomie : Cran 0 (Observer)", style = MaterialTheme.typography.labelSmall, color = secondaryText)
                }
                OutlinedButton(
                    onClick = onRevokeGuestPass,
                    enabled = !busy,
                    modifier = Modifier.minTouchTarget().testTag("guest_pass_revoke")
                ) {
                    Text("Révoquer")
                }
            }
        } else {
            OutlinedTextField(
                value = guestName,
                onValueChange = { guestName = it },
                label = { Text("Nom de l'invité") },
                singleLine = true,
                enabled = !busy,
                modifier = Modifier.fillMaxWidth().testTag("guest_pass_name")
            )
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("wifi.connect", "salon.lumieres", "musique", "tv.control").forEach { preset ->
                    FilterChip(
                        selected = guestPower == preset,
                        onClick = { guestPower = preset },
                        label = { Text(preset) }
                    )
                }
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(1800 to "30 min", 3600 to "1 h", 7200 to "2 h").forEach { (secs, label) ->
                    FilterChip(
                        selected = durationSecs == secs,
                        onClick = { durationSecs = secs },
                        label = { Text(label) }
                    )
                }
            }
            Button(
                onClick = { onStartGuestPass(guestName, guestPower, durationSecs) },
                enabled = !busy && guestName.isNotBlank() && guestPower.isNotBlank(),
                modifier = Modifier.align(Alignment.End).minTouchTarget().testTag("guest_pass_start")
            ) {
                Text("Activer le pass invité")
            }
        }
    }
}
