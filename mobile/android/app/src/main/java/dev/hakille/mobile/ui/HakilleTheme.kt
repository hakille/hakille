package dev.hakille.mobile

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.border
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import hakille.tokens.HakilleTokens as T

enum class Appearance { DARK, LIGHT, SYSTEM }
val LocalHakilleDark = staticCompositionLocalOf { true }
fun tokenColor(value: String) = Color(android.graphics.Color.parseColor(value))

/** Monospace font family for cryptographic hashes, UUIDs, and currency micro-units */
val HakilleMonoFont: FontFamily = FontFamily.Monospace

/** Deep Obsidian OLED surface and accent palette accessors */
object HakillePalette {
    val surface0Dark: Color get() = T.colorSurface0Dark
    val surface0Light: Color get() = T.colorSurface0Light
    val surface1Dark: Color get() = T.colorSurface1Dark
    val surface1Light: Color get() = T.colorSurface1Light
    val surface2Dark: Color get() = T.colorSurface2Dark
    val surface2Light: Color get() = T.colorSurface2Light
    val ligneDark: Color get() = T.colorLigneDark
    val ligneLight: Color get() = T.colorLigneLight
    val lueurDark: Color get() = T.colorLueurDark
    val lueurLight: Color get() = T.colorLueurLight
    val texte1Dark: Color get() = T.colorTexte1Dark
    val texte1Light: Color get() = T.colorTexte1Light
    val texte2Dark: Color get() = T.colorTexte2Dark
    val texte2Light: Color get() = T.colorTexte2Light
    val verreFondDark: Color get() = T.colorVerreFondDark
    val verreFondLight: Color get() = T.colorVerreFondLight
    val verreRefletDark: Color get() = T.colorVerreRefletDark
    val verreRefletLight: Color get() = T.colorVerreRefletLight

    /** Strictly reserved for validation, 600ms hold-progress, and decision-ring */
    val braiseDark: Color get() = T.colorBraiseDark
    val braiseLight: Color get() = T.colorBraiseLight
    val braiseHaloDark: Color get() = T.colorBraiseHaloDark
    val braiseHaloLight: Color get() = T.colorBraiseHaloLight

    val erreurDark: Color get() = T.colorErreurDark
    val erreurLight: Color get() = T.colorErreurLight
    val succesDark: Color get() = T.colorSuccesDark
    val succesLight: Color get() = T.colorSuccesLight
    val avertissementDark: Color get() = T.colorAvertissementDark
    val avertissementLight: Color get() = T.colorAvertissementLight
}

/** Hairline border modifier conforming to tokens/color.toml */
fun Modifier.hairlineBorder(
    color: Color = HakillePalette.ligneDark,
    width: Dp = 1.dp,
    shape: Shape = RoundedCornerShape(T.layoutRadiusCard.toInt().dp)
): Modifier = this.border(BorderStroke(width, color), shape)

/** Strict touch target modifier enforcing minimum 48dp on Android */
fun Modifier.minTouchTarget(
    min: Dp = T.layoutTouchPtAndroid.toInt().dp
): Modifier = this.sizeIn(minWidth = min, minHeight = min)

/** Glassmorphic acrylic blur modifiers conforming to tokens/motion.toml blur tokens */
fun Modifier.acrylicHud(): Modifier = this.blur(T.blurHudAcrylic.toInt().dp)
fun Modifier.acrylicPanel(): Modifier = this.blur(T.blurPanel.toInt().dp)
fun Modifier.acrylicGlow(): Modifier = this.blur(T.blurGlow.toInt().dp)

/** Tabular monospace style for audit hashes, addresses, and currency micro-units */
fun hakilleMonoStyle(
    size: Long = T.typeMonoSize,
    lineHeight: Long = T.typeMonoLine,
    weight: Long = T.typeMonoWeight
): TextStyle = TextStyle(
    fontFamily = HakilleMonoFont,
    fontSize = size.toFloat().sp,
    lineHeight = lineHeight.toFloat().sp,
    fontWeight = FontWeight(weight.toInt())
)

/** Couleurs et métriques issues des tokens ; Inter embarquée et substitutions natives pour les écritures système. */
@OptIn(ExperimentalTextApi::class)
@Composable fun HakilleTheme(appearance: Appearance, content: @Composable () -> Unit) {
    val dark = when (appearance) { Appearance.DARK -> true; Appearance.LIGHT -> false; Appearance.SYSTEM -> isSystemInDarkTheme() }
    fun color(darkValue: String, lightValue: String) = tokenColor(if (dark) darkValue else lightValue)
    val background = color(T.colorSurface0DarkHex, T.colorSurface0LightHex)
    val surface = color(T.colorSurface1DarkHex, T.colorSurface1LightHex)
    val raised = color(T.colorSurface2DarkHex, T.colorSurface2LightHex)
    val text = color(T.colorTexte1DarkHex, T.colorTexte1LightHex)
    val secondary = color(T.colorTexte2DarkHex, T.colorTexte2LightHex)
    val glow = color(T.colorLueurDarkHex, T.colorLueurLightHex)
    val line = color(T.colorLigneDarkHex, T.colorLigneLightHex)
    val base = if (dark) darkColorScheme() else lightColorScheme()
    val colors = base.copy(
        primary = glow, onPrimary = background, primaryContainer = raised, onPrimaryContainer = text,
        secondary = secondary, onSecondary = background, secondaryContainer = raised, onSecondaryContainer = text,
        tertiary = secondary, onTertiary = background, tertiaryContainer = raised, onTertiaryContainer = text,
        error = text, onError = background, errorContainer = raised, onErrorContainer = text,
        background = background, onBackground = text, surface = surface, onSurface = text,
        surfaceVariant = raised, onSurfaceVariant = secondary, surfaceContainer = surface, surfaceContainerLow = surface, surfaceContainerLowest = background,
        surfaceContainerHigh = raised, surfaceContainerHighest = raised, surfaceTint = glow,
        surfaceDim = background, surfaceBright = raised, inversePrimary = background,
        outline = secondary, outlineVariant = line, inverseSurface = text, inverseOnSurface = background
    )
    val inter = FontFamily(
        Font(R.font.inter_variable, weight = FontWeight.Normal, variationSettings = FontVariation.Settings(FontVariation.weight(400))),
        Font(R.font.inter_variable, weight = FontWeight.Medium, variationSettings = FontVariation.Settings(FontVariation.weight(500))),
        Font(R.font.inter_variable, weight = FontWeight.SemiBold, variationSettings = FontVariation.Settings(FontVariation.weight(600)))
    )
    fun style(size: Long, lineHeight: Long, weight: Long = 400) = TextStyle(
        fontFamily = inter,
        fontSize = size.toFloat().sp,
        lineHeight = lineHeight.toFloat().sp,
        fontWeight = FontWeight(weight.toInt())
    )
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
        bodyLarge = style(T.typeBodySize, T.typeBodyLine),
        bodyMedium = style(T.typeBodySize, T.typeBodyLine),
        bodySmall = style(T.typeCaptionSize, T.typeCaptionLine),
        labelLarge = style(T.typeBodyStrongSize, T.typeBodyStrongLine, T.typeBodyStrongWeight),
        labelMedium = style(T.typeCaptionSize, T.typeCaptionLine, 500),
        labelSmall = style(T.typeCaptionSize, T.typeCaptionLine)
    )
    CompositionLocalProvider(LocalHakilleDark provides dark) {
        MaterialTheme(
            colorScheme = colors,
            typography = typography,
            shapes = Shapes(
                small = RoundedCornerShape(T.layoutRadiusControl.toInt().dp),
                medium = RoundedCornerShape(T.layoutRadiusCard.toInt().dp),
                large = RoundedCornerShape(T.layoutRadiusCard.toInt().dp)
            ),
            content = content
        )
    }
}
