package dev.hakille.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.hakille.mobile.R
import hakille.tokens.HakilleTokens as T
import kotlinx.coroutines.delay

/**
 * 3-Tier Honest Wait Counter (states.rs §19):
 * - t < 400ms: LoadingFast (invisible transition, no spinner, no jitter)
 * - 400ms <= t < 2000ms: LoadingSkeleton (calm static skeleton, no fake radar)
 * - t >= 2000ms: LoadingHonest (exact elapsed seconds, local Qwen CPU explanation, active Annuler button)
 */
@Composable
fun HonestWaitComponent(
    startTimeMs: Long?,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier
) {
    if (startTimeMs == null) return

    var elapsedSeconds by remember(startTimeMs) { mutableLongStateOf(0L) }
    var elapsedMs by remember(startTimeMs) { mutableLongStateOf(0L) }

    LaunchedEffect(startTimeMs) {
        while (true) {
            val now = System.currentTimeMillis()
            elapsedMs = (now - startTimeMs).coerceAtLeast(0L)
            elapsedSeconds = elapsedMs / 1000L
            delay(100L)
        }
    }

    when {
        elapsedMs < 400L -> {
            // LoadingFast: invisible transition, zero height, no jitter
            Spacer(modifier = Modifier.height(0.dp).testTag("wait_fast"))
        }
        elapsedMs < 2000L -> {
            // LoadingSkeleton: calm static skeleton, no fake radar
            Card(
                modifier = modifier.fillMaxWidth().testTag("wait_skeleton"),
                colors = CardDefaults.cardColors(containerColor = T.colorSurface1Dark),
                shape = RoundedCornerShape(12.dp)
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth(0.6f)
                            .height(14.dp)
                            .clip(RoundedCornerShape(4.dp))
                            .background(T.colorSurface2Dark)
                    )
                    Box(
                        modifier = Modifier
                            .fillMaxWidth(0.85f)
                            .height(12.dp)
                            .clip(RoundedCornerShape(4.dp))
                            .background(T.colorSurface2Dark)
                    )
                }
            }
        }
        else -> {
            // LoadingHonest: exact elapsed seconds + local Qwen CPU explanation + active cancel button
            Card(
                modifier = modifier.fillMaxWidth().testTag("wait_honest"),
                colors = CardDefaults.cardColors(containerColor = T.colorSurface1Dark),
                shape = RoundedCornerShape(12.dp)
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(20.dp),
                            strokeWidth = 2.dp,
                            color = T.colorBraiseDark
                        )
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = "Génération locale Qwen CPU en cours… (${elapsedSeconds}s)",
                                style = MaterialTheme.typography.bodyMedium,
                                color = T.colorTexte1Dark
                            )
                            Text(
                                text = "Inférence sur le processeur du hub. Aucun traitement cloud.",
                                style = MaterialTheme.typography.bodySmall,
                                color = T.colorTexte2Dark
                            )
                        }
                    }
                    Button(
                        onClick = onCancel,
                        modifier = Modifier.align(Alignment.End).testTag("wait_cancel_button"),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = T.colorSurface2Dark,
                            contentColor = T.colorTexte1Dark
                        )
                    ) {
                        Text(stringResource(R.string.cancel_request))
                    }
                }
            }
        }
    }
}
