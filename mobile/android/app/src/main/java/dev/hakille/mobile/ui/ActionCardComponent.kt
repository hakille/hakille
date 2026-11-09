package dev.hakille.mobile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.hakille.mobile.*
import dev.hakille.mobile.R
import hakille.tokens.HakilleTokens as T
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive

/**
 * Reversibility badge representation conforming to crates/hakille-design/src/action_card.rs:33-46.
 * Uses icon + label, never color alone.
 */
enum class CardReversibility(val label: String) {
    UNDOABLE("− annulable"),
    FINAL("+ définitif")
}

/**
 * Helper object for ActionCard logic and formatting.
 */
object ActionCardHelper {
    /**
     * Single Decision Open Constraint (spec §8 / action_card.rs:67).
     * Refuses to display or process more than 1 active decision card at a time.
     */
    fun filterSingleDecision(cards: List<JsonObject>): List<JsonObject> = cards.take(1)

    /**
     * Determines reversibility enum from card json.
     */
    fun reversibility(card: JsonObject): CardReversibility {
        val isReversible = card["reversible"]?.jsonPrimitive?.booleanOrNull == true
        return if (isReversible) CardReversibility.UNDOABLE else CardReversibility.FINAL
    }

    /**
     * Extracts the 3 mandatory badges in strict order:
     * 1. Cost (real monetary amount or 0.00 in JetBrains Mono)
     * 2. Data Destination (Local / Cloud : provider)
     * 3. Reversibility ("− annulable" or "+ définitif")
     */
    fun pastilles(card: JsonObject): Triple<String, String, String> {
        val cost = CardPresentation.money(card["cost"])
        val data = CardPresentation.destination(
            card["data_exit"],
            local = "Local",
            cloud = "Cloud",
            unknown = "Local"
        )
        val rev = reversibility(card).label
        return Triple(cost, data, rev)
    }

    /**
     * Parses the ProvenanceBand enum from the card JSON.
     */
    fun provenanceBand(card: JsonObject): ProvenanceBand {
        val raw = card["provenance"]
        return when ((raw as? kotlinx.serialization.json.JsonPrimitive)?.content) {
            "Hakille" -> ProvenanceBand.Hakille
            "External" -> ProvenanceBand.External
            else -> {
                val skill = (raw as? JsonObject)?.get("VerifiedSkill") as? JsonObject
                if (skill != null) {
                    ProvenanceBand.VerifiedSkill(
                        publisher = skill.text("publisher"),
                        name = skill.text("name")
                    )
                } else {
                    ProvenanceBand.Hakille
                }
            }
        }
    }
}

/**
 * ActionCardComponent conforming to Milestone 1 specs:
 * - Single decision open constraint
 * - 3 mandatory immutable badges in strict order (Cost, Data, Reversibility)
 * - Security Provenance Banner embedded at top
 * - 600 ms Hold-to-Confirm Button (with Braise progress ring)
 * - Direct alternative actions: Refuser (Reject) and Modifier (Edit)
 * - Dynamic type support without truncation
 */
@Composable
fun ActionCardComponent(
    card: JsonObject,
    approval: JsonObject?,
    busy: Boolean,
    onApprove: (JsonElement) -> Unit,
    onReject: (JsonElement) -> Unit,
    onEdit: (JsonElement) -> Unit = {},
    onRequestWhy: (JsonElement) -> Unit = {},
    onCancelApproval: () -> Unit = {},
    modifier: Modifier = Modifier
) {
    val isDark = LocalHakilleDark.current
    val id = card.getValue("id")
    val armed = approval?.get("card_id") == id

    val (costLabel, dataLabel, reversibilityLabel) = ActionCardHelper.pastilles(card)
    val provenance = ActionCardHelper.provenanceBand(card)
    val verb = sanitizeSkillText(card.text("verb"))
    val obj = sanitizeSkillText(card.text("object"))
    val reason = sanitizeSkillText(card.text("reason"))

    val surfaceColor = if (isDark) T.colorSurface1Dark else T.colorSurface1Light
    val outlineColor = if (isDark) T.colorLigneDark else T.colorLigneLight

    Card(
        modifier = modifier
            .fillMaxWidth()
            .hairlineBorder(outlineColor, 1.dp, RoundedCornerShape(T.layoutRadiusCard.toInt().dp)),
        colors = CardDefaults.cardColors(containerColor = surfaceColor),
        shape = RoundedCornerShape(T.layoutRadiusCard.toInt().dp)
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            // 1. Provenance Banner
            SecurityProvenanceBanner(provenance = provenance)

            // 2. Verb + Object headline
            Text(
                text = "$verb $obj",
                style = MaterialTheme.typography.headlineMedium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.semantics { heading() }
            )

            // 3. Plain language reason
            if (reason.isNotEmpty()) {
                Text(
                    text = reason,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }

            // 4. Three Mandatory Badges in Strict Order
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                // Pastille 1: Cost (Tabular Monospace font)
                Surface(
                    shape = RoundedCornerShape(6.dp),
                    color = if (isDark) T.colorSurface2Dark else T.colorSurface2Light,
                    border = BorderStroke(1.dp, outlineColor)
                ) {
                    Text(
                        text = costLabel,
                        style = hakilleMonoStyle(),
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp)
                    )
                }

                // Pastille 2: Data Destination
                Surface(
                    shape = RoundedCornerShape(6.dp),
                    color = if (isDark) T.colorSurface2Dark else T.colorSurface2Light,
                    border = BorderStroke(1.dp, outlineColor)
                ) {
                    Text(
                        text = dataLabel,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp)
                    )
                }

                // Pastille 3: Reversibility (Never color alone! Icon + Text)
                Surface(
                    shape = RoundedCornerShape(6.dp),
                    color = if (isDark) T.colorSurface2Dark else T.colorSurface2Light,
                    border = BorderStroke(1.dp, outlineColor)
                ) {
                    Text(
                        text = reversibilityLabel,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp)
                    )
                }
            }

            // 5. Action Buttons (Hold-to-Confirm, Reject, Edit, Pourquoi)
            if (!busy) {
                Spacer(modifier = Modifier.height(4.dp))

                HoldToConfirmButton(
                    onConfirmed = { onApprove(id) },
                    onCancelled = onCancelApproval,
                    label = "Maintenir 600 ms pour valider",
                    confirmedLabel = "Action validée",
                    enabled = true
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    OutlinedButton(
                        onClick = { onReject(id) },
                        modifier = Modifier
                            .weight(1f)
                            .minTouchTarget(),
                        colors = ButtonDefaults.outlinedButtonColors(
                            contentColor = if (isDark) T.colorErreurDark else T.colorErreurLight
                        )
                    ) {
                        Text("Refuser")
                    }

                    TextButton(
                        onClick = { onEdit(id) },
                        modifier = Modifier
                            .weight(1f)
                            .minTouchTarget()
                    ) {
                        Text("Modifier")
                    }

                    TextButton(
                        onClick = { onRequestWhy(id) },
                        modifier = Modifier
                            .weight(1f)
                            .minTouchTarget()
                    ) {
                        Text("Pourquoi ?")
                    }
                }
            }
        }
    }
}
