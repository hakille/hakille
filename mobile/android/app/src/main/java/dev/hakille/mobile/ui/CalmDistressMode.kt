package dev.hakille.mobile.ui

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.hakille.mobile.LocalHakilleDark
import dev.hakille.mobile.minTouchTarget
import hakille.tokens.HakilleTokens as T

/**
 * Calm Distress Mode constants conforming strictly to crates/hakille-design/src/household.rs.
 * STRICTLY ZERO BRAISE (#FF7A2F) and STRICTLY ZERO ANIMATIONS.
 */
object CalmDistressConstants {
    const val CALM_COPY: String = "Je reste là. Un geste pour appeler ton contact, un autre pour les urgences."
    const val EMERGENCY_NUMBER: String = "112"
}

/**
 * Soothing emergency UI for moments of distress.
 * Zero telemetry, zero autonomous psychological speculation, zero graph logging.
 */
@Composable
fun CalmDistressMode(
    trustedContact: String? = null,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current
    val isDark = LocalHakilleDark.current

    // Quiet, soothing palette. Braise (#FF7A2F) is STRICTLY PROHIBITED here.
    val backgroundColor = if (isDark) T.colorSurface0Dark else T.colorSurface0Light
    val containerColor = if (isDark) T.colorSurface1Dark else T.colorSurface1Light
    val outlineColor = if (isDark) T.colorLigneDark else T.colorLigneLight
    val textColor = if (isDark) T.colorTexte1Dark else T.colorTexte1Light
    val secondaryText = if (isDark) T.colorTexte2Dark else T.colorTexte2Light

    fun dialNumber(number: String) {
        val sanitized = number.filter { it.isDigit() || it == '+' }
        val uri = Uri.fromParts("tel", sanitized, null)
        val intent = Intent(Intent.ACTION_DIAL, uri).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK
        }
        runCatching { context.startActivity(intent) }
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(backgroundColor)
            .padding(24.dp),
        contentAlignment = Alignment.Center
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(containerColor, RoundedCornerShape(T.layoutRadiusCard.toInt().dp))
                .border(1.dp, outlineColor, RoundedCornerShape(T.layoutRadiusCard.toInt().dp))
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(20.dp)
        ) {
            // Static Calm Copy (ZERO animations or pulsating graphics)
            Text(
                text = CalmDistressConstants.CALM_COPY,
                style = MaterialTheme.typography.headlineSmall,
                color = textColor,
                textAlign = TextAlign.Center,
                modifier = Modifier.semantics { heading() }
            )

            Text(
                text = "Hakille est en veille sécurisée. Aucune donnée n'est envoyée.",
                style = MaterialTheme.typography.bodySmall,
                color = secondaryText,
                textAlign = TextAlign.Center
            )

            Spacer(modifier = Modifier.height(8.dp))

            // Direct Dial 1: Trusted Contact
            if (!trustedContact.isNullOrBlank()) {
                Button(
                    onClick = { dialNumber(trustedContact) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 56.dp)
                        .minTouchTarget(),
                    shape = RoundedCornerShape(T.layoutRadiusControl.toInt().dp),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = (if (isDark) T.colorSurface2Dark else T.colorSurface2Light),
                        contentColor = textColor
                    )
                ) {
                    Text(
                        text = "Appeler mon contact de confiance ($trustedContact)",
                        style = MaterialTheme.typography.titleSmall
                    )
                }
            }

            // Direct Dial 2: Emergency Services (112)
            Button(
                onClick = { dialNumber(CalmDistressConstants.EMERGENCY_NUMBER) },
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .minTouchTarget(),
                shape = RoundedCornerShape(T.layoutRadiusControl.toInt().dp),
                colors = ButtonDefaults.buttonColors(
                    containerColor = (if (isDark) T.colorSurface2Dark else T.colorSurface2Light),
                    contentColor = textColor
                )
            ) {
                Text(
                    text = "Appeler les urgences (112)",
                    style = MaterialTheme.typography.titleSmall
                )
            }

            Spacer(modifier = Modifier.height(4.dp))

            // Safe Dismiss Button
            OutlinedButton(
                onClick = onDismiss,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 48.dp)
                    .minTouchTarget(),
                shape = RoundedCornerShape(T.layoutRadiusControl.toInt().dp),
                colors = ButtonDefaults.outlinedButtonColors(
                    contentColor = secondaryText
                )
            ) {
                Text("Je vais bien, fermer")
            }
        }
    }
}
