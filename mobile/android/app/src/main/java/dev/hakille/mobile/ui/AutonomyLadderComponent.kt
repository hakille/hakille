package dev.hakille.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.hakille.mobile.*
import kotlinx.serialization.json.*

/**
 * Échelle d'autonomie par domaine (spec §24) : crans 0–4, jamais globaux.
 * Crans :
 * 0 : Observer (aucune action sans demande)
 * 1 : Préparer (prépare l'action, attend validation humaine)
 * 2 : Agir sur le réversible (actions réversibles directes, auditées)
 * 3 : Agir sous enveloppe (actions bornées par coût et volume, 90j)
 * 4 : Déléguer (autonomie opérationnelle complète sous supervision d'audit)
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AutonomyLadderComponent(
    domainAutonomies: List<JsonObject>,
    onSaveCrank: (domain: String, crank: Int, maxCostCents: Long?, maxActions: Int?, daysValid: Int?) -> Unit,
    modifier: Modifier = Modifier
) {
    val defaultDomains = listOf("Mail", "Agenda", "Maison", "Fichiers", "Paiement", "Code")
    var selectedDomainIndex by remember { mutableIntStateOf(0) }
    val activeDomain = defaultDomains.getOrElse(selectedDomainIndex) { "Mail" }

    val activeDto = domainAutonomies.firstOrNull { it.text("domain").equals(activeDomain, ignoreCase = true) }
    val currentCrank = (activeDto?.get("crank") as? JsonPrimitive)?.intOrNull ?: 1
    val validatedCount = (activeDto?.get("validated_count") as? JsonPrimitive)?.intOrNull ?: 0
    val promotionOfferable = (activeDto?.get("promotion_offerable") as? JsonPrimitive)?.booleanOrNull ?: false
    val envelope = activeDto?.objectOrNull("envelope")

    var sliderPosition by remember(activeDomain, currentCrank) { mutableFloatStateOf(currentCrank.toFloat()) }
    var maxCostCents by remember(activeDomain, envelope) {
        mutableStateOf(envelope?.get("max_cost_cents")?.jsonPrimitive?.contentOrNull ?: "1000")
    }
    var maxActions by remember(activeDomain, envelope) {
        mutableStateOf(envelope?.get("max_actions")?.jsonPrimitive?.contentOrNull ?: "50")
    }

    val crankLabels = listOf(
        "Cran 0 · Observer",
        "Cran 1 · Préparer",
        "Cran 2 · Agir sur le réversible",
        "Cran 3 · Agir sous enveloppe",
        "Cran 4 · Déléguer"
    )

    val crankDescriptions = listOf(
        "Hakille n'entreprend rien sans votre demande directe. Idéal pour l'observation passive.",
        "Hakille prépare les actions et les propositions en attente de votre décision explicite.",
        "Hakille exécute les actions réversibles directement. Toute action annulée rétrograde le cran.",
        "Hakille exécute dans les limites strictes d'une enveloppe de coût et d'actions (90 jours).",
        "Délégation complète avec journal d'audit cryptographique et surveillance permanente."
    )

    Card(
        modifier = modifier
            .fillMaxWidth()
            .testTag("autonomy_ladder_card")
            .hairlineBorder(HakillePalette.ligneDark),
        colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // Header
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = "Échelle d'autonomie",
                    style = MaterialTheme.typography.titleMedium,
                    color = HakillePalette.texte1Dark,
                    fontWeight = FontWeight.Bold
                )
                Text(
                    text = "Spec §24",
                    style = hakilleMonoStyle(size = 11),
                    color = HakillePalette.lueurDark
                )
            }

            // Domain Tabs
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                defaultDomains.forEachIndexed { index, domain ->
                    FilterChip(
                        selected = selectedDomainIndex == index,
                        onClick = { selectedDomainIndex = index },
                        label = { Text(domain) },
                        modifier = Modifier.minTouchTarget()
                    )
                }
            }

            // Promotion Offer Banner
            if (promotionOfferable) {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(containerColor = HakillePalette.surface2Dark)
                ) {
                    Row(
                        modifier = Modifier.padding(12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Box(
                            modifier = Modifier
                                .size(8.dp)
                                .background(HakillePalette.succesDark, RoundedCornerShape(4.dp))
                        )
                        Column {
                            Text(
                                text = "Promotion méritée",
                                style = MaterialTheme.typography.labelMedium,
                                color = HakillePalette.succesDark,
                                fontWeight = FontWeight.Bold
                            )
                            Text(
                                text = "$validatedCount actions validées sans correction : vous pouvez augmenter l'autonomie.",
                                style = MaterialTheme.typography.bodySmall,
                                color = HakillePalette.texte2Dark
                            )
                        }
                    }
                }
            }

            // Discrete Slider 0-4
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Text(
                        text = crankLabels[sliderPosition.toInt().coerceIn(0, 4)],
                        style = MaterialTheme.typography.bodyLarge,
                        color = HakillePalette.texte1Dark,
                        fontWeight = FontWeight.SemiBold
                    )
                    Text(
                        text = "${sliderPosition.toInt()}/4",
                        style = hakilleMonoStyle(size = 14, weight = 700),
                        color = HakillePalette.lueurDark
                    )
                }

                Slider(
                    value = sliderPosition,
                    onValueChange = { sliderPosition = it },
                    valueRange = 0f..4f,
                    steps = 3,
                    modifier = Modifier.fillMaxWidth().testTag("autonomy_slider")
                )

                Text(
                    text = crankDescriptions[sliderPosition.toInt().coerceIn(0, 4)],
                    style = MaterialTheme.typography.bodyMedium,
                    color = HakillePalette.texte2Dark,
                    lineHeight = 18.sp
                )
            }

            // Envelope Configuration for Cranks 3 and 4
            if (sliderPosition >= 3f) {
                HorizontalDivider(color = HakillePalette.ligneDark)
                Text(
                    text = "Enveloppe de sécurité (expiration 90 jours)",
                    style = MaterialTheme.typography.labelMedium,
                    color = HakillePalette.lueurDark
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    OutlinedTextField(
                        value = maxCostCents,
                        onValueChange = { maxCostCents = it.filter { char -> char.isDigit() } },
                        label = { Text("Plafond (centimes)") },
                        modifier = Modifier.weight(1f).testTag("envelope_cost_input"),
                        singleLine = true
                    )
                    OutlinedTextField(
                        value = maxActions,
                        onValueChange = { maxActions = it.filter { char -> char.isDigit() } },
                        label = { Text("Max actions") },
                        modifier = Modifier.weight(1f).testTag("envelope_actions_input"),
                        singleLine = true
                    )
                }
            }

            // Honesty warning: cancellation demotes
            Text(
                text = "Toute annulation d'une action autonome rétrograde automatiquement le domaine d'un cran.",
                style = MaterialTheme.typography.bodySmall,
                color = HakillePalette.texte2Dark
            )

            // Save button
            Button(
                onClick = {
                    val crank = sliderPosition.toInt()
                    val cost = if (crank >= 3) maxCostCents.toLongOrNull() ?: 1000L else null
                    val actions = if (crank >= 3) maxActions.toIntOrNull() ?: 50 else null
                    onSaveCrank(activeDomain, crank, cost, actions, 90)
                },
                modifier = Modifier
                    .fillMaxWidth()
                    .minTouchTarget()
                    .testTag("save_autonomy_button"),
                colors = ButtonDefaults.buttonColors(containerColor = HakillePalette.surface2Dark)
            ) {
                Text(
                    text = "Appliquer au domaine $activeDomain",
                    color = HakillePalette.texte1Dark
                )
            }
        }
    }
}
