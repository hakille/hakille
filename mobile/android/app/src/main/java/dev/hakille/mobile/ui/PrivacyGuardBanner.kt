package dev.hakille.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.hakille.mobile.LocalHakilleDark
import dev.hakille.mobile.minTouchTarget
import dev.hakille.mobile.objectOrNull
import dev.hakille.mobile.text
import hakille.tokens.HakilleTokens as T
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive

object PrivacyGuardHelper {
    fun shouldTriggerVeil(peopleCount: Int): Boolean = peopleCount >= 2

    fun triggerLabel(triggerObj: JsonObject?, triggerText: String): String {
        if (triggerText == "EphemeralTtlPurged") {
            return "Purge RAM biométrique (TTL)"
        }
        if (triggerObj != null) {
            triggerObj.objectOrNull("ShoulderSurfingDetected")?.let {
                val count = it["people_count"]?.jsonPrimitive?.intOrNull ?: 2
                return "Regard tiers détecté ($count personnes)"
            }
            triggerObj.objectOrNull("RawDataExfiltrationBlocked")?.let {
                val scope = it.text("blocked_scope")
                return "Exfiltration bloquée ($scope)"
            }
            triggerObj.objectOrNull("HardwareKillSwitchActivated")?.let {
                val device = it.text("device")
                return "Coupe-circuit matériel ($device)"
            }
        }
        return "Bouclier de confidentialité"
    }
}

class PrivacyGuardDebounceFilter(private val requiredConsecutive: Int = 3) {
    private var consecutiveTriggers = 0
    var veilActive: Boolean = false
        private set

    fun update(peopleCount: Int): Boolean {
        if (PrivacyGuardHelper.shouldTriggerVeil(peopleCount)) {
            consecutiveTriggers++
            if (consecutiveTriggers >= requiredConsecutive) {
                veilActive = true
            }
        } else {
            consecutiveTriggers = 0
            veilActive = false
        }
        return veilActive
    }
}

@Composable
fun PrivacyGuardBanner(
    event: JsonObject,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    val isDark = LocalHakilleDark.current
    val containerColor = if (isDark) T.colorSurface1Dark else T.colorSurface1Light
    val outlineColor = if (isDark) T.colorLigneDark else T.colorLigneLight
    val textColor = if (isDark) T.colorTexte1Dark else T.colorTexte1Light
    val secondaryText = if (isDark) T.colorTexte2Dark else T.colorTexte2Light

    val triggerObj = event.objectOrNull("trigger")
    val triggerText = event.text("trigger")
    val badge = PrivacyGuardHelper.triggerLabel(triggerObj, triggerText)
    val description = event.text("description")
    val recommended = event.text("recommended_action")

    Column(
        modifier = modifier
            .fillMaxWidth()
            .testTag("privacy_guard_banner")
            .background(containerColor, RoundedCornerShape(T.layoutRadiusCard.toInt().dp))
            .border(1.dp, outlineColor, RoundedCornerShape(T.layoutRadiusCard.toInt().dp))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = badge,
                style = MaterialTheme.typography.titleSmall,
                color = textColor,
                modifier = Modifier.weight(1f).semantics { heading() }
            )
            TextButton(
                onClick = onDismiss,
                modifier = Modifier.minTouchTarget().testTag("privacy_guard_dismiss")
            ) {
                Text("Fermer", color = secondaryText)
            }
        }
        if (description.isNotEmpty()) {
            Text(
                text = description,
                style = MaterialTheme.typography.bodyMedium,
                color = textColor
            )
        }
        if (recommended.isNotEmpty()) {
            Text(
                text = recommended,
                style = MaterialTheme.typography.bodySmall,
                color = secondaryText
            )
        }
    }
}

@Composable
fun PrivacyGuardVeilOverlay(
    event: JsonObject?,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    val isDark = LocalHakilleDark.current
    // Strict Obsidian palette — zero Braise (#FF7A2F) in privacy veil
    val veilBackground = if (isDark) T.colorSurface0Dark else T.colorSurface0Light
    val cardBackground = if (isDark) T.colorSurface1Dark else T.colorSurface1Light
    val outlineColor = if (isDark) T.colorLigneDark else T.colorLigneLight
    val textColor = if (isDark) T.colorTexte1Dark else T.colorTexte1Light
    val secondaryText = if (isDark) T.colorTexte2Dark else T.colorTexte2Light

    val description = event?.text("description")?.ifBlank { null }
        ?: "Observateur tiers détecté derrière l'écran."
    val recommended = event?.text("recommended_action")?.ifBlank { null }
        ?: "Écran voilé pour protéger vos données souveraines."

    Box(
        modifier = modifier
            .fillMaxSize()
            .testTag("privacy_guard_veil")
            .background(veilBackground.copy(alpha = 0.96f))
            .padding(24.dp),
        contentAlignment = Alignment.Center
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(cardBackground, RoundedCornerShape(T.layoutRadiusCard.toInt().dp))
                .border(1.dp, outlineColor, RoundedCornerShape(T.layoutRadiusCard.toInt().dp))
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            Text(
                text = "Bouclier PrivacyGuard actif",
                style = MaterialTheme.typography.headlineSmall,
                color = textColor,
                textAlign = TextAlign.Center,
                modifier = Modifier.semantics { heading() }
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodyLarge,
                color = textColor,
                textAlign = TextAlign.Center
            )
            Text(
                text = recommended,
                style = MaterialTheme.typography.bodySmall,
                color = secondaryText,
                textAlign = TextAlign.Center
            )
            Spacer(modifier = Modifier.height(8.dp))
            OutlinedButton(
                onClick = onDismiss,
                modifier = Modifier
                    .fillMaxWidth()
                    .minTouchTarget()
                    .testTag("privacy_veil_unlock"),
                shape = RoundedCornerShape(T.layoutRadiusControl.toInt().dp)
            ) {
                Text("Je suis seul · Déverrouiller l'écran", color = textColor)
            }
        }
    }
}
