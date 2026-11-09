package dev.hakille.mobile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import hakille.tokens.HakilleTokens as T

/**
 * Gabarit d'erreur d'action (states.rs §19):
 * Toujours dans l'ordre strict:
 * 1. Fait
 * 2. Pas fait
 * 3. Actions [Réessayer] [Conserver le brouillon]
 * 4. Détails techniques
 */
data class ActionErrorState(
    val done: String,
    val notDone: String,
    val technicalDetail: String,
    val retryLabel: String = "Réessayer",
    val undoLabel: String = "Conserver le brouillon"
)

@Composable
fun HonestErrorComponent(
    errorState: ActionErrorState,
    onRetry: () -> Unit,
    onKeepDraft: () -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth().testTag("honest_error_card"),
        colors = CardDefaults.cardColors(containerColor = T.colorSurface1Dark),
        border = BorderStroke(1.dp, T.colorErreurDark),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            // 1. Fait
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    text = "Fait :",
                    style = MaterialTheme.typography.labelMedium,
                    color = T.colorSuccesDark
                )
                Text(
                    text = errorState.done,
                    style = MaterialTheme.typography.bodyMedium,
                    color = T.colorTexte1Dark
                )
            }
            // 2. Pas fait
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    text = "Pas fait :",
                    style = MaterialTheme.typography.labelMedium,
                    color = T.colorErreurDark
                )
                Text(
                    text = errorState.notDone,
                    style = MaterialTheme.typography.bodyMedium,
                    color = T.colorTexte1Dark
                )
            }
            // 3. Actions
            Row(
                modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Button(
                    onClick = onRetry,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = T.colorBraiseDark,
                        contentColor = T.colorSurBraiseDark
                    ),
                    modifier = Modifier.testTag("error_retry_button")
                ) {
                    Text(errorState.retryLabel)
                }
                OutlinedButton(
                    onClick = onKeepDraft,
                    colors = ButtonDefaults.outlinedButtonColors(
                        contentColor = T.colorTexte1Dark
                    ),
                    border = BorderStroke(1.dp, T.colorLigneDark),
                    modifier = Modifier.testTag("error_keep_draft_button")
                ) {
                    Text(errorState.undoLabel)
                }
            }
            // 4. Détails
            Text(
                text = "Détails : ${errorState.technicalDetail}",
                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                color = T.colorTexte2Dark
            )
        }
    }
}
