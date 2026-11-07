package dev.hakille.mobile

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/** Signe immobile. Aucun indicateur de niveau audio n’est inventé. */
@Composable fun PresenceMark(listening: Boolean, modifier: Modifier = Modifier) {
    val line = MaterialTheme.colorScheme.outlineVariant
    val glow = MaterialTheme.colorScheme.primary
    Box(modifier.size(176.dp), contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            drawCircle(if (listening) glow else line, radius = size.minDimension / 2 - 2.dp.toPx(), style = Stroke(if (listening) 2.dp.toPx() else 1.dp.toPx()))
            drawCircle(line, radius = size.minDimension / 2 - 12.dp.toPx(), style = Stroke(1.dp.toPx()))
        }
        Image(painterResource(if (LocalHakilleDark.current) R.drawable.hakille_mark_dark else R.drawable.hakille_mark_light), contentDescription = null, modifier = Modifier.size(136.dp))
    }
}

enum class AppDestination(val label: Int, val tag: String) {
    VOICE(R.string.ui_voice, "nav_voice"), MEMORY(R.string.memory, "nav_memory"), ACTIVITY(R.string.ui_activity, "nav_activity"), HUB(R.string.ui_hub, "nav_hub")
}

@Composable fun HakilleNavigation(selected: AppDestination, onSelect: (AppDestination) -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)) {
        Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            AppDestination.entries.forEach { destination ->
                val active = selected == destination
                Column(Modifier.weight(1f).testTag(destination.tag)
                    .selectable(selected = active, role = Role.Tab, onClick = { onSelect(destination) })
                    .heightIn(min = 64.dp).padding(horizontal = 4.dp, vertical = 12.dp),
                    horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Surface(shape = CircleShape, color = if (active) MaterialTheme.colorScheme.surfaceVariant else MaterialTheme.colorScheme.surface) {
                        Box(Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) { NavigationGlyph(destination, active) }
                    }
                    Text(stringResource(destination.label), style = MaterialTheme.typography.labelMedium, textAlign = TextAlign.Center,
                        color = if (active) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

@Composable private fun NavigationGlyph(destination: AppDestination, active: Boolean) {
    val color = if (active) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
    Canvas(Modifier.size(24.dp)) {
        val unit = size.width / 24f
        val stroke = Stroke(1.7f * unit, cap = StrokeCap.Round)
        fun point(x: Float, y: Float) = Offset(x * unit, y * unit)
        fun line(x1: Float, y1: Float, x2: Float, y2: Float) = drawLine(color, point(x1,y1), point(x2,y2), stroke.width, StrokeCap.Round)
        when (destination) {
            AppDestination.VOICE -> {
                drawRoundRect(color, point(9f,3f), Size(6*unit,11*unit), androidx.compose.ui.geometry.CornerRadius(3*unit), style = stroke)
                drawArc(color,0f,180f,false, point(6f,9f),Size(12*unit,10*unit),style=stroke)
                line(12f,19f,12f,22f); line(9f,22f,15f,22f)
            }
            AppDestination.MEMORY -> {
                drawRoundRect(color,point(4f,4f),Size(14*unit,14*unit),androidx.compose.ui.geometry.CornerRadius(3*unit),style=stroke)
                line(8f,22f,22f,22f); line(22f,8f,22f,22f); line(8f,9f,14f,9f); line(8f,13f,12f,13f)
            }
            AppDestination.ACTIVITY -> {
                drawCircle(color,10*unit,point(12f,12f),style=stroke); line(12f,6f,12f,12f); line(12f,12f,17f,15f)
            }
            AppDestination.HUB -> {
                drawCircle(color,3*unit,point(12f,12f),style=stroke)
                for (p in listOf(point(4f,4f),point(20f,4f),point(4f,20f),point(20f,20f))) {
                    drawLine(color,point(12f,12f),p,stroke.width); drawCircle(color,2.5f*unit,p,style=stroke)
                }
            }
        }
    }
}

@Composable fun SectionHeading(title: String, subtitle: String? = null) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(title, style = MaterialTheme.typography.headlineMedium, modifier = Modifier.semantics { heading() })
        subtitle?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

@Composable fun QuietPanel(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Surface(modifier.fillMaxWidth(), color = MaterialTheme.colorScheme.surface, shape = MaterialTheme.shapes.medium,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
    }
}

@Composable fun StateNotice(message: String, error: Boolean = false) {
    QuietPanel { Text(message, style = MaterialTheme.typography.bodySmall,
        color = if (error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
}
