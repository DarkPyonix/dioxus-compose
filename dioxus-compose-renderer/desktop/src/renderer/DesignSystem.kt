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
 * A GNOME session takes Adwaita. The remaining Linux desktops, and any platform without a
 * design language of its own, take the Host's fallback. That is why `Theme::adaptive`
 * makes the fallback a required argument: there is no platform for which adaptive has
 * nothing to choose.
 */
internal fun adaptiveSystem(platform: HostPlatform, fallback: DesignSystem): DesignSystem =
    when (platform) {
        HostPlatform.Android -> DesignSystem.Material3
        HostPlatform.MacOs, HostPlatform.Ios -> DesignSystem.Cupertino
        HostPlatform.Windows -> DesignSystem.Fluent
        // A browser has no design language of its own, so the choice is arbitrary; Fluent 2
        // is the documented default and an app can say `unified` to be explicit.
        HostPlatform.Web -> DesignSystem.Fluent
        HostPlatform.LinuxGnome -> DesignSystem.Gnome
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

    /**
     * How a container or overlay looks. The widget says which kind of container it is and
     * nothing else: the background, the corner, the resting height, the scrim behind a
     * modal and the hairline under a bar are all decided here.
     */
    fun container(role: ContainerRole, theme: ResolvedTheme): ContainerStyle

    /** How a tab strip and its selection indicator look. */
    fun tabs(theme: ResolvedTheme): TabsStyle

    /**
     * The metrics this system's icon set is drawn to.
     *
     * The Host registers a meaning, never a picture or a system icon name, so the artwork
     * is chosen here. That is what makes one declaration come out as the shape Cupertino
     * draws and as the shape Material 3 draws, without the Host knowing either.
     */
    fun icon(role: dioxus.compose.protocol.IconRole, theme: ResolvedTheme): IconStyle

    /**
     * How this system asks for a date, a time and a choice.
     *
     * The three systems do not merely style their pickers differently, they are operated
     * differently: a calendar grid, a wheel, a flyout. The widget carries a value, a range
     * and a change event, so the way of picking is decided here and nowhere else.
     */
    val pickers: PickerRules

    /** State transition timing. Motion is a design system rule, not a Host parameter. */
    val motion: Motion
}

/** The design system's state transition timing. */
data class Motion(
    val pressMillis: Int,
    val releaseMillis: Int,
    val easing: androidx.compose.animation.core.Easing,
    /** How long a pointer rests on something before its explanation appears. */
    val tooltipDelayMillis: Int = 500,
)

/**
 * The kinds of container the core vocabulary has. A widget emits the role; which colour,
 * corner and height that role means belongs to the design system.
 */
enum class ContainerRole { Card, Surface, TopAppBar, Dialog, Menu, Tooltip }

/**
 * How one container role is drawn.
 *
 * `scrim` is what covers what is behind a modal, and is transparent for the roles that
 * cover nothing. `separator` is the hairline a bar draws under itself, null where the
 * system draws none.
 */
data class ContainerStyle(
    val container: Color,
    val content: Color,
    val shape: Shape,
    val elevation: Dp,
    val borderWidth: Dp,
    val borderColor: Color,
    val horizontalPadding: Dp,
    val verticalPadding: Dp,
    val separator: Color?,
    val scrim: Color,
    val typeRole: TypeRole,
)

/**
 * How a tab strip is drawn.
 *
 * The three systems disagree about what marks the selection: Material underlines the tab,
 * Cupertino fills the selected segment, Fluent draws a short bar under the label. All three
 * are expressed here, so the widget only has to know which tab is selected.
 */
data class TabsStyle(
    val container: Color,
    val shape: Shape,
    val selectedContent: Color,
    val unselectedContent: Color,
    val selectedContainer: Color,
    val selectedShape: Shape,
    val indicator: Color,
    val indicatorHeight: Dp,
    val indicatorShape: Shape,
    /** True where the mark spans the whole tab, false where it is a short centred bar. */
    val indicatorFillsTab: Boolean,
    val horizontalPadding: Dp,
    val verticalPadding: Dp,
    val typeRole: TypeRole,
)

/**
 * The metrics one icon is drawn to.
 *
 * The geometry of each role is shared, because `Back` means the same thing everywhere. What
 * differs is how it is drawn: Material's heavier stem with flat ends, Cupertino's thin
 * stroke with rounded ends, Fluent's lighter stroke with square ends, each at its own
 * optical size.
 */
data class IconStyle(
    val size: Dp,
    val strokeWidth: Dp,
    val cap: androidx.compose.ui.graphics.StrokeCap,
    val join: androidx.compose.ui.graphics.StrokeJoin,
)

/** The way a date is picked. */
enum class DatePresentation {
    /** A month laid out as a grid of days. */
    CalendarGrid,

    /** Scrolling wheels, one per field. */
    Wheel,

    /** A field that opens a calendar over the page. */
    CalendarFlyout,
}

/** The way a time of day is picked. */
enum class TimePresentation {
    /** A clock face the hand is dragged around. */
    Dial,

    /** Scrolling wheels for the hour and the minute. */
    Wheel,

    /** A field with the hour and the minute stepped up and down. */
    Stepper,
}

/** The way one item out of a list is picked. */
enum class ChoicePresentation {
    /** A field that drops a menu below itself. */
    ExposedMenu,

    /** A wheel of the options, with the chosen one in the middle. */
    Wheel,

    /** A field that opens a flyout list over the page. */
    ComboBox,
}

/**
 * Which of the ways of picking this design system uses.
 *
 * There is no Host property that can override any of these. A widget that could ask for a
 * wheel would stop being a date and start being a piece of interface design, and the
 * system's own conventions would be the thing that loses.
 */
data class PickerRules(
    val date: DatePresentation,
    val time: TimePresentation,
    val choice: ChoicePresentation,
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
    DesignSystem.Gnome -> GnomeRules
}

/**
 * The theme every interpreted node reads. `SetTheme` changes it once and Compose invalidates
 * the readers, so a theme change is one record on the wire rather than a `SetProp` for
 * every node in the tree.
 */
val LocalDesignTheme = staticCompositionLocalOf {
    resolveTheme(theme = null, platform = HostPlatform.Unknown, systemDark = false)
}
