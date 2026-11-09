package dev.hakille.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.hakille.mobile.*
import kotlinx.serialization.json.*

/**
 * Feuille « Pourquoi » à trois niveaux (spec §25) : phrase, faits, raison technique.
 * Niveau 1 : Phrase concise explicative.
 * Niveau 2 : Graphe des faits mobilisés avec bouton « Ce fait est faux ».
 * Niveau 3 : Plan détaillé, pouvoirs, confiance (mot + score), modèle, coût, sortie données.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WhySheetComponent(
    whySheet: JsonObject,
    onDismiss: () -> Unit,
    onDisputeFact: (String) -> Unit = {}
) {
    val targetId = whySheet.text("target_id")
    val sentence = whySheet.text("sentence").ifBlank { "Je n'ai pas gardé la raison de ce choix." }
    val facts = whySheet.rows("facts")
    val plan = whySheet.text("plan")
    val powers = (whySheet["powers"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty()
    val confidenceWord = whySheet.text("confidence_word").ifBlank { "basse" }
    val confidenceValue = (whySheet["confidence_value"] as? JsonPrimitive)?.doubleOrNull ?: 0.0
    val alternatives = (whySheet["alternatives"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty()
    val modelLine = whySheet.text("model_line").ifBlank { "Qwen 2.5 0.5B Local CPU" }
    val costLabel = whySheet.text("cost_label").ifBlank { "0.00 EUR" }
    val dataLine = whySheet.text("data_line").ifBlank { "Local uniquement" }
    val wouldChange = whySheet.text("would_change")

    val confidenceColor = when {
        confidenceValue >= 0.85 -> HakillePalette.succesDark
        confidenceValue >= 0.50 -> HakillePalette.avertissementDark
        else -> HakillePalette.erreurDark
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = HakillePalette.surface1Dark,
        scrimColor = Color.Black.copy(alpha = 0.7f),
        shape = RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp),
        modifier = Modifier.testTag("why_sheet_modal")
    ) {
        LazyColumn(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // Header
            item {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = "Pourquoi cette action ?",
                        style = MaterialTheme.typography.titleLarge,
                        color = HakillePalette.texte1Dark,
                        fontWeight = FontWeight.Bold
                    )
                    IconButton(
                        onClick = onDismiss,
                        modifier = Modifier.minTouchTarget()
                    ) {
                        Text("✕", color = HakillePalette.texte2Dark, fontSize = 18.sp)
                    }
                }
            }

            // Niveau 1 : En une phrase
            item {
                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .hairlineBorder(HakillePalette.ligneDark),
                    colors = CardDefaults.cardColors(containerColor = HakillePalette.surface2Dark)
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Text(
                            text = "NIVEAU 1 · EN BREF",
                            style = hakilleMonoStyle(size = 11, weight = 600),
                            color = HakillePalette.lueurDark
                        )
                        Text(
                            text = sentence,
                            style = MaterialTheme.typography.bodyLarge,
                            color = HakillePalette.texte1Dark,
                            lineHeight = 22.sp
                        )
                    }
                }
            }

            // Niveau 2 : Faits mobilisés
            item {
                Text(
                    text = "NIVEAU 2 · FAITS DU GRAPHE MOBILISÉS",
                    style = hakilleMonoStyle(size = 11, weight = 600),
                    color = HakillePalette.lueurDark
                )
            }

            if (facts.isEmpty()) {
                item {
                    Text(
                        text = "Aucun fait externe mobilisé directement. Décision basée sur les règles locales.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = HakillePalette.texte2Dark
                    )
                }
            } else {
                items(facts) { fact ->
                    val label = fact.text("label")
                    val source = fact.text("source")
                    val date = fact.text("date")
                    val factId = fact.text("id").ifBlank { label }

                    Card(
                        modifier = Modifier
                            .fillMaxWidth()
                            .hairlineBorder(HakillePalette.ligneDark),
                        colors = CardDefaults.cardColors(containerColor = HakillePalette.surface0Dark)
                    ) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(12.dp),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column(modifier = Modifier.weight(1f)) {
                                Text(
                                    text = label,
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = HakillePalette.texte1Dark
                                )
                                Text(
                                    text = "Source : $source · $date",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = HakillePalette.texte2Dark
                                )
                            }
                            OutlinedButton(
                                onClick = { onDisputeFact(factId) },
                                modifier = Modifier
                                    .padding(start = 8.dp)
                                    .minTouchTarget(),
                                border = androidx.compose.foundation.BorderStroke(1.dp, HakillePalette.braiseDark),
                                colors = ButtonDefaults.outlinedButtonColors(contentColor = HakillePalette.braiseDark)
                            ) {
                                Text("Ce fait est faux", fontSize = 12.sp)
                            }
                        }
                    }
                }
            }

            // Niveau 3 : Raisonnement technique et garanties
            item {
                Text(
                    text = "NIVEAU 3 · RAISONNEMENT TECHNIQUE & GARANTIES",
                    style = hakilleMonoStyle(size = 11, weight = 600),
                    color = HakillePalette.lueurDark
                )
            }

            item {
                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .hairlineBorder(HakillePalette.ligneDark),
                    colors = CardDefaults.cardColors(containerColor = HakillePalette.surface2Dark)
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        // Confiance
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text("Indice de confiance :", color = HakillePalette.texte2Dark, style = MaterialTheme.typography.bodyMedium)
                            Row(
                                horizontalArrangement = Arrangement.spacedBy(6.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(8.dp)
                                        .background(confidenceColor, RoundedCornerShape(4.dp))
                                )
                                Text(
                                    text = "$confidenceWord (${(confidenceValue * 100).toInt()} %)",
                                    style = hakilleMonoStyle(size = 13, weight = 600),
                                    color = confidenceColor
                                )
                            }
                        }

                        // Modèle & Coût
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text("Moteur d'inférence :", color = HakillePalette.texte2Dark, style = MaterialTheme.typography.bodyMedium)
                            Text(modelLine, color = HakillePalette.texte1Dark, style = MaterialTheme.typography.bodyMedium)
                        }

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text("Coût prévisionnel :", color = HakillePalette.texte2Dark, style = MaterialTheme.typography.bodyMedium)
                            Text(costLabel, color = HakillePalette.texte1Dark, style = hakilleMonoStyle(size = 13))
                        }

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text("Destination des données :", color = HakillePalette.texte2Dark, style = MaterialTheme.typography.bodyMedium)
                            Text(dataLine, color = HakillePalette.texte1Dark, style = MaterialTheme.typography.bodyMedium)
                        }

                        if (powers.isNotEmpty()) {
                            Text("Capacités requises :", color = HakillePalette.texte2Dark, style = MaterialTheme.typography.bodyMedium)
                            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                powers.forEach { power ->
                                    Surface(
                                        shape = RoundedCornerShape(4.dp),
                                        color = HakillePalette.surface0Dark,
                                        border = androidx.compose.foundation.BorderStroke(1.dp, HakillePalette.ligneDark)
                                    ) {
                                        Text(
                                            text = power,
                                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                                            style = hakilleMonoStyle(size = 11),
                                            color = HakillePalette.texte1Dark
                                        )
                                    }
                                }
                            }
                        }

                        if (plan.isNotBlank()) {
                            HorizontalDivider(color = HakillePalette.ligneDark)
                            Text("Plan d'exécution détaillé :", color = HakillePalette.texte2Dark, style = MaterialTheme.typography.bodySmall)
                            Text(
                                text = plan,
                                style = MaterialTheme.typography.bodyMedium,
                                color = HakillePalette.texte1Dark
                            )
                        }

                        if (wouldChange.isNotBlank()) {
                            Text("Impact de validation :", color = HakillePalette.texte2Dark, style = MaterialTheme.typography.bodySmall)
                            Text(
                                text = wouldChange,
                                style = MaterialTheme.typography.bodyMedium,
                                color = HakillePalette.texte1Dark
                            )
                        }
                    }
                }
            }

            // Bouton fermer
            item {
                Button(
                    onClick = onDismiss,
                    modifier = Modifier
                        .fillMaxWidth()
                        .minTouchTarget(),
                    colors = ButtonDefaults.buttonColors(containerColor = HakillePalette.surface2Dark)
                ) {
                    Text("Fermer l'explication", color = HakillePalette.texte1Dark)
                }
            }
        }
    }
}
