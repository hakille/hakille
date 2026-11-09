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
 * Catalogue des agents et compétences (spec §28 & §30).
 * CONTRAT D'HONNÊTETÉ STRICT :
 * `artifact_present: true` = binaire WASM vérifié et exécutable en bac à sable.
 * `artifact_present: false` = manifeste déclaratif (déclaratif uniquement, binaire absent, exécution interdite).
 */
@Composable
fun AgentCatalogComponent(
    agents: List<JsonObject>,
    lastAgentRun: JsonObject?,
    busy: Boolean,
    onRefresh: () -> Unit,
    onRunAgent: (name: String, inputJson: String) -> Unit,
    modifier: Modifier = Modifier
) {
    val executableAgents = agents.filter { it["artifact_present"]?.jsonPrimitive?.booleanOrNull == true }
    val declarativeAgents = agents.filter { it["artifact_present"]?.jsonPrimitive?.booleanOrNull != true }

    var runningAgentName by remember { mutableStateOf<String?>(null) }
    var agentInputJson by remember { mutableStateOf("{}") }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .testTag("agent_catalog_component"),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // Refresh & Summary Header
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .hairlineBorder(HakillePalette.ligneDark),
            colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column {
                    Text(
                        text = "Catalogue d'agents",
                        style = MaterialTheme.typography.titleMedium,
                        color = HakillePalette.texte1Dark,
                        fontWeight = FontWeight.Bold
                    )
                    Text(
                        text = "${executableAgents.size} exécutable(s) · ${declarativeAgents.size} déclaratif(s)",
                        style = MaterialTheme.typography.bodySmall,
                        color = HakillePalette.texte2Dark
                    )
                }
                Button(
                    onClick = onRefresh,
                    enabled = !busy,
                    modifier = Modifier.minTouchTarget()
                ) {
                    Text("Actualiser")
                }
            }
        }

        // Section 1 : Agents Exécutables (WASM vérifié)
        Text(
            text = "AGENTS EXÉCUTABLES (WASM VÉRIFIÉ)",
            style = hakilleMonoStyle(size = 11, weight = 600),
            color = HakillePalette.succesDark
        )

        if (executableAgents.isEmpty()) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = HakillePalette.surface2Dark)
            ) {
                Text(
                    text = "Aucun agent WASM exécutable n'est actuellement provisionné sur le hub.",
                    modifier = Modifier.padding(16.dp),
                    style = MaterialTheme.typography.bodyMedium,
                    color = HakillePalette.texte2Dark
                )
            }
        } else {
            executableAgents.forEach { agent ->
                val name = agent.text("name")
                val sha256 = agent.text("sha256")
                val caps = (agent["required_caps"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty()

                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("agent_exec_$name")
                        .hairlineBorder(HakillePalette.ligneDark),
                    colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text(
                                text = name,
                                style = MaterialTheme.typography.titleMedium,
                                color = HakillePalette.texte1Dark,
                                fontWeight = FontWeight.Bold
                            )
                            Surface(
                                shape = RoundedCornerShape(4.dp),
                                color = HakillePalette.succesDark.copy(alpha = 0.2f),
                                border = androidx.compose.foundation.BorderStroke(1.dp, HakillePalette.succesDark)
                            ) {
                                Text(
                                    text = "WASM VÉRIFIÉ",
                                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                    style = hakilleMonoStyle(size = 10, weight = 700),
                                    color = HakillePalette.succesDark
                                )
                            }
                        }

                        if (sha256.isNotBlank()) {
                            Text(
                                text = "SHA-256 : ${sha256.take(16)}…${sha256.takeLast(8)}",
                                style = hakilleMonoStyle(size = 11),
                                color = HakillePalette.lueurDark
                            )
                        }

                        if (caps.isNotEmpty()) {
                            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                caps.forEach { cap ->
                                    Surface(
                                        shape = RoundedCornerShape(4.dp),
                                        color = HakillePalette.surface0Dark
                                    ) {
                                        Text(
                                            text = cap,
                                            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                            style = hakilleMonoStyle(size = 11),
                                            color = HakillePalette.texte2Dark
                                        )
                                    }
                                }
                            }
                        }

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.End
                        ) {
                            Button(
                                onClick = {
                                    runningAgentName = name
                                    agentInputJson = "{}"
                                },
                                enabled = !busy,
                                modifier = Modifier.minTouchTarget()
                            ) {
                                Text("Lancer en bac à sable")
                            }
                        }
                    }
                }
            }
        }

        // Section 2 : Agents Déclaratifs (Manifestes d'intention)
        Text(
            text = "AGENTS DÉCLARATIFS (MANIFESTES D'INTENTION)",
            style = hakilleMonoStyle(size = 11, weight = 600),
            color = HakillePalette.texte2Dark
        )

        if (declarativeAgents.isEmpty()) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = HakillePalette.surface2Dark)
            ) {
                Text(
                    text = "Aucun agent déclaratif répertorié.",
                    modifier = Modifier.padding(16.dp),
                    style = MaterialTheme.typography.bodyMedium,
                    color = HakillePalette.texte2Dark
                )
            }
        } else {
            declarativeAgents.forEach { agent ->
                val name = agent.text("name")
                val caps = (agent["required_caps"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty()

                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("agent_decl_$name")
                        .hairlineBorder(HakillePalette.ligneDark),
                    colors = CardDefaults.cardColors(containerColor = HakillePalette.surface1Dark)
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text(
                                text = name,
                                style = MaterialTheme.typography.titleMedium,
                                color = HakillePalette.texte1Dark,
                                fontWeight = FontWeight.SemiBold
                            )
                            Surface(
                                shape = RoundedCornerShape(4.dp),
                                color = Color.Gray.copy(alpha = 0.2f),
                                border = androidx.compose.foundation.BorderStroke(1.dp, Color.Gray)
                            ) {
                                Text(
                                    text = "DÉCLARATIF",
                                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                    style = hakilleMonoStyle(size = 10, weight = 600),
                                    color = Color.Gray
                                )
                            }
                        }

                        Text(
                            text = "Manifeste d'intention seul. Aucun artefact binaire disponible sur ce hub.",
                            style = MaterialTheme.typography.bodySmall,
                            color = HakillePalette.texte2Dark
                        )

                        if (caps.isNotEmpty()) {
                            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                caps.forEach { cap ->
                                    Surface(
                                        shape = RoundedCornerShape(4.dp),
                                        color = HakillePalette.surface0Dark
                                    ) {
                                        Text(
                                            text = cap,
                                            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                            style = hakilleMonoStyle(size = 11),
                                            color = HakillePalette.texte2Dark
                                        )
                                    }
                                }
                            }
                        }

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.End
                        ) {
                            OutlinedButton(
                                onClick = {},
                                enabled = false,
                                modifier = Modifier.minTouchTarget()
                            ) {
                                Text("Binaire WASM absent")
                            }
                        }
                    }
                }
            }
        }

        // Last Agent Execution Outcome Card
        lastAgentRun?.let { outcome ->
            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("last_agent_run_card")
                    .hairlineBorder(HakillePalette.ligneDark),
                colors = CardDefaults.cardColors(containerColor = HakillePalette.surface2Dark)
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Text(
                        text = "DERNIÈRE EXÉCUTION EN BAC À SABLE",
                        style = hakilleMonoStyle(size = 11, weight = 700),
                        color = HakillePalette.succesDark
                    )
                    Text(
                        text = "Agent : ${outcome.text("name")} (${outcome.text("duration_ms")} ms)",
                        style = MaterialTheme.typography.titleSmall,
                        color = HakillePalette.texte1Dark
                    )
                    Text(
                        text = "Empreinte entrée SHA-256 : ${outcome.text("input_sha256").take(16)}…",
                        style = hakilleMonoStyle(size = 11),
                        color = HakillePalette.lueurDark
                    )
                    Text(
                        text = "Empreinte sortie SHA-256 : ${outcome.text("output_sha256").take(16)}…",
                        style = hakilleMonoStyle(size = 11),
                        color = HakillePalette.lueurDark
                    )
                    SelectionContainer {
                        Text(
                            text = outcome.text("output_json"),
                            style = hakilleMonoStyle(size = 12),
                            color = HakillePalette.texte1Dark
                        )
                    }
                }
            }
        }
    }

    // Modal Runner Dialog
    runningAgentName?.let { agentName ->
        AlertDialog(
            onDismissRequest = { runningAgentName = null },
            title = { Text("Lancer $agentName") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        text = "Entrée JSON transmise au bac à sable Wasmtime :",
                        style = MaterialTheme.typography.bodySmall
                    )
                    OutlinedTextField(
                        value = agentInputJson,
                        onValueChange = { agentInputJson = it },
                        modifier = Modifier.fillMaxWidth().testTag("sandbox_input_field"),
                        minLines = 3
                    )
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        onRunAgent(agentName, agentInputJson)
                        runningAgentName = null
                    },
                    enabled = !busy
                ) {
                    Text("Exécuter")
                }
            },
            dismissButton = {
                TextButton(onClick = { runningAgentName = null }) {
                    Text("Annuler")
                }
            }
        )
    }
}
