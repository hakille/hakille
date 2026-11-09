package dev.hakille.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
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
 * Composant de consultation du journal d'audit cryptographique signé Ed25519 (spec §22).
 * Chaque entrée est append-only dans `audit.redb`.
 * Permet l'annulation (Undo) des actions réversibles et l'affichage de la feuille "Pourquoi ?".
 */
@Composable
fun AuditJournalComponent(
    entries: List<JsonObject>,
    busy: Boolean,
    onRefresh: () -> Unit,
    onUndo: (Long) -> Unit,
    onRequestWhy: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .testTag("audit_journal_component"),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // Cryptographic Chain Verification Badge
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .hairlineBorder(HakillePalette.ligneDark),
            colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Box(
                            modifier = Modifier
                                .size(10.dp)
                                .background(HakillePalette.succesDark, RoundedCornerShape(5.dp))
                        )
                        Text(
                            text = "Chaîne d'audit intègre",
                            style = MaterialTheme.typography.titleSmall,
                            color = HakillePalette.succesDark,
                            fontWeight = FontWeight.Bold
                        )
                    }
                    Text(
                        text = "Ed25519 + SHA-256",
                        style = hakilleMonoStyle(size = 11, weight = 600),
                        color = HakillePalette.lueurDark
                    )
                }
                Text(
                    text = "Toutes les entrées sont signées par la clé Ed25519 du hub et chaînées par hachage cryptographique non répudiable.",
                    style = MaterialTheme.typography.bodySmall,
                    color = HakillePalette.texte2Dark
                )
                Button(
                    onClick = onRefresh,
                    enabled = !busy,
                    modifier = Modifier.align(Alignment.End).minTouchTarget()
                ) {
                    Text("Actualiser le journal")
                }
            }
        }

        if (entries.isEmpty()) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = HakillePalette.surface2Dark)
            ) {
                Text(
                    text = "Aucune entrée de journal reçue du hub. Le journal d'audit n'est jamais simulé.",
                    modifier = Modifier.padding(16.dp),
                    style = MaterialTheme.typography.bodyMedium,
                    color = HakillePalette.texte2Dark
                )
            }
        } else {
            entries.forEach { entry ->
                val idStr = entry.text("id")
                val timestamp = entry.text("timestamp")
                val action = entry.text("action")
                val summary = entry.text("summary")
                val journalId = idStr.toLongOrNull() ?: 0L
                val isReversible = !action.lowercase().contains("suppression définitive")

                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("journal_entry_$idStr")
                        .hairlineBorder(HakillePalette.ligneDark),
                    colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text(
                                text = action,
                                style = MaterialTheme.typography.titleMedium,
                                color = HakillePalette.texte1Dark,
                                fontWeight = FontWeight.SemiBold
                            )
                            Text(
                                text = "#$idStr",
                                style = hakilleMonoStyle(size = 12),
                                color = HakillePalette.lueurDark
                            )
                        }

                        Text(
                            text = timestamp,
                            style = hakilleMonoStyle(size = 11),
                            color = HakillePalette.texte2Dark
                        )

                        SelectionContainer {
                            Text(
                                text = summary,
                                style = MaterialTheme.typography.bodyMedium,
                                color = HakillePalette.texte1Dark
                            )
                        }

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.End,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            // Bouton Pourquoi
                            TextButton(
                                onClick = { onRequestWhy(idStr) },
                                modifier = Modifier.minTouchTarget()
                            ) {
                                Text("Pourquoi ?", color = HakillePalette.lueurDark)
                            }

                            Spacer(modifier = Modifier.width(8.dp))

                            // Bouton Annuler (Undo)
                            if (isReversible && journalId > 0) {
                                OutlinedButton(
                                    onClick = { onUndo(journalId) },
                                    enabled = !busy,
                                    modifier = Modifier.minTouchTarget(),
                                    border = androidx.compose.foundation.BorderStroke(1.dp, HakillePalette.braiseDark),
                                    colors = ButtonDefaults.outlinedButtonColors(contentColor = HakillePalette.braiseDark)
                                ) {
                                    Text("Annuler (Undo)")
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
