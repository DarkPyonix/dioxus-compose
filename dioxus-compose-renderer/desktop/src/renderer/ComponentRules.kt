package dioxus.compose.design

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dioxus.compose.protocol.ButtonVariant
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import dioxus.compose.protocol.ColorRole
import dioxus.compose.protocol.IconRole
import dioxus.compose.protocol.ShapeRole
import dioxus.compose.protocol.SpaceRole
import dioxus.compose.protocol.TypeRole
import dioxus.compose.protocol.WindowSizeClass

/**
 * Material 3 rules: elevation, button variants and motion.
 *
 * Reference: m3.material.io, "Elevation", "Buttons" and "Motion easing and duration",
 * 2024 baseline, the same revision the generated token table cites.
 *
 * Elevation is tonal plus a shadow, buttons are fully rounded with a state layer that grows
 * on press, and motion uses the emphasised easing.
 */
internal object Material3Rules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // Material 3 raises the surface tone with height as well as casting a shadow. The
        // tint is drawn first so a node that also sets its own Background still wins.
        val tint = lerp(
            theme.color(ColorRole.Surface),
            theme.color(ColorRole.Primary),
            (elevation.value / TONE_FULL_DP).coerceIn(0f, MAX_TONE),
        )
        return modifier.shadow(elevation, shape).background(tint, shape)
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val outline = theme.color(ColorRole.Outline)
        val base = ButtonStyle(
            container = Color.Transparent,
            pressedContainer = Color.Transparent,
            content = theme.color(ColorRole.Primary),
            // The state layer, not opacity, is Material's press feedback, so the label
            // keeps its full opacity.
            pressedContentAlpha = 1f,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            pressedBorderColor = Color.Transparent,
            topHighlight = null,
            // Material 3 buttons are pills.
            shape = theme.shape(ShapeRole.Full),
            horizontalPadding = theme.space(SpaceRole.Lg),
            verticalPadding = theme.space(SpaceRole.Sm) + 2.dp,
            minHeight = 40.dp,
            typeRole = TypeRole.Label,
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            ButtonVariant.Filled -> base.copy(
                container = theme.color(ColorRole.Primary),
                pressedContainer = stateLayer(theme.color(ColorRole.Primary), theme.color(ColorRole.OnPrimary)),
                content = theme.color(ColorRole.OnPrimary),
                // A pressed filled button rises: Material's feedback is tonal and spatial.
                pressedElevation = 2.dp,
            )

            ButtonVariant.Tonal -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                pressedContainer = stateLayer(
                    theme.color(ColorRole.SurfaceVariant),
                    theme.color(ColorRole.OnSurfaceVariant),
                ),
                content = theme.color(ColorRole.OnSurfaceVariant),
                pressedElevation = 1.dp,
            )

            ButtonVariant.Outlined -> base.copy(
                pressedContainer = stateLayer(Color.Transparent, theme.color(ColorRole.Primary)),
                borderWidth = 1.dp,
                borderColor = outline,
                pressedBorderColor = outline,
            )

            ButtonVariant.Text -> base.copy(
                pressedContainer = stateLayer(Color.Transparent, theme.color(ColorRole.Primary)),
            )
        }
    }

    /** Material's pressed state layer: the content colour at 12% over the container. */
    private fun stateLayer(container: Color, content: Color): Color =
        lerp(container, content.copy(alpha = 1f), STATE_LAYER_ALPHA)
            .copy(alpha = if (container.alpha == 0f) STATE_LAYER_ALPHA else container.alpha)

    /**
     * Material's containers: the card is a tonal surface with a large corner, the dialog
     * and the menu rise above the screen, and the tooltip inverts the surface so it reads
     * as an overlay rather than part of the page.
     */
    override fun container(role: ContainerRole, theme: ResolvedTheme): ContainerStyle {
        val base = ContainerStyle(
            container = theme.color(ColorRole.Surface),
            content = theme.color(ColorRole.OnSurface),
            shape = theme.shape(ShapeRole.Medium),
            elevation = 0.dp,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            horizontalPadding = theme.space(SpaceRole.Md),
            verticalPadding = theme.space(SpaceRole.Md),
            separator = null,
            scrim = Color.Transparent,
            typeRole = TypeRole.Body,
        )
        return when (role) {
            ContainerRole.Card -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                content = theme.color(ColorRole.OnSurfaceVariant),
                shape = theme.shape(ShapeRole.Large),
                elevation = 1.dp,
            )

            // A `Surface` is a panel: a layer raised off the page, holding the page's own
            // reading ink. That is `SurfaceContainer`, not `Surface`. `Surface` is the base
            // reading colour, and Material 3 gives it the same value as `Background` on
            // purpose, so a panel painted with it was drawn full size, in the right colour,
            // and could not be seen against the page it sat on.
            ContainerRole.Surface -> base.copy(container = theme.color(ColorRole.SurfaceContainer))

            ContainerRole.TopAppBar -> base.copy(
                shape = theme.shape(ShapeRole.None),
                verticalPadding = theme.space(SpaceRole.Sm),
                typeRole = TypeRole.Title,
            )

            ContainerRole.Dialog -> base.copy(
                shape = theme.shape(ShapeRole.Large),
                elevation = 6.dp,
                horizontalPadding = theme.space(SpaceRole.Lg),
                verticalPadding = theme.space(SpaceRole.Lg),
                scrim = Color.Black.copy(alpha = SCRIM_ALPHA),
            )

            ContainerRole.Menu -> base.copy(
                shape = theme.shape(ShapeRole.ExtraSmall),
                elevation = 3.dp,
                horizontalPadding = 0.dp,
                verticalPadding = theme.space(SpaceRole.Xs),
            )

            // The inverse surface: light text on a dark chip, and the reverse in dark mode.
            ContainerRole.Tooltip -> base.copy(
                container = theme.color(ColorRole.OnSurface),
                content = theme.color(ColorRole.Surface),
                shape = theme.shape(ShapeRole.ExtraSmall),
                horizontalPadding = theme.space(SpaceRole.Sm),
                verticalPadding = theme.space(SpaceRole.Xs),
                typeRole = TypeRole.Caption,
            )
        }
    }

    /** A Material tab row: the selected tab is underlined across its full width. */
    override fun tabs(theme: ResolvedTheme): TabsStyle = TabsStyle(
        container = theme.color(ColorRole.Surface),
        shape = theme.shape(ShapeRole.None),
        selectedContent = theme.color(ColorRole.Primary),
        unselectedContent = theme.color(ColorRole.OnSurfaceVariant),
        selectedContainer = Color.Transparent,
        selectedShape = theme.shape(ShapeRole.None),
        indicator = theme.color(ColorRole.Primary),
        indicatorHeight = 3.dp,
        indicatorShape = theme.shape(ShapeRole.ExtraSmall),
        indicatorFillsTab = true,
        horizontalPadding = theme.space(SpaceRole.Md),
        verticalPadding = theme.space(SpaceRole.Sm),
        typeRole = TypeRole.Label,
    )

    /**
     * Material Symbols metrics: a 24 dp grid, a 2 dp stem and flat ends.
     *
     * Every role is drawn to the same weight, which is what makes a row of them line up.
     */
    override fun icon(role: IconRole, theme: ResolvedTheme): IconStyle = IconStyle(
        size = 24.dp,
        strokeWidth = 2.dp,
        cap = StrokeCap.Butt,
        join = StrokeJoin.Miter,
    )

    /** A calendar grid for the date, a dial for the time, a menu under the field. */
    override val pickers: PickerRules = PickerRules(
        date = DatePresentation.CalendarGrid,
        time = TimePresentation.Dial,
        choice = ChoicePresentation.ExposedMenu,
    )

    override val motion: Motion = Motion(
        pressMillis = 100,
        releaseMillis = 200,
        easing = FastOutSlowInEasing,
        tooltipDelayMillis = 500,
    )

    private const val SCRIM_ALPHA = 0.32f
    private const val STATE_LAYER_ALPHA = 0.12f
    private const val TONE_FULL_DP = 24f
    private const val MAX_TONE = 0.14f
}

/**
 * Apple's rules: Liquid Glass, the design language macOS 26 and iOS 26 draw.
 *
 * Reference: Apple Human Interface Guidelines, "Materials", "Liquid Glass", "Buttons" and
 * "Motion", 2026 revision, the same revision the generated token table cites.
 *
 * Four things separate this from the flat fills it replaced, and all four are drawn here
 * rather than described: surfaces are a translucent tint over whatever the application
 * drew behind them, their edges are lit along the top and shaded along the bottom,
 * corners are continuous rather than circular and inner corners are cut concentric with
 * the container they sit in, and depth comes from layers overlapping instead of from a
 * stack of shadows.
 *
 * What is not drawn, and is not claimed: the material does not sample the desktop behind
 * the window. That needs a platform compositing view outside the Compose surface.
 *
 * Where the glass goes depends on the window. macOS 26 puts it on chrome, the sidebars
 * and toolbars and the title bar area, over document content that stays opaque; a phone
 * sized window carries it onto the content surfaces too. A window that is glass from edge
 * to edge is wrong at both sizes, so the size class decides, and [glassRoles] is where
 * that decision lives.
 */
internal object LiquidGlassRules : ComponentRules {

    /**
     * Which container roles are glass in a window of this class.
     *
     * Compact is a phone: the floating control surfaces are glass as well as the chrome.
     * Medium and Expanded are a desktop window, where a card or a plain surface is the
     * document being read and has to stay opaque behind its text.
     */
    private fun isGlass(role: ContainerRole, sizeClass: WindowSizeClass): Boolean = when (role) {
        // Chrome, at every size.
        ContainerRole.TopAppBar, ContainerRole.Menu, ContainerRole.Dialog, ContainerRole.Tooltip -> true
        // Content, glass only where the screen is small enough that there is no separate
        // chrome to speak of.
        ContainerRole.Card, ContainerRole.Surface -> sizeClass == WindowSizeClass.Compact
    }

    /**
     * The material for one container role: glass where this window puts glass, and a flat
     * fill everywhere else.
     *
     * [backdrop] is what the surface expects to sit over, which is what decides how the
     * translucent tint will actually read once composited. [content] is the colour that
     * will be drawn on top, and is what the opaque fallback has to stay legible against.
     */
    private fun material(
        role: ContainerRole,
        container: Color,
        content: Color,
        theme: ResolvedTheme,
    ): SurfaceMaterial = if (isGlass(role, theme.sizeClass)) {
        LiquidGlass.material(
            dark = theme.dark,
            // A menu or a tooltip is small and sits over anything, so it has to win
            // against a busy backdrop. A bar or a card covers a known surface and can
            // afford to let more of it through.
            prominence = when (role) {
                ContainerRole.Menu, ContainerRole.Tooltip, ContainerRole.Dialog -> GlassProminence.Regular
                else -> GlassProminence.Clear
            },
            backdrop = container,
            content = content,
        )
    } else {
        SurfaceMaterial.Opaque(container)
    }

    /** A continuous corner at the radius this system's table gives [role]. */
    private fun continuous(role: ShapeRole, theme: ResolvedTheme): Shape {
        val radius = theme.radius(role)
        return if (radius.value >= CAPSULE_RADIUS) CapsuleShape else ContinuousCornerShape(radius)
    }

    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // One soft shadow spread over roughly twice the requested height, at a low alpha.
        // Glass gets its depth from layers tinting each other rather than from a stack of
        // shadows, so the shadow that remains is there to lift the layer off the page and
        // nothing more.
        return modifier.shadow(
            elevation = elevation * SPREAD,
            shape = shape,
            clip = false,
            ambientColor = Color.Black.copy(alpha = AMBIENT_ALPHA),
            spotColor = Color.Black.copy(alpha = SPOT_ALPHA),
        )
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val base = ButtonStyle(
            container = Color.Transparent,
            pressedContainer = Color.Transparent,
            content = theme.color(ColorRole.Primary),
            // A press dims the whole control rather than layering a colour on it.
            pressedContentAlpha = PRESSED_ALPHA,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            pressedBorderColor = Color.Transparent,
            topHighlight = null,
            // Since iOS 26 a button is a capsule, and the capsule is a continuous curve
            // rather than a half circle glued onto two straight lines.
            shape = CapsuleShape,
            horizontalPadding = theme.space(SpaceRole.Md),
            verticalPadding = theme.space(SpaceRole.Sm),
            minHeight = 34.dp,
            typeRole = TypeRole.BodyStrong,
            // Apple's buttons never cast a shadow, pressed or not.
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            ButtonVariant.Filled -> base.copy(
                container = theme.color(ColorRole.Primary),
                pressedContainer = theme.color(ColorRole.Primary).copy(alpha = PRESSED_ALPHA),
                content = theme.color(ColorRole.OnPrimary),
            )

            // A glass button: the surface tint rather than a solid grey, with the lit edge
            // that tells it apart from a flat chip.
            ButtonVariant.Tonal -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                pressedContainer = theme.color(ColorRole.OutlineVariant),
                content = theme.color(ColorRole.Primary),
            )

            ButtonVariant.Outlined -> base.copy(
                pressedContainer = theme.color(ColorRole.SurfaceVariant),
                borderWidth = 1.dp,
                borderColor = theme.color(ColorRole.Outline),
                pressedBorderColor = theme.color(ColorRole.Outline),
            )

            // A plain button: content colour only, no container even when pressed.
            ButtonVariant.Text -> base
        }
    }

    /**
     * The containers: chrome is glass, grouped content is a surface with a continuous
     * corner, and a bar is separated from what it covers by a hairline rather than by a
     * shadow.
     */
    override fun container(role: ContainerRole, theme: ResolvedTheme): ContainerStyle {
        val base = ContainerStyle(
            container = theme.color(ColorRole.Surface),
            content = theme.color(ColorRole.OnSurface),
            shape = continuous(ShapeRole.Medium, theme),
            elevation = 0.dp,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            horizontalPadding = theme.space(SpaceRole.Md),
            verticalPadding = theme.space(SpaceRole.Md),
            separator = null,
            scrim = Color.Transparent,
            typeRole = TypeRole.Body,
        )
        val styled = when (role) {
            // A grouped box: no shadow, a different surface and a generous continuous
            // corner.
            ContainerRole.Card -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                shape = continuous(ShapeRole.Large, theme),
            )

            // A `Surface` is a panel: a layer raised off the page, holding the page's own
            // reading ink. That is `SurfaceContainer`, not `Surface`. `Surface` is the base
            // reading colour, and Material 3 gives it the same value as `Background` on
            // purpose, so a panel painted with it was drawn full size, in the right colour,
            // and could not be seen against the page it sat on.
            ContainerRole.Surface -> base.copy(container = theme.color(ColorRole.SurfaceContainer))

            // A toolbar. Glass at every size, because this is the piece macOS 26 makes
            // glass over opaque content.
            ContainerRole.TopAppBar -> base.copy(
                shape = continuous(ShapeRole.None, theme),
                verticalPadding = theme.space(SpaceRole.Sm),
                separator = theme.color(ColorRole.OutlineVariant),
                typeRole = TypeRole.BodyStrong,
            )

            // An alert: centred, heavily rounded, over a dimmed screen.
            ContainerRole.Dialog -> base.copy(
                shape = continuous(ShapeRole.Large, theme),
                horizontalPadding = theme.space(SpaceRole.Lg),
                verticalPadding = theme.space(SpaceRole.Lg),
                scrim = Color.Black.copy(alpha = SCRIM_ALPHA),
            )

            ContainerRole.Menu -> base.copy(
                shape = continuous(ShapeRole.Medium, theme),
                elevation = 2.dp,
                horizontalPadding = 0.dp,
                verticalPadding = theme.space(SpaceRole.Xs),
            )

            // A help tag: a light chip, not an inverted one.
            ContainerRole.Tooltip -> base.copy(
                shape = continuous(ShapeRole.Small, theme),
                horizontalPadding = theme.space(SpaceRole.Sm),
                verticalPadding = theme.space(SpaceRole.Xs),
                typeRole = TypeRole.Caption,
            )
        }
        return styled.copy(material = material(role, styled.container, styled.content, theme))
    }

    /**
     * A segmented control: the selection is a filled segment inside a track.
     *
     * The selected segment's corner is cut concentric with the track's, so the gap
     * between the two outlines is the same all the way round instead of pinching at the
     * corners. This is the smallest place the concentric rule shows, and the easiest to
     * see once you know to look.
     */
    override fun tabs(theme: ResolvedTheme): TabsStyle {
        val inset = theme.space(SpaceRole.Xs)
        val track = theme.radius(ShapeRole.Medium)
        return TabsStyle(
            container = theme.color(ColorRole.SurfaceVariant),
            shape = ContinuousCornerShape(track),
            selectedContent = theme.color(ColorRole.OnSurface),
            unselectedContent = theme.color(ColorRole.OnSurfaceVariant),
            selectedContainer = theme.color(ColorRole.Surface),
            selectedShape = ContinuousCornerShape(concentricRadius(track, inset)),
            indicator = Color.Transparent,
            indicatorHeight = 0.dp,
            indicatorShape = continuous(ShapeRole.None, theme),
            indicatorFillsTab = true,
            horizontalPadding = theme.space(SpaceRole.Md),
            verticalPadding = inset,
            typeRole = TypeRole.Body,
        )
    }

    /**
     * SF Symbols metrics: a lighter stroke on a 22 dp grid, with rounded ends and joins.
     * The rounded terminal is the single thing that reads most as Apple's icon set.
     */
    override fun icon(role: IconRole, theme: ResolvedTheme): IconStyle = IconStyle(
        size = 22.dp,
        strokeWidth = 1.75.dp,
        cap = StrokeCap.Round,
        join = StrokeJoin.Round,
    )

    /** Wheels throughout: a date, a time and a list are all spun to the value. */
    override val pickers: PickerRules = PickerRules(
        date = DatePresentation.Wheel,
        time = TimePresentation.Wheel,
        choice = ChoicePresentation.Wheel,
    )

    override val motion: Motion = Motion(
        pressMillis = 80,
        releaseMillis = 180,
        easing = LinearOutSlowInEasing,
        // A help tag waits until the pointer has clearly stopped.
        tooltipDelayMillis = 1000,
    )

    private const val SCRIM_ALPHA = 0.4f
    private const val PRESSED_ALPHA = 0.6f
    private const val AMBIENT_ALPHA = 0.08f
    private const val SPOT_ALPHA = 0.12f
    private const val SPREAD = 2f

    /** The radius at which the table means "a pill", not a corner of that size. */
    private const val CAPSULE_RADIUS = 1000.0f
}

/**
 * WinUI / Fluent 2 rules: elevation, button variants and motion.
 *
 * Reference: fluent2.microsoft.design, "Elevation", "Button" and "Motion duration", 2024.
 *
 * Elevation is a layer shadow with a hairline stroke, buttons are 4 dp rounded with a
 * lighter top edge, and the press is an immediate tone change with no travel.
 */
internal object FluentRules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // Fluent pairs every layer shadow with a thin stroke so the layer edge stays crisp.
        return modifier
            .shadow(elevation, shape, clip = false)
            .border(1.dp, theme.color(ColorRole.OutlineVariant), shape)
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val outline = theme.color(ColorRole.Outline)
        val base = ButtonStyle(
            container = theme.color(ColorRole.Surface),
            pressedContainer = theme.color(ColorRole.SurfaceVariant),
            content = theme.color(ColorRole.OnSurface),
            pressedContentAlpha = 1f,
            borderWidth = 1.dp,
            borderColor = outline,
            pressedBorderColor = theme.color(ColorRole.OutlineVariant),
            // The lighter top edge every Fluent control carries.
            topHighlight = Color.White.copy(alpha = if (theme.dark) 0.08f else 0.6f),
            // Fluent corners are 4 dp, which is ShapeRole.Medium in the Fluent table.
            shape = theme.shape(ShapeRole.Medium),
            horizontalPadding = theme.space(SpaceRole.Lg),
            verticalPadding = theme.space(SpaceRole.Sm),
            minHeight = 32.dp,
            typeRole = TypeRole.BodyStrong,
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            // Accent button.
            ButtonVariant.Filled -> base.copy(
                container = theme.color(ColorRole.Primary),
                pressedContainer = lerp(theme.color(ColorRole.Primary), Color.Black, PRESS_SHADE),
                content = theme.color(ColorRole.OnPrimary),
                borderColor = theme.color(ColorRole.Primary),
                pressedBorderColor = Color.Transparent,
                topHighlight = Color.White.copy(alpha = 0.18f),
            )

            // Standard button.
            ButtonVariant.Tonal -> base

            // Standard button with a stronger stroke.
            ButtonVariant.Outlined -> base.copy(
                container = Color.Transparent,
                borderColor = outline,
                topHighlight = null,
            )

            // Subtle button: no container until it is pressed.
            ButtonVariant.Text -> base.copy(
                container = Color.Transparent,
                borderWidth = 0.dp,
                borderColor = Color.Transparent,
                pressedBorderColor = Color.Transparent,
                topHighlight = null,
            )
        }
    }

    /**
     * Fluent containers: a layer is a surface with a hairline stroke, and anything that
     * floats over the page carries both the stroke and a layer shadow.
     */
    override fun container(role: ContainerRole, theme: ResolvedTheme): ContainerStyle {
        val stroke = theme.color(ColorRole.OutlineVariant)
        val base = ContainerStyle(
            container = theme.color(ColorRole.Surface),
            content = theme.color(ColorRole.OnSurface),
            shape = theme.shape(ShapeRole.Medium),
            elevation = 0.dp,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            horizontalPadding = theme.space(SpaceRole.Md),
            verticalPadding = theme.space(SpaceRole.Md),
            separator = null,
            scrim = Color.Transparent,
            typeRole = TypeRole.Body,
        )
        return when (role) {
            ContainerRole.Card -> base.copy(
                elevation = 2.dp,
                borderWidth = 1.dp,
                borderColor = stroke,
            )

            // A `Surface` is a panel: a layer raised off the page, holding the page's own
            // reading ink. That is `SurfaceContainer`, not `Surface`. `Surface` is the base
            // reading colour, and Material 3 gives it the same value as `Background` on
            // purpose, so a panel painted with it was drawn full size, in the right colour,
            // and could not be seen against the page it sat on.
            ContainerRole.Surface -> base.copy(container = theme.color(ColorRole.SurfaceContainer))

            // A command bar: flat, tight, and ruled off from the content below it.
            ContainerRole.TopAppBar -> base.copy(
                shape = theme.shape(ShapeRole.None),
                verticalPadding = theme.space(SpaceRole.Xs),
                separator = stroke,
                typeRole = TypeRole.BodyStrong,
            )

            ContainerRole.Dialog -> base.copy(
                elevation = 8.dp,
                borderWidth = 1.dp,
                borderColor = stroke,
                horizontalPadding = theme.space(SpaceRole.Lg),
                verticalPadding = theme.space(SpaceRole.Lg),
                scrim = Color.Black.copy(alpha = SCRIM_ALPHA),
            )

            // A flyout.
            ContainerRole.Menu -> base.copy(
                elevation = 8.dp,
                borderWidth = 1.dp,
                borderColor = stroke,
                horizontalPadding = 0.dp,
                verticalPadding = theme.space(SpaceRole.Xs),
            )

            ContainerRole.Tooltip -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                content = theme.color(ColorRole.OnSurfaceVariant),
                shape = theme.shape(ShapeRole.Small),
                borderWidth = 1.dp,
                borderColor = stroke,
                horizontalPadding = theme.space(SpaceRole.Sm),
                verticalPadding = theme.space(SpaceRole.Xs),
                elevation = 4.dp,
                typeRole = TypeRole.Caption,
            )
        }
    }

    /** A pivot: a short rounded bar centred under the selected header. */
    override fun tabs(theme: ResolvedTheme): TabsStyle = TabsStyle(
        container = theme.color(ColorRole.Surface),
        shape = theme.shape(ShapeRole.None),
        selectedContent = theme.color(ColorRole.OnSurface),
        unselectedContent = theme.color(ColorRole.OnSurfaceVariant),
        selectedContainer = Color.Transparent,
        selectedShape = theme.shape(ShapeRole.None),
        indicator = theme.color(ColorRole.Primary),
        indicatorHeight = 3.dp,
        indicatorShape = theme.shape(ShapeRole.Full),
        indicatorFillsTab = false,
        horizontalPadding = theme.space(SpaceRole.Md),
        verticalPadding = theme.space(SpaceRole.Sm),
        typeRole = TypeRole.Subtitle,
    )

    /**
     * Fluent icon metrics: a 20 dp grid, a 1.5 dp stroke and square ends, which is what
     * keeps a command bar's icons reading as one set with its text.
     */
    override fun icon(role: IconRole, theme: ResolvedTheme): IconStyle = IconStyle(
        size = 20.dp,
        strokeWidth = 1.5.dp,
        cap = StrokeCap.Square,
        join = StrokeJoin.Bevel,
    )

    /** A calendar flyout, a stepped field for the time, and a combo box for a list. */
    override val pickers: PickerRules = PickerRules(
        date = DatePresentation.CalendarFlyout,
        time = TimePresentation.Stepper,
        choice = ChoicePresentation.ComboBox,
    )

    override val motion: Motion = Motion(
        // Fluent's "ultra fast" duration: the press reads as instant.
        pressMillis = 50,
        releaseMillis = 100,
        easing = LinearEasing,
        tooltipDelayMillis = 300,
    )

    private const val SCRIM_ALPHA = 0.3f
    private const val PRESS_SHADE = 0.12f
}
