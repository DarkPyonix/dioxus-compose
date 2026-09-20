package org.thisisthepy.dioxus.compose.renderer

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
import org.thisisthepy.dioxus.compose.protocol.ColorRole
import org.thisisthepy.dioxus.compose.protocol.ColorScheme
import org.thisisthepy.dioxus.compose.protocol.DesignSystem
import org.thisisthepy.dioxus.compose.protocol.DesignTokenTable
import org.thisisthepy.dioxus.compose.protocol.DesignTokens
import org.thisisthepy.dioxus.compose.protocol.Paint
import org.thisisthepy.dioxus.compose.protocol.ShapeRole
import org.thisisthepy.dioxus.compose.protocol.SpaceRole
import org.thisisthepy.dioxus.compose.protocol.Theme
import org.thisisthepy.dioxus.compose.protocol.TypeRole
import org.thisisthepy.dioxus.compose.protocol.TypeToken

/**
 * The platforms `Theme::adaptive` distinguishes (SPEC FR-14.3).
 *
 * `LinuxGnome`, `LinuxKde` and `LinuxOther` exist because the table in 14.3 maps them to the
 * stage 2 systems; until those exist they resolve to the Host's mandatory fallback.
 */
enum class HostPlatform { Android, MacOs, Ios, Windows, LinuxGnome, LinuxKde, LinuxOther, Web, Unknown }

/**
 * Reads the platform once, the way SPEC FR-14.3 prescribes: the OS first, then
 * `XDG_CURRENT_DESKTOP` and `DESKTOP_SESSION` for the Linux desktop environment.
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
 * The system `adaptive` picks for a platform (SPEC FR-14.3).
 *
 * The stage 2 systems (GNOME 50, KDE Breeze, Deepin) are not implemented, so every Linux
 * desktop and every platform without a design language of its own takes the Host's
 * fallback. FR-14.3 makes that fallback a required argument precisely for this case.
 */
internal fun adaptiveSystem(platform: HostPlatform, fallback: DesignSystem): DesignSystem =
    when (platform) {
        HostPlatform.Android -> DesignSystem.Material3
        HostPlatform.MacOs, HostPlatform.Ios -> DesignSystem.Cupertino
        HostPlatform.Windows -> DesignSystem.Fluent
        // FR-14.3: the browser has no platform look, and Fluent 2 is the documented default.
        HostPlatform.Web -> DesignSystem.Fluent
        HostPlatform.LinuxGnome,
        HostPlatform.LinuxKde,
        HostPlatform.LinuxOther,
        HostPlatform.Unknown,
        -> fallback
    }

/**
 * A design system bound to one colour scheme: the generated token table (FR-14.6 items 1 to
 * 4) plus the component rules the Renderer owns (items 5 to 7).
 */
class ResolvedTheme(
    val system: DesignSystem,
    val tokens: DesignTokenTable,
    val rules: ComponentRules,
    val dark: Boolean,
) {
    fun color(role: ColorRole): Color = Color(tokens.color(role, dark))

    /** A literal paints itself, a role goes through the table (SPEC FR-13.1). */
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
 * FR-13.7 keeps font resources out of the protocol, so the ladder picks between the two
 * families the Renderer always has. The token table's family names are documentation of the
 * guideline, not a font the Host may request.
 */
val TypeToken.family: FontFamily get() = if (monospace) FontFamily.Monospace else FontFamily.Default

/**
 * Items 5 to 7 of the FR-14.6 table: elevation rendering, `ButtonVariant` styling and motion.
 *
 * A fourth design system is one `DesignTokenTable` in the generated protocol plus one
 * implementation of this interface, and nothing else (SPEC FR-14.1).
 */
interface ComponentRules {
    /** How `Modifier::Elevation(dp)` is drawn (FR-13.5, FR-14.6 item 5). */
    fun elevation(
        modifier: androidx.compose.ui.Modifier,
        elevation: Dp,
        shape: Shape,
        theme: ResolvedTheme,
    ): androidx.compose.ui.Modifier

    /** How a `ButtonVariant` looks, resting and pressed (FR-14.2, FR-14.6 item 6). */
    fun button(
        variant: org.thisisthepy.dioxus.compose.protocol.ButtonVariant,
        theme: ResolvedTheme,
    ): ButtonStyle

    /** State transition timing (FR-14.6 item 7). */
    val motion: Motion
}

/** The design system's state transition timing (FR-14.6 item 7). */
data class Motion(
    val pressMillis: Int,
    val releaseMillis: Int,
    val easing: androidx.compose.animation.core.Easing,
)

/**
 * How one button variant is drawn. The pressed values are separate fields rather than a
 * second call, so the press animation can interpolate between the two (FR-14.6 item 7).
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
 * fallback (FR-14.3). A Host that has sent a theme is a different case from one that has
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
 * the readers, so a theme change is O(1) on the wire (SPEC FR-14.4).
 */
val LocalDesignTheme = staticCompositionLocalOf {
    resolveTheme(theme = null, platform = HostPlatform.Unknown, systemDark = false)
}
