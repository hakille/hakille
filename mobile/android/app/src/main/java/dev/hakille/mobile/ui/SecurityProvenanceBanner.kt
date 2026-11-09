package dev.hakille.mobile.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.hakille.mobile.LocalHakilleDark
import dev.hakille.mobile.tokenColor
import hakille.tokens.HakilleTokens as T
import kotlinx.coroutines.*

/** Sensitive clipboard TTL: 60 seconds (crates/hakille-design/src/provenance.rs:73) */
const val CLIPBOARD_SENSITIVE_TTL_SECS: Long = 60L

/**
 * Representation of the provenance band matching crates/hakille-design/src/provenance.rs.
 */
sealed class ProvenanceBand {
    object Hakille : ProvenanceBand() {
        fun label(): String = "Hakille"
    }
    data class VerifiedSkill(val publisher: String, val name: String) : ProvenanceBand() {
        fun label(): String = "skill vérifié ($publisher · $name)"
    }
    object External : ProvenanceBand() {
        fun label(): String = "externe"
    }
}

/**
 * External block constants (spec §17).
 * 2px lateral border rule + "externe" badge.
 */
object ExternalBlockConstants {
    const val SIDE_RULE_PX: Int = 2
    val SIDE_RULE_DP: Dp = 2.dp
    const val BADGE: String = "externe"
}

/**
 * Checks for typosquatting / lookalike attacks against installed skills.
 */
fun looksLikeInstalled(skillName: String, installedSkills: List<String>): Boolean {
    return installedSkills.any { other ->
        !other.equals(skillName, ignoreCase = true) &&
            (other.contains(skillName, ignoreCase = true) || skillName.contains(other, ignoreCase = true))
    }
}

/**
 * Strict text sanitization (crates/hakille-design/src/provenance.rs).
 * Strips entire <script> and <style> blocks (including bodies), all HTML tags,
 * and all terminal ANSI escape sequences.
 */
fun sanitizeSkillText(input: String): String {
    val noScriptOrStyle = input
        .replace(Regex("<script\\b[^>]*>[\\s\\S]*?</script>", RegexOption.IGNORE_CASE), "")
        .replace(Regex("<style\\b[^>]*>[\\s\\S]*?</style>", RegexOption.IGNORE_CASE), "")

    val noHtml = noScriptOrStyle.replace(Regex("<[^>]*>"), "")

    return noHtml
        .replace(Regex("\u001B\\[[0-9;?]*[a-zA-Z]"), "")
        .replace(Regex("\u001B\\].*?(\u0007|\u001B\\\\)"), "")
        .replace(Regex("\u001B[@-Z\\-_]"), "")
        .replace("\u001B", "")
}

/**
 * Copies sensitive text to the clipboard and automatically purges it after 60 seconds.
 */
fun copySensitiveToClipboard(
    context: Context,
    label: String,
    text: String,
    scope: CoroutineScope = CoroutineScope(Dispatchers.Main)
) {
    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
    val clip = ClipData.newPlainText(label, text)
    clipboard.setPrimaryClip(clip)

    scope.launch {
        delay(CLIPBOARD_SENSITIVE_TTL_SECS * 1000L)
        // Verify current clip is still the sensitive clip before purging
        if (clipboard.primaryClip?.description?.label == label) {
            clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
        }
    }
}

/**
 * Security Provenance Banner Component.
 * Embedded at the top of cards, with a distinct 2px lateral border for external origins.
 */
@Composable
fun SecurityProvenanceBanner(
    provenance: ProvenanceBand,
    warning: String? = null,
    modifier: Modifier = Modifier
) {
    val isDark = LocalHakilleDark.current
    val isExternal = provenance is ProvenanceBand.External
    val lateralRuleColor = if (isDark) T.colorLigneDark else T.colorLigneLight

    val badgeLabel = when (provenance) {
        is ProvenanceBand.Hakille -> ProvenanceBand.Hakille.label()
        is ProvenanceBand.VerifiedSkill -> provenance.label()
        is ProvenanceBand.External -> ExternalBlockConstants.BADGE
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .then(
                if (isExternal) {
                    // 2px lateral border on the left side
                    Modifier.drawBehind {
                        drawLine(
                            color = lateralRuleColor,
                            start = Offset(1.dp.toPx(), 0f),
                            end = Offset(1.dp.toPx(), size.height),
                            strokeWidth = ExternalBlockConstants.SIDE_RULE_DP.toPx()
                        )
                    }.padding(start = 8.dp)
                } else Modifier
            )
            .padding(vertical = 4.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Surface(
                shape = RoundedCornerShape(4.dp),
                color = if (isExternal) {
                    (if (isDark) T.colorLigneDark else T.colorLigneLight).copy(alpha = 0.4f)
                } else {
                    MaterialTheme.colorScheme.surfaceVariant
                }
            ) {
                Text(
                    text = badgeLabel,
                    style = MaterialTheme.typography.labelSmall,
                    color = if (isExternal) {
                        if (isDark) T.colorTexte2Dark else T.colorTexte2Light
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp)
                )
            }

            if (warning != null) {
                Text(
                    text = warning,
                    style = MaterialTheme.typography.labelSmall,
                    color = if (isDark) T.colorAvertissementDark else T.colorAvertissementLight
                )
            }
        }
    }
}
