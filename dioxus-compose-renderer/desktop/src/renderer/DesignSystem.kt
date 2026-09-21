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
import dioxus.compose.protocol.WindowSizeClass
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
 * A GNOME session takes Adwaita, a KDE session takes Breeze, and any other Linux session
 * takes Deepin. A platform with no design language of its own takes the Host's fallback. That is why `Theme::adaptive`
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
        HostPlatform.LinuxKde -> DesignSystem.Breeze
        // Any other Linux session, and one that cannot be identified at all, takes
        // Deepin. That is the job it was written for.
        HostPlatform.LinuxOther -> DesignSystem.Deepin
        HostPlatform.Unknown -> fallback
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
     * How the selection controls and the two indicators are drawn.
     *
     * One call for the six of them because they are one family: something that holds a
     * state, something that shows it, and a reaction to being touched. A widget sends
     * whether it is on, where it sits between two ends, or which way it runs. The size of
     * the box, the shape of the tick, the dimensions of a track and a thumb, the speed of
     * an indeterminate sweep and the weight of a rule are all decided here, which is why
     * the same declaration comes out as a Material checkbox and as a Cupertino one.
     */
    fun controls(theme: ResolvedTheme): ControlsStyle

    /**
     * Who draws those six.
     *
     * The default draws them from [controls], so a new design system implements `controls()`
     * and nothing else. Material 3 overrides this because `androidx.compose.material3`
     * already draws Material's controls, and a copy of them would only drift from the
     * specification it claims to follow.
     */
    val controlWidgets: ControlWidgets get() = DrawnControlWidgets

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

    // The three answers below have defaults, and that is the point of them. A design
    // system implements what it has an opinion about; what it says nothing about is
    // derived from its own token table, so it still comes out in its own colours rather
    // than in someone else's. A seventh design system is one more implementation of this
    // interface, and it does not stop compiling when the vocabulary grows again.

    /**
     * How a set of destinations is presented at this width, and what it is drawn with.
     *
     * The width decides between a bar across the bottom, a rail down the side and a
     * drawer standing open, because that is the correspondence every one of these design
     * systems already makes. The Host declared one navigation and said nothing about
     * which of the three it wanted.
     */
    fun navigation(sizeClass: WindowSizeClass, theme: ResolvedTheme): NavigationStyle =
        NavigationStyle(
            presentation = when (sizeClass) {
                WindowSizeClass.Compact -> NavigationPresentation.Bar
                WindowSizeClass.Medium -> NavigationPresentation.Rail
                WindowSizeClass.Expanded -> NavigationPresentation.Drawer
            },
            container = theme.color(ColorRole.SurfaceContainer),
            content = theme.color(ColorRole.OnSurfaceVariant),
            selectedContent = theme.color(ColorRole.Primary),
            indicator = Color.Transparent,
            indicatorShape = theme.shape(ShapeRole.Full),
            indicatorKind = NavigationIndicator.None,
            separator = theme.color(ColorRole.OutlineVariant),
            barHeight = 64.dp,
            railWidth = 80.dp,
            drawerWidth = 240.dp,
            itemSpacing = theme.space(SpaceRole.Xs),
            itemPadding = theme.space(SpaceRole.Sm),
            labelInRail = true,
            typeRole = TypeRole.Label,
        )

    /**
     * Which edge a sheet enters from at this width, and what it is drawn with.
     *
     * Up from the bottom where the window is narrow, in from the side where it is wide:
     * the same convention in all of these systems, and the same reason as the navigation
     * presentation. The Host has no property that could ask for one of them.
     */
    fun sheet(sizeClass: WindowSizeClass, theme: ResolvedTheme): SheetStyle = SheetStyle(
        edge = if (sizeClass == WindowSizeClass.Compact) SheetEdge.Bottom else SheetEdge.End,
        container = theme.color(ColorRole.SurfaceContainer),
        content = theme.color(ColorRole.OnSurface),
        shape = theme.shape(ShapeRole.Large),
        elevation = 6.dp,
        scrim = Color.Black.copy(alpha = 0.32f),
        handle = theme.color(ColorRole.OutlineVariant),
        widthFraction = 0.4f,
        heightFraction = 0.5f,
        padding = theme.space(SpaceRole.Lg),
        borderWidth = 0.dp,
        borderColor = Color.Transparent,
    )

    /**
     * How a transient message is drawn, and how long each of the two durations lasts.
     *
     * The milliseconds are here rather than on the wire because they are a rule of the
     * design language: Material's four and ten seconds are not Apple's, and a Host that
     * sent a number would be overruling whichever system it ended up under.
     */
    fun message(theme: ResolvedTheme): MessageStyle = MessageStyle(
        container = theme.color(ColorRole.OnSurface),
        content = theme.color(ColorRole.Surface),
        actionContent = theme.color(ColorRole.Primary),
        shape = theme.shape(ShapeRole.Small),
        elevation = 3.dp,
        placement = MessagePlacement.BottomCenter,
        horizontalPadding = theme.space(SpaceRole.Md),
        verticalPadding = theme.space(SpaceRole.Sm),
        inset = theme.space(SpaceRole.Md),
        shortMillis = 4_000,
        longMillis = 10_000,
        borderWidth = 0.dp,
        borderColor = Color.Transparent,
        typeRole = TypeRole.Body,
    )
}

/**
 * What marks the selected destination.
 *
 * The three systems disagree, visibly: Material puts a filled pill behind the icon, Fluent
 * draws a short bar along the leading edge of the row, and a Cupertino tab bar marks the
 * selection with colour alone. A widget that only knew "selected" could not produce any of
 * the three on purpose.
 */
enum class NavigationIndicator { Pill, LeadingEdgeBar, None }

/** Which of its three shapes a set of destinations has taken. */
enum class NavigationPresentation {
    /** A bar across the bottom of the window, destinations side by side. */
    Bar,

    /** A narrow column down the leading edge. */
    Rail,

    /** A wide column down the leading edge, labels beside their icons. */
    Drawer,
}

/**
 * How one presentation of a set of destinations is drawn.
 *
 * All three presentations are described by one style, because they are one concept: what
 * changes between them is where the strip sits and how much room a destination gets, not
 * what a destination is.
 */
data class NavigationStyle(
    val presentation: NavigationPresentation,
    val container: Color,
    val content: Color,
    val selectedContent: Color,
    /** What is drawn behind or beside the selected destination. */
    val indicator: Color,
    val indicatorShape: Shape,
    val indicatorKind: NavigationIndicator,
    /** The hairline between the destinations and the screen, null where there is none. */
    val separator: Color?,
    val barHeight: Dp,
    val railWidth: Dp,
    val drawerWidth: Dp,
    val itemSpacing: Dp,
    val itemPadding: Dp,
    /** Whether a rail, which is narrow, still writes the label under the icon. */
    val labelInRail: Boolean,
    val typeRole: TypeRole,
)

/** The edge a sheet comes in from. */
enum class SheetEdge { Bottom, End }

/**
 * How a sheet is drawn, and where it comes from.
 *
 * Nothing here is sent by the Host, including the edge: the Renderer measured the window,
 * so the Renderer decides.
 */
data class SheetStyle(
    val edge: SheetEdge,
    val container: Color,
    val content: Color,
    val shape: Shape,
    val elevation: Dp,
    val scrim: Color,
    /** The grab handle a sheet draws along its dragging edge, null where none is drawn. */
    val handle: Color?,
    /** How much of the window a sheet entering from the side takes. */
    val widthFraction: Float,
    /** How much of the window a sheet entering from the bottom takes. */
    val heightFraction: Float,
    val padding: Dp,
    val borderWidth: Dp,
    val borderColor: Color,
)

/** Where a transient message appears. */
enum class MessagePlacement { BottomStart, BottomCenter, TopEnd }

/**
 * How a transient message is drawn and how long it stays.
 *
 * The durations are part of this because they are part of the design language, and because
 * the alternative is the Host holding a timer for an animation it cannot see.
 */
data class MessageStyle(
    val container: Color,
    val content: Color,
    val actionContent: Color,
    val shape: Shape,
    val elevation: Dp,
    val placement: MessagePlacement,
    val horizontalPadding: Dp,
    val verticalPadding: Dp,
    /** How far the message sits from the window's edges. */
    val inset: Dp,
    val shortMillis: Int,
    val longMillis: Int,
    val borderWidth: Dp,
    val borderColor: Color,
    val typeRole: TypeRole,
)

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

/** Which of the three toggles is being drawn. */
enum class ToggleRole { Checkbox, RadioButton, Switch }

/**
 * How one toggle is drawn, in both of the states it holds.
 *
 * The unchecked and checked colours are separate fields rather than a second call so the
 * transition between them can be interpolated.
 */
data class ToggleStyle(
    /** The control's own footprint, before anything placed beside it. */
    val size: Dp,
    /** The box, circle or track that holds the state. */
    val container: Color,
    val containerChecked: Color,
    /** The tick, the dot or the thumb that shows it. */
    val mark: Color,
    val markUnchecked: Color,
    val border: Color,
    val borderWidth: Dp,
    val shape: Shape,
    /** A switch's travelling thumb. The other two roles leave it at zero. */
    val thumbSize: Dp,
    val trackWidth: Dp,
    val trackHeight: Dp,
    val disabledAlpha: Float,
)

/**
 * How a slider's track, its filled part and its thumb are drawn.
 *
 * `tick` is null where the system marks no discrete stops, which is the difference
 * between a Material slider and a Cupertino one at a glance.
 */
data class SliderStyle(
    val trackHeight: Dp,
    val track: Color,
    val activeTrack: Color,
    val thumbSize: Dp,
    val thumb: Color,
    val thumbBorder: Color,
    val thumbBorderWidth: Dp,
    val tick: Color?,
)

/** How progress is shown, as a bar or as a ring. */
data class ProgressStyle(
    val thickness: Dp,
    val track: Color,
    val indicator: Color,
    /** Diameter when circular. */
    val diameter: Dp,
    val rounded: Boolean,
    /** One full sweep of the indeterminate animation. */
    val periodMillis: Int,
)

/** A rule between two things: thin and full width in some systems, inset in others. */
data class DividerStyle(
    val thickness: Dp,
    val color: Color,
    /** How far it is held back from the leading edge. */
    val inset: Dp,
)

/** Everything the selection controls and the indicators need, answered in one call. */
data class ControlsStyle(
    val checkbox: ToggleStyle,
    val radioButton: ToggleStyle,
    val switch: ToggleStyle,
    val slider: SliderStyle,
    val progress: ProgressStyle,
    val divider: DividerStyle,
) {
    fun toggle(role: ToggleRole): ToggleStyle = when (role) {
        ToggleRole.Checkbox -> checkbox
        ToggleRole.RadioButton -> radioButton
        ToggleRole.Switch -> switch
    }
}

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
    /**
     * How much of the button is left when it is disabled.
     *
     * One number for the whole control rather than a disabled colour per variant: four
     * more colours is four more answers every design system owes, and a seventh system
     * would owe them too. Fading the drawn button says the same thing in every system's
     * own palette, because it is that system's palette being faded.
     */
    val disabledAlpha: Float,
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
    DesignSystem.Breeze -> BreezeRules
    DesignSystem.Deepin -> DeepinRules
}

/**
 * The theme every interpreted node reads. `SetTheme` changes it once and Compose invalidates
 * the readers, so a theme change is one record on the wire rather than a `SetProp` for
 * every node in the tree.
 */
val LocalDesignTheme = staticCompositionLocalOf {
    resolveTheme(theme = null, platform = HostPlatform.Unknown, systemDark = false)
}
