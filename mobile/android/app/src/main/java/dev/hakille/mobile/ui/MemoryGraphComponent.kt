package dev.hakille.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
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
 * Explorateur de mémoire souveraine et graphe de connaissances SPO (spec §23).
 * Affiche les triplets Sujet-Prédicat-Objet, la source certifiée, et permet
 * le droit à l'oubli cryptographique permanent avec émission de reçu d'effacement.
 */
@Composable
fun MemoryGraphComponent(
    memoryRows: List<JsonObject>,
    tombstones: List<String>,
    busy: Boolean,
    onSearch: (String) -> Unit,
    onRemember: (String) -> Unit,
    onForgetFact: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    var searchQuery by remember { mutableStateOf("") }
    var newFactText by remember { mutableStateOf("") }
    var factToForget by remember { mutableStateOf<String?>(null) }

    fun sourceColor(source: String): Color = when (source.lowercase()) {
        "user", "utilisateur" -> Color(0xFF5A98D6)
        "system", "système" -> Color(0xFF9B72CF)
        "inferred", "déduit" -> Color(0xFFDDA752)
        else -> Color(0xFF888888)
    }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .testTag("memory_graph_component"),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // Search & Filter Panel
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .hairlineBorder(HakillePalette.ligneDark),
            colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                Text(
                    text = "Recherche dans la mémoire souveraine",
                    style = MaterialTheme.typography.titleMedium,
                    color = HakillePalette.texte1Dark,
                    fontWeight = FontWeight.SemiBold
                )
                OutlinedTextField(
                    value = searchQuery,
                    onValueChange = { searchQuery = it },
                    label = { Text("Mots-clés ou entité...") },
                    modifier = Modifier.fillMaxWidth().testTag("memory_search_input"),
                    singleLine = true
                )
                Button(
                    onClick = { onSearch(searchQuery) },
                    enabled = !busy,
                    modifier = Modifier.align(Alignment.End).minTouchTarget()
                ) {
                    Text("Rechercher")
                }
            }
        }

        // Tombstones Banner (Cryptographic Erasure Receipts)
        if (tombstones.isNotEmpty()) {
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
                            .background(HakillePalette.avertissementDark, RoundedCornerShape(4.dp))
                    )
                    Text(
                        text = "${tombstones.size} fait(s) révoqué(s) (tombstones cryptographiques locales)",
                        style = hakilleMonoStyle(size = 12),
                        color = HakillePalette.texte2Dark
                    )
                }
            }
        }

        // List of SPO Triplets
        if (memoryRows.isEmpty()) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = HakillePalette.surface2Dark)
            ) {
                Text(
                    text = "Aucune ligne reçue. La mémoire n'est pas simulée ici.",
                    modifier = Modifier.padding(16.dp),
                    style = MaterialTheme.typography.bodyMedium,
                    color = HakillePalette.texte2Dark
                )
            }
        } else {
            memoryRows.forEach { row ->
                val rowId = row.text("id")
                val text = row.text("text")
                val source = row.text("source").ifBlank { "User" }
                val tags = (row["tags"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty()

                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("memory_row_$rowId")
                        .hairlineBorder(HakillePalette.ligneDark),
                    colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        // Source tag & Row ID
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Surface(
                                shape = RoundedCornerShape(4.dp),
                                color = sourceColor(source).copy(alpha = 0.2f),
                                border = androidx.compose.foundation.BorderStroke(1.dp, sourceColor(source))
                            ) {
                                Text(
                                    text = source,
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                                    style = hakilleMonoStyle(size = 11, weight = 600),
                                    color = sourceColor(source)
                                )
                            }
                            Text(
                                text = rowId,
                                style = hakilleMonoStyle(size = 11),
                                color = HakillePalette.texte2Dark
                            )
                        }

                        // Triplet Content
                        SelectionContainer {
                            Text(
                                text = text,
                                style = MaterialTheme.typography.bodyLarge,
                                color = HakillePalette.texte1Dark
                            )
                        }

                        // Tags
                        if (tags.isNotEmpty()) {
                            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                tags.forEach { tag ->
                                    Text(
                                        text = "#$tag",
                                        style = MaterialTheme.typography.bodySmall,
                                        color = HakillePalette.texte2Dark
                                    )
                                }
                            }
                        }

                        // Action: Forget fact (Droit à l'oubli)
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.End
                        ) {
                            OutlinedButton(
                                onClick = { factToForget = rowId },
                                enabled = !busy,
                                modifier = Modifier.minTouchTarget(),
                                border = androidx.compose.foundation.BorderStroke(1.dp, HakillePalette.braiseDark),
                                colors = ButtonDefaults.outlinedButtonColors(contentColor = HakillePalette.braiseDark)
                            ) {
                                Text("Ce fait est faux / Oublier", fontSize = 12.sp)
                            }
                        }
                    }
                }
            }
        }

        // New Fact Anchoring Card (Mémorisation)
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .hairlineBorder(HakillePalette.ligneDark),
            colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp)
            ) {
                Text(
                    text = "Ancrer un nouveau fait en mémoire",
                    style = MaterialTheme.typography.titleMedium,
                    color = HakillePalette.texte1Dark,
                    fontWeight = FontWeight.SemiBold
                )
                OutlinedTextField(
                    value = newFactText,
                    onValueChange = { newFactText = it },
                    label = { Text("Texte du fait à mémoriser...") },
                    modifier = Modifier.fillMaxWidth().testTag("memory_remember_input"),
                    minLines = 2
                )
                Button(
                    onClick = {
                        if (newFactText.isNotBlank()) {
                            onRemember(newFactText)
                            newFactText = ""
                        }
                    },
                    enabled = !busy && newFactText.isNotBlank(),
                    modifier = Modifier.align(Alignment.End).minTouchTarget()
                ) {
                    Text("Mémoriser")
                }
            }
        }
    }

    // Confirmation Dialog for Fact Forgetting
    factToForget?.let { targetId ->
        AlertDialog(
            onDismissRequest = { factToForget = null },
            title = { Text("Confirmer l'effacement définitif") },
            text = {
                Text("Ce fait sera physiquement purgé du graphe souverain sur le hub. Une preuve d'effacement cryptographique scellée sera conservée.")
            },
            confirmButton = {
                Button(
                    onClick = {
                        onForgetFact(targetId)
                        factToForget = null
                    },
                    colors = ButtonDefaults.buttonColors(containerColor = HakillePalette.braiseDark)
                ) {
                    Text("Confirmer l'oubli")
                }
            },
            dismissButton = {
                TextButton(onClick = { factToForget = null }) {
                    Text("Annuler")
                }
            }
        )
    }
}
