package dioxus.compose.design

import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.TextUnit
import dioxus.compose.protocol.ColorRole
import dioxus.compose.protocol.ColorScheme
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.DesignTokenTable
import dioxus.compose.protocol.DesignTokens
import dioxus.compose.protocol.Paint
import dioxus.compose.protocol.ShapeRole
import dioxus.compose.protocol.SpaceRole
import dioxus.compose.protocol.Theme
import dioxus.compose.protocol.TypeRole
import dioxus.compose.protocol.TypeToken
import java.lang.System

/**
 * The platforms `Theme::adaptive` distinguishes.
 *
 * `LinuxGnome`, `LinuxKde` and `LinuxOther` exist because each maps to a design system of
 * its own (GNOME 50, KDE Breeze, Deepin); until those are implemented they resolve to the
 * Host's mandatory fallback.
 */
enum class HostPlatform { Android, MacOs, Ios, Windows, LinuxGnome, LinuxKde, LinuxOther, Web, Unknown }

/**
 * Reads the platform once, at startup: the OS first, then `XDG_CURRENT_DESKTOP` and, if
 * that is empty, `DESKTOP_SESSION` for the Linux desktop environment.
 */
internal fun detectHostPlatform(
    osName: String = System.getProperty("os.name").orEmpty(),
    androidRuntime: String? = System.getProperty("java.vm.vendor"),
    xdgCurrentDesktop: String? = System.getenv("XDG_CURRENT_DESKTOP"),
    desktopSession: String? = System.getenv("DESKTOP_SESSION"),
): HostPlatform {
    val os = osName.lowercase()
    return when {
        androidRuntime?.contains("Android", ignoreCase = true) == true -> HostPlatform.Android
        os.contains("mac") || os.contains("darwin") -> HostPlatform.MacOs
        os.contains("ios") -> HostPlatform.Ios
        os.contains("win") -> HostPlatform.Windows
        os.contains("linux") || os.contains("nix") || os.contains("bsd") ->
            linuxDesktop(xdgCurrentDesktop, desktopSession)
        else -> HostPlatform.Unknown
    }
}

private fun linuxDesktop(xdgCurrentDesktop: String?, desktopSession: String?): HostPlatform {
    val value = xdgCurrentDesktop?.takeIf { it.isNotBlank() } ?: desktopSession.orEmpty()
    return when {
        value.contains("GNOME", ignoreCase = true) -> HostPlatform.LinuxGnome
        value.contains("KDE", ignoreCase = true) -> HostPlatform.LinuxKde
        else -> HostPlatform.LinuxOther
    }
}

/**
 * The system `adaptive` picks for a platform.
 *
 * The stage 2 systems (GNOME 50, KDE Breeze, Deepin) are not implemented, so every Linux
 * desktop and every platform without a design language of its own takes the Host's
 * fallback. That is why `Theme::adaptive` makes the fallback a required argument: there is
 * no platform for which adaptive has nothing to choose.
 */
internal fun adaptiveSystem(platform: HostPlatform, fallback: DesignSystem): DesignSystem =
    when (platform) {
        HostPlatform.Android -> DesignSystem.Material3
        HostPlatform.MacOs, HostPlatform.Ios -> DesignSystem.Cupertino
        HostPlatform.Windows -> DesignSystem.Fluent
        // A browser has no design language of its own, so the choice is arbitrary; Fluent 2
        // is the documented default and an app can say `unified` to be explicit.
        HostPlatform.Web -> DesignSystem.Fluent
        HostPlatform.LinuxGnome,
        HostPlatform.LinuxKde,
        HostPlatform.LinuxOther,
        HostPlatform.Unknown,
        -> fallback
    }

/**
 * A design system bound to one colour scheme: the generated token table (colours, type,
 * shapes, spacing) plus the component rules the Renderer owns (elevation, button variants,
 * motion).
 */
class ResolvedTheme(
    val system: DesignSystem,
    val tokens: DesignTokenTable,
    val rules: ComponentRules,
    val dark: Boolean,
) {
    fun color(role: ColorRole): Color = Color(tokens.color(role, dark))

    /** A literal paints itself, a role goes through the table. */
    fun color(paint: Paint): Color = when (paint) {
        is Paint.Literal -> Color(paint.argb)
        is Paint.Role -> color(paint.role)
    }

    fun space(role: SpaceRole): Dp = tokens.space(role).dp

    fun radius(role: ShapeRole): Dp = tokens.radius(role).dp

    /** `Full` is stored as a very large radius, which is a pill at any height. */
    fun shape(role: ShapeRole): Shape = roundedShape(tokens.radius(role))

    fun type(role: TypeRole): TypeToken = tokens.type(role)

    companion object {
        internal fun roundedShape(radius: Float): Shape =
            if (radius >= FULL_RADIUS) RoundedCornerShape(percent = 50) else RoundedCornerShape(radius.dp)

        private const val FULL_RADIUS = 1000.0f
    }
}

val TypeToken.fontSize: TextUnit get() = size.sp
val TypeToken.composeWeight: FontWeight get() = FontWeight(weight)
val TypeToken.composeLineHeight: TextUnit get() = lineHeight.sp
val TypeToken.composeLetterSpacing: TextUnit get() = letterSpacing.sp

/**
 * Font resources deliberately do not cross the protocol: a Host that named a font would
 * push the check that it exists out to run time. So the ladder picks between the two
 * families the Renderer always has. The token table's family names are documentation of the
 * guideline, not a font the Host may request.
 */
val TypeToken.family: FontFamily get() = if (monospace) FontFamily.Monospace else FontFamily.Default

/**
 * The hand-written half of a design system: elevation rendering, `ButtonVariant` styling
 * and motion.
 *
 * A fourth design system is one `DesignTokenTable` in the generated protocol plus one
 * implementation of this interface, and nothing else. No widget, property, modifier or wire
 * format changes.
 */
interface ComponentRules {
    /** How `Modifier::Elevation(dp)` is drawn. The Host sends a dp value and nothing else. */
    fun elevation(
        modifier: androidx.compose.ui.Modifier,
        elevation: Dp,
        shape: Shape,
        theme: ResolvedTheme,
    ): androidx.compose.ui.Modifier

    /** How a `ButtonVariant` looks, resting and pressed. */
    fun button(
        variant: dioxus.compose.protocol.ButtonVariant,
        theme: ResolvedTheme,
    ): ButtonStyle

    /** State transition timing. Motion is a design system rule, not a Host parameter. */
    val motion: Motion
}

/** The design system's state transition timing. */
data class Motion(
    val pressMillis: Int,
    val releaseMillis: Int,
    val easing: androidx.compose.animation.core.Easing,
)

/**
 * How one button variant is drawn. The pressed values are separate fields rather than a
 * second call, so the press animation can interpolate between the two.
 */
data class ButtonStyle(
    val container: Color,
    val pressedContainer: Color,
    val content: Color,
    val pressedContentAlpha: Float,
    val borderWidth: Dp,
    val borderColor: Color,
    val pressedBorderColor: Color,
    /** Fluent's lighter top edge; null where the system has no such stroke. */
    val topHighlight: Color?,
    val shape: Shape,
    val horizontalPadding: Dp,
    val verticalPadding: Dp,
    val minHeight: Dp,
    val typeRole: TypeRole,
    val restElevation: Dp,
    val pressedElevation: Dp,
)

/**
 * Resolves the Host's `SetTheme` into the table and rules used for this frame.
 *
 * The Host's choice is read, never second-guessed: `adaptive` follows the platform only
 * because the Host said `adaptive = true`.
 *
 * With no `SetTheme` at all the default follows the platform, with Material 3 as the
 * fallback. A Host that has sent a theme is a different case from one that has
 * not said anything yet, and only the second is this default.
 */
fun resolveTheme(
    theme: Theme?,
    platform: HostPlatform,
    systemDark: Boolean,
): ResolvedTheme {
    val system = when {
        theme == null -> adaptiveSystem(platform, DesignSystem.Material3)
        theme.adaptive -> adaptiveSystem(platform, theme.fallback)
        else -> theme.designSystem
    }
    val dark = when (theme?.colorScheme ?: ColorScheme.FollowSystem) {
        ColorScheme.Light -> false
        ColorScheme.Dark -> true
        ColorScheme.FollowSystem -> systemDark
    }
    return ResolvedTheme(system, DesignTokens.of(system), rulesFor(system), dark)
}

internal fun rulesFor(system: DesignSystem): ComponentRules = when (system) {
    DesignSystem.Material3 -> Material3Rules
    DesignSystem.Cupertino -> CupertinoRules
    DesignSystem.Fluent -> FluentRules
}

/**
 * The theme every interpreted node reads. `SetTheme` changes it once and Compose invalidates
 * the readers, so a theme change is one record on the wire rather than a `SetProp` for
 * every node in the tree.
 */
val LocalDesignTheme = staticCompositionLocalOf {
    resolveTheme(theme = null, platform = HostPlatform.Unknown, systemDark = false)
}
