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

            ContainerRole.Surface -> base

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
 * Cupertino rules: elevation, button variants and motion.
 *
 * Reference: Apple Human Interface Guidelines, "Materials", "Buttons" and "Motion", 2024.
 *
 * Shadows are wide and faint rather than layered, the emphasised button is flat with no
 * shadow at all, and the press feedback is a dim, not a ripple.
 */
internal object CupertinoRules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // One soft shadow spread over roughly twice the requested height, at a low alpha.
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
            // HIG presses dim the whole control instead of layering a colour on it.
            pressedContentAlpha = PRESSED_ALPHA,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            pressedBorderColor = Color.Transparent,
            topHighlight = null,
            // The continuous curvature of a HIG capsule is approximated by the Medium
            // radius; see the note in the Renderer README about squircle support.
            shape = theme.shape(ShapeRole.Medium),
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
     * HIG containers: grouped content sits on a slightly different surface rather than
     * casting a shadow, bars are separated by a hairline, and only what floats over the
     * screen is raised at all.
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
            // A grouped box: no shadow, just a different surface and a generous corner.
            ContainerRole.Card -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                shape = theme.shape(ShapeRole.Large),
            )

            ContainerRole.Surface -> base

            // A navigation bar is flush with the content and divided by a hairline.
            ContainerRole.TopAppBar -> base.copy(
                shape = theme.shape(ShapeRole.None),
                verticalPadding = theme.space(SpaceRole.Sm),
                separator = theme.color(ColorRole.OutlineVariant),
                typeRole = TypeRole.BodyStrong,
            )

            // An alert: centred, heavily rounded, over a dimmed screen.
            ContainerRole.Dialog -> base.copy(
                shape = theme.shape(ShapeRole.Large),
                horizontalPadding = theme.space(SpaceRole.Lg),
                verticalPadding = theme.space(SpaceRole.Lg),
                scrim = Color.Black.copy(alpha = SCRIM_ALPHA),
            )

            ContainerRole.Menu -> base.copy(
                elevation = 2.dp,
                horizontalPadding = 0.dp,
                verticalPadding = theme.space(SpaceRole.Xs),
                borderWidth = 1.dp,
                borderColor = theme.color(ColorRole.OutlineVariant),
            )

            // A help tag: a light chip with a hairline, not an inverted one.
            ContainerRole.Tooltip -> base.copy(
                shape = theme.shape(ShapeRole.Small),
                borderWidth = 1.dp,
                borderColor = theme.color(ColorRole.OutlineVariant),
                horizontalPadding = theme.space(SpaceRole.Sm),
                verticalPadding = theme.space(SpaceRole.Xs),
                typeRole = TypeRole.Caption,
            )
        }
    }

    /** A segmented control: the selection is a filled segment inside a track. */
    override fun tabs(theme: ResolvedTheme): TabsStyle = TabsStyle(
        container = theme.color(ColorRole.SurfaceVariant),
        shape = theme.shape(ShapeRole.Medium),
        selectedContent = theme.color(ColorRole.OnSurface),
        unselectedContent = theme.color(ColorRole.OnSurfaceVariant),
        selectedContainer = theme.color(ColorRole.Surface),
        selectedShape = theme.shape(ShapeRole.Small),
        indicator = Color.Transparent,
        indicatorHeight = 0.dp,
        indicatorShape = theme.shape(ShapeRole.None),
        indicatorFillsTab = true,
        horizontalPadding = theme.space(SpaceRole.Md),
        verticalPadding = theme.space(SpaceRole.Xs),
        typeRole = TypeRole.Body,
    )

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

            ContainerRole.Surface -> base

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
