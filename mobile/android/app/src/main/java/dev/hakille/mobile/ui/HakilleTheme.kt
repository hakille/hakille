package dev.hakille.mobile

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import hakille.tokens.HakilleTokens as T

enum class Appearance { DARK, LIGHT, SYSTEM }
val LocalHakilleDark = staticCompositionLocalOf { true }
fun tokenColor(value: String) = Color(android.graphics.Color.parseColor(value))

/** Couleurs et métriques issues des tokens ; Inter embarquée et substitutions natives pour les écritures système. */
@OptIn(ExperimentalTextApi::class)
@Composable fun HakilleTheme(appearance: Appearance, content: @Composable () -> Unit) {
    val dark = when (appearance) { Appearance.DARK -> true; Appearance.LIGHT -> false; Appearance.SYSTEM -> isSystemInDarkTheme() }
    fun color(darkValue: String, lightValue: String) = tokenColor(if (dark) darkValue else lightValue)
    val background = color(T.colorSurface0Dark, T.colorSurface0Light)
    val surface = color(T.colorSurface1Dark, T.colorSurface1Light)
    val raised = color(T.colorSurface2Dark, T.colorSurface2Light)
    val text = color(T.colorTexte1Dark, T.colorTexte1Light)
    val secondary = color(T.colorTexte2Dark, T.colorTexte2Light)
    val glow = color(T.colorLueurDark, T.colorLueurLight)
    val line = color(T.colorLigneDark, T.colorLigneLight)
    val base = if (dark) darkColorScheme() else lightColorScheme()
    val colors = base.copy(primary = glow, onPrimary = background, primaryContainer = raised, onPrimaryContainer = text,
        secondary = secondary, onSecondary = background, secondaryContainer = raised, onSecondaryContainer = text,
        tertiary = secondary, onTertiary = background, tertiaryContainer = raised, onTertiaryContainer = text,
        error = text, onError = background, errorContainer = raised, onErrorContainer = text,
        background = background, onBackground = text, surface = surface, onSurface = text,
        surfaceVariant = raised, onSurfaceVariant = secondary, surfaceContainer = surface, surfaceContainerLow = surface, surfaceContainerLowest = background,
        surfaceContainerHigh = raised, surfaceContainerHighest = raised, surfaceTint = glow,
        surfaceDim = background, surfaceBright = raised, inversePrimary = background,
        outline = secondary, outlineVariant = line, inverseSurface = text, inverseOnSurface = background)
    val inter = FontFamily(
        Font(R.font.inter_variable, weight = FontWeight.Normal, variationSettings = FontVariation.Settings(FontVariation.weight(400))),
        Font(R.font.inter_variable, weight = FontWeight.Medium, variationSettings = FontVariation.Settings(FontVariation.weight(500))),
        Font(R.font.inter_variable, weight = FontWeight.SemiBold, variationSettings = FontVariation.Settings(FontVariation.weight(600))))
    fun style(size: Long, lineHeight: Long, weight: Long = 400) = TextStyle(fontFamily = inter,
        fontSize = size.toFloat().sp, lineHeight = lineHeight.toFloat().sp, fontWeight = FontWeight(weight.toInt()))
    val typography = Typography(
        displayLarge = style(T.typeDisplaySize, T.typeDisplayLine, T.typeDisplayWeight),
        displayMedium = style(T.typeDisplaySize, T.typeDisplayLine, T.typeDisplayWeight),
        displaySmall = style(T.typeDisplaySize, T.typeDisplayLine, T.typeDisplayWeight),
        headlineLarge = style(T.typeTitleSize, T.typeTitleLine, T.typeTitleWeight),
        headlineSmall = style(T.typeHeadingSize, T.typeHeadingLine, T.typeHeadingWeight),
        headlineMedium = style(T.typeTitleSize, T.typeTitleLine, T.typeTitleWeight),
        titleLarge = style(T.typeHeadingSize, T.typeHeadingLine, T.typeHeadingWeight),
        titleSmall = style(T.typeBodyStrongSize, T.typeBodyStrongLine, T.typeBodyStrongWeight),
        titleMedium = style(T.typeBodyStrongSize, T.typeBodyStrongLine, T.typeBodyStrongWeight),
        bodyLarge = style(T.typeBodySize, T.typeBodyLine), bodyMedium = style(T.typeBodySize, T.typeBodyLine),
        bodySmall = style(T.typeCaptionSize, T.typeCaptionLine), labelLarge = style(T.typeBodyStrongSize, T.typeBodyStrongLine, T.typeBodyStrongWeight),
        labelMedium = style(T.typeCaptionSize, T.typeCaptionLine, 500), labelSmall = style(T.typeCaptionSize, T.typeCaptionLine))
    CompositionLocalProvider(LocalHakilleDark provides dark) {
        MaterialTheme(colorScheme = colors, typography = typography,
            shapes = Shapes(small = RoundedCornerShape(T.layoutRadiusControl.toInt().dp), medium = RoundedCornerShape(T.layoutRadiusCard.toInt().dp), large = RoundedCornerShape(T.layoutRadiusCard.toInt().dp)), content = content)
    }
}
