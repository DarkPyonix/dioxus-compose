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

            // A `Surface` is a panel: a layer raised off the page, holding the page's own
            // reading ink. That is `SurfaceContainer`, not `Surface`. `Surface` is the base
            // reading colour, and Material 3 gives it the same value as `Background` on
            // purpose, so a panel painted with it was drawn full size, in the right colour,
            // and could not be seen against the page it sat on.
            ContainerRole.Surface -> base.copy(container = theme.color(ColorRole.SurfaceContainer))

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

/**
 * GNOME 50 rules, in the Adwaita design language.
 *
 * Reference: the GNOME Human Interface Guidelines ("Ui Styling") and the libadwaita
 * stylesheet, GNOME 50, the same revision the generated token table cites.
 *
 * Adwaita keeps surfaces flat and separates layers with a hairline rather than a tint, it
 * spends its accent on one suggested action per view, and a pressed control changes fill
 * and stops: nothing ripples and nothing rises.
 */
internal object GnomeRules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // Half the requested height, because a Material shadow at the same number reads as
        // a floating card rather than a GTK popover. The surface colour is left alone: a
        // raised Adwaita layer is marked by its border, not by a tone shift.
        return modifier
            .shadow(elevation * SHADOW_SCALE, shape, clip = false)
            .border(1.dp, theme.color(ColorRole.OutlineVariant), shape)
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val neutral = theme.color(ColorRole.SurfaceVariant)
        val neutralPressed = lerp(neutral, theme.color(ColorRole.OnSurface), PRESS_MIX)
        val onNeutral = theme.color(ColorRole.OnSurface)
        val base = ButtonStyle(
            container = neutral,
            pressedContainer = neutralPressed,
            content = onNeutral,
            pressedContentAlpha = 1f,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            pressedBorderColor = Color.Transparent,
            topHighlight = null,
            // GTK buttons are 6 dp rounded, which is ShapeRole.Small in the Adwaita table.
            shape = theme.shape(ShapeRole.Small),
            horizontalPadding = theme.space(SpaceRole.Md),
            verticalPadding = theme.space(SpaceRole.Sm),
            minHeight = 34.dp,
            typeRole = TypeRole.BodyStrong,
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            // The suggested action, and the only place the accent appears.
            ButtonVariant.Filled -> base.copy(
                container = theme.color(ColorRole.Primary),
                pressedContainer = lerp(theme.color(ColorRole.Primary), Color.Black, PRESS_SHADE),
                content = theme.color(ColorRole.OnPrimary),
            )

            // The standard button: grey, not a tinted accent, which is the visible
            // difference from a Material screen where every variant carries the hue.
            ButtonVariant.Tonal -> base

            ButtonVariant.Outlined -> base.copy(
                container = Color.Transparent,
                borderWidth = 1.dp,
                borderColor = theme.color(ColorRole.Outline),
                pressedBorderColor = theme.color(ColorRole.Outline),
            )

            // A flat button, which takes the neutral fill only while it is held.
            ButtonVariant.Text -> base.copy(
                container = Color.Transparent,
                pressedContainer = neutral,
            )
        }
    }

    /**
     * Adwaita containers: a card is a flat view with a hairline, a header bar is the
     * chrome grey ruled off from the view below it, and a dialog is a rounded sheet.
     */
    override fun container(role: ContainerRole, theme: ResolvedTheme): ContainerStyle {
        val hairline = theme.color(ColorRole.OutlineVariant)
        val base = ContainerStyle(
            container = theme.color(ColorRole.Surface),
            content = theme.color(ColorRole.OnSurface),
            shape = theme.shape(ShapeRole.Large),
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
            // A boxed list: the view white, a 12 dp corner and a single line around it.
            ContainerRole.Card -> base.copy(
                borderWidth = 1.dp,
                borderColor = hairline,
            )

            ContainerRole.Surface -> base.copy(shape = theme.shape(ShapeRole.None))

            // A header bar: the chrome grey, flat, with a rule under it.
            ContainerRole.TopAppBar -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                shape = theme.shape(ShapeRole.None),
                verticalPadding = theme.space(SpaceRole.Sm),
                separator = hairline,
                typeRole = TypeRole.BodyStrong,
            )

            ContainerRole.Dialog -> base.copy(
                elevation = 4.dp,
                borderWidth = 1.dp,
                borderColor = hairline,
                horizontalPadding = theme.space(SpaceRole.Xl),
                verticalPadding = theme.space(SpaceRole.Lg),
                scrim = Color.Black.copy(alpha = SCRIM_ALPHA),
            )

            // A popover: rounded, bordered, and only shallowly raised.
            ContainerRole.Menu -> base.copy(
                shape = theme.shape(ShapeRole.Medium),
                elevation = 3.dp,
                borderWidth = 1.dp,
                borderColor = hairline,
                horizontalPadding = 0.dp,
                verticalPadding = theme.space(SpaceRole.Xs),
            )

            // A GTK tooltip is a dark chip with light text in both schemes.
            ContainerRole.Tooltip -> base.copy(
                container = theme.color(ColorRole.OnSurface),
                content = theme.color(ColorRole.Surface),
                shape = theme.shape(ShapeRole.Small),
                horizontalPadding = theme.space(SpaceRole.Sm),
                verticalPadding = theme.space(SpaceRole.Xs),
                typeRole = TypeRole.Caption,
            )
        }
    }

    /**
     * A view switcher: the selected view is a filled pill in the header bar, which is
     * what GNOME uses where Material would underline and Fluent would draw a pivot bar.
     */
    override fun tabs(theme: ResolvedTheme): TabsStyle = TabsStyle(
        container = theme.color(ColorRole.SurfaceVariant),
        shape = theme.shape(ShapeRole.None),
        selectedContent = theme.color(ColorRole.OnSurface),
        unselectedContent = theme.color(ColorRole.OnSurfaceVariant),
        selectedContainer = theme.color(ColorRole.Surface),
        selectedShape = theme.shape(ShapeRole.Full),
        indicator = Color.Transparent,
        indicatorHeight = 0.dp,
        indicatorShape = theme.shape(ShapeRole.None),
        indicatorFillsTab = true,
        horizontalPadding = theme.space(SpaceRole.Md),
        verticalPadding = theme.space(SpaceRole.Xs),
        typeRole = TypeRole.BodyStrong,
    )

    /**
     * Adwaita icon metrics: a 16 dp symbolic grid drawn with a heavier stem and rounded
     * ends, which is what makes a GNOME icon read as solid rather than outlined.
     */
    override fun icon(role: IconRole, theme: ResolvedTheme): IconStyle = IconStyle(
        size = 16.dp,
        strokeWidth = 2.dp,
        cap = StrokeCap.Round,
        join = StrokeJoin.Round,
    )

    /** A calendar grid, a stepped time entry, and a popover list for a choice. */
    override val pickers: PickerRules = PickerRules(
        date = DatePresentation.CalendarGrid,
        time = TimePresentation.Stepper,
        choice = ChoicePresentation.ComboBox,
    )

    override val motion: Motion = Motion(
        // GTK state changes are a short symmetric ease, and nothing ripples.
        pressMillis = 200,
        releaseMillis = 200,
        easing = FastOutSlowInEasing,
        tooltipDelayMillis = 500,
    )

    private const val SCRIM_ALPHA = 0.45f
    private const val PRESS_MIX = 0.1f
    private const val PRESS_SHADE = 0.16f
    private const val SHADOW_SCALE = 0.5f
}

/**
 * KDE Breeze rules, the Plasma design language.
 *
 * Reference: the Breeze colour schemes shipped with Plasma, the KDE Human Interface
 * Guidelines and Kirigami's unit scale, Plasma 6, the revision the token table cites.
 *
 * Breeze draws with thin, precise strokes: every frame is a hairline, a raised layer is
 * marked by a line before it is marked by a shadow, and an outlined control recolours its
 * line on press rather than filling in.
 */
internal object BreezeRules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        return modifier
            .shadow(elevation * SHADOW_SCALE, shape, clip = false)
            .border(1.dp, theme.color(ColorRole.Outline), shape)
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val accent = theme.color(ColorRole.Primary)
        val line = theme.color(ColorRole.Outline)
        val standard = theme.color(ColorRole.Surface)
        val onStandard = theme.color(ColorRole.OnSurface)
        val base = ButtonStyle(
            container = standard,
            pressedContainer = lerp(standard, onStandard, PRESS_MIX),
            content = onStandard,
            pressedContentAlpha = 1f,
            borderWidth = 1.dp,
            borderColor = line,
            pressedBorderColor = accent,
            topHighlight = null,
            // Breeze rounds a control at 4 dp, which is ShapeRole.Medium in its table.
            shape = theme.shape(ShapeRole.Medium),
            horizontalPadding = theme.space(SpaceRole.Lg),
            verticalPadding = theme.space(SpaceRole.Sm),
            minHeight = 30.dp,
            typeRole = TypeRole.Body,
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            // Even the accent button is a filled rectangle inside a darker line.
            ButtonVariant.Filled -> base.copy(
                container = accent,
                pressedContainer = lerp(accent, Color.Black, PRESS_SHADE),
                content = theme.color(ColorRole.OnPrimary),
                borderColor = lerp(accent, Color.Black, BORDER_SHADE),
                pressedBorderColor = lerp(accent, Color.Black, BORDER_SHADE),
            )

            // The standard button: a light fill inside a hairline.
            ButtonVariant.Tonal -> base

            // Hovering or pressing an outlined Breeze button recolours its line rather
            // than filling it in.
            ButtonVariant.Outlined -> base.copy(
                container = Color.Transparent,
                pressedContainer = Color.Transparent,
            )

            ButtonVariant.Text -> base.copy(
                container = Color.Transparent,
                pressedContainer = Color.Transparent,
                borderWidth = 0.dp,
                borderColor = Color.Transparent,
                pressedBorderColor = Color.Transparent,
            )
        }
    }

    /**
     * Breeze containers: everything is framed. A card is a view inside a single pixel
     * line, a toolbar is ruled off from the content, and a menu is a framed list.
     */
    override fun container(role: ContainerRole, theme: ResolvedTheme): ContainerStyle {
        val line = theme.color(ColorRole.Outline)
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
                shape = theme.shape(ShapeRole.Large),
                borderWidth = 1.dp,
                borderColor = line,
            )

            ContainerRole.Surface -> base.copy(shape = theme.shape(ShapeRole.None))

            // A Plasma toolbar: the window grey, tight, ruled off below.
            ContainerRole.TopAppBar -> base.copy(
                container = theme.color(ColorRole.Background),
                shape = theme.shape(ShapeRole.None),
                verticalPadding = theme.space(SpaceRole.Sm),
                separator = line,
                typeRole = TypeRole.Subtitle,
            )

            ContainerRole.Dialog -> base.copy(
                shape = theme.shape(ShapeRole.Large),
                elevation = 6.dp,
                borderWidth = 1.dp,
                borderColor = line,
                horizontalPadding = theme.space(SpaceRole.Xl),
                verticalPadding = theme.space(SpaceRole.Lg),
                scrim = Color.Black.copy(alpha = SCRIM_ALPHA),
            )

            ContainerRole.Menu -> base.copy(
                shape = theme.shape(ShapeRole.Small),
                elevation = 4.dp,
                borderWidth = 1.dp,
                borderColor = line,
                horizontalPadding = 0.dp,
                verticalPadding = theme.space(SpaceRole.Xs),
            )

            ContainerRole.Tooltip -> base.copy(
                shape = theme.shape(ShapeRole.ExtraSmall),
                borderWidth = 1.dp,
                borderColor = line,
                horizontalPadding = theme.space(SpaceRole.Sm),
                verticalPadding = theme.space(SpaceRole.Xs),
                typeRole = TypeRole.Caption,
            )
        }
    }

    /**
     * A Plasma tab bar: the selected tab is a framed page tab sitting on the toolbar,
     * with a thin highlight line along its top edge.
     */
    override fun tabs(theme: ResolvedTheme): TabsStyle = TabsStyle(
        container = theme.color(ColorRole.Background),
        shape = theme.shape(ShapeRole.None),
        selectedContent = theme.color(ColorRole.OnSurface),
        unselectedContent = theme.color(ColorRole.OnSurfaceVariant),
        selectedContainer = theme.color(ColorRole.Surface),
        selectedShape = theme.shape(ShapeRole.ExtraSmall),
        indicator = theme.color(ColorRole.Primary),
        indicatorHeight = 2.dp,
        indicatorShape = theme.shape(ShapeRole.None),
        indicatorFillsTab = true,
        horizontalPadding = theme.space(SpaceRole.Lg),
        verticalPadding = theme.space(SpaceRole.Xs),
        typeRole = TypeRole.Body,
    )

    /**
     * Breeze icon metrics: a 22 dp grid drawn with a thin, even stroke and mitred joins,
     * which is what gives the set its drafted look next to Adwaita's solid symbolics.
     */
    override fun icon(role: IconRole, theme: ResolvedTheme): IconStyle = IconStyle(
        size = 22.dp,
        strokeWidth = 1.25.dp,
        cap = StrokeCap.Butt,
        join = StrokeJoin.Miter,
    )

    /** A calendar grid, a stepped time field, and a drop down list for a choice. */
    override val pickers: PickerRules = PickerRules(
        date = DatePresentation.CalendarGrid,
        time = TimePresentation.Stepper,
        choice = ChoicePresentation.ExposedMenu,
    )

    override val motion: Motion = Motion(
        // Plasma's transitions are quick and nearly linear: a Breeze control is meant to
        // feel mechanical, so the release is shorter than the press.
        pressMillis = 100,
        releaseMillis = 80,
        easing = LinearEasing,
        tooltipDelayMillis = 700,
    )

    private const val SCRIM_ALPHA = 0.5f
    private const val PRESS_MIX = 0.12f
    private const val PRESS_SHADE = 0.1f
    private const val BORDER_SHADE = 0.22f
    private const val SHADOW_SCALE = 0.75f
}

/**
 * Deepin rules, and the look a Linux session that is neither GNOME nor KDE gets.
 *
 * Reference: the Deepin Design specification and the DTK control defaults, deepin 23, the
 * revision the token table cites. Only token values and style rules are taken: Deepin's
 * icon set and its bundled typeface carry their own licences.
 *
 * Depth here is a wide, soft, warm shadow plus a small lightening of the surface itself,
 * never a border: a hairline would fight the large radii that make the language what it
 * is.
 */
internal object DeepinRules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // Spread further than the dp asked for, and lightened, so a card at 8 dp sits
        // visibly above one at 2 dp even where the shadow itself is hard to see.
        val lifted = lerp(
            theme.color(ColorRole.Surface),
            if (theme.dark) LIFT_DARK else Color.White,
            (elevation.value / LIFT_FULL_DP).coerceIn(0f, 1f) * MAX_LIFT,
        )
        return modifier
            .shadow(
                elevation = elevation * SPREAD,
                shape = shape,
                clip = false,
                ambientColor = Color.Black.copy(alpha = AMBIENT_ALPHA),
                spotColor = Color.Black.copy(alpha = SPOT_ALPHA),
            )
            .background(lifted, shape)
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val accent = theme.color(ColorRole.Primary)
        val tonal = theme.color(ColorRole.SurfaceVariant)
        val onTonal = theme.color(ColorRole.OnSurface)
        val base = ButtonStyle(
            container = tonal,
            pressedContainer = lerp(tonal, theme.color(ColorRole.Outline), PRESS_MIX),
            content = onTonal,
            pressedContentAlpha = 1f,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            pressedBorderColor = Color.Transparent,
            topHighlight = null,
            // A lozenge: 12 dp, which is ShapeRole.Medium in the Deepin table.
            shape = theme.shape(ShapeRole.Medium),
            horizontalPadding = theme.space(SpaceRole.Lg),
            verticalPadding = theme.space(SpaceRole.Sm),
            minHeight = 36.dp,
            typeRole = TypeRole.Body,
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            // A rounded slab of brand blue with no border at all.
            ButtonVariant.Filled -> base.copy(
                container = accent,
                pressedContainer = lerp(accent, Color.Black, PRESS_SHADE),
                content = theme.color(ColorRole.OnPrimary),
            )

            // A warm tinted fill, which is where the palette shows on a control that
            // carries no accent.
            ButtonVariant.Tonal -> base

            ButtonVariant.Outlined -> base.copy(
                container = Color.Transparent,
                pressedContainer = tonal,
                borderWidth = 1.dp,
                borderColor = theme.color(ColorRole.Outline),
                pressedBorderColor = accent,
            )

            // A text button keeps the accent for its label, which is the one place the
            // brand colour appears without a fill behind it.
            ButtonVariant.Text -> base.copy(
                container = Color.Transparent,
                pressedContainer = tonal,
                content = accent,
            )
        }
    }

    /**
     * Deepin containers: large radii throughout, depth by shadow, and a title bar that is
     * part of the rounded window rather than a bar ruled off from it.
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
                shape = theme.shape(ShapeRole.Large),
                elevation = 3.dp,
            )

            ContainerRole.Surface -> base.copy(shape = theme.shape(ShapeRole.None))

            // Part of the window, not a bar over it: no rule underneath, and the title
            // sits on the same warm surface as the content.
            ContainerRole.TopAppBar -> base.copy(
                shape = theme.shape(ShapeRole.None),
                verticalPadding = theme.space(SpaceRole.Sm),
                typeRole = TypeRole.Subtitle,
            )

            ContainerRole.Dialog -> base.copy(
                shape = theme.shape(ShapeRole.Large),
                elevation = 12.dp,
                horizontalPadding = theme.space(SpaceRole.Xl),
                verticalPadding = theme.space(SpaceRole.Xl),
                scrim = Color.Black.copy(alpha = SCRIM_ALPHA),
            )

            ContainerRole.Menu -> base.copy(
                shape = theme.shape(ShapeRole.Small),
                elevation = 8.dp,
                horizontalPadding = theme.space(SpaceRole.Xs),
                verticalPadding = theme.space(SpaceRole.Xs),
            )

            ContainerRole.Tooltip -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                content = theme.color(ColorRole.OnSurfaceVariant),
                shape = theme.shape(ShapeRole.ExtraSmall),
                elevation = 6.dp,
                horizontalPadding = theme.space(SpaceRole.Sm),
                verticalPadding = theme.space(SpaceRole.Xs),
                typeRole = TypeRole.Caption,
            )
        }
    }

    /**
     * A pill switcher: the selected view is a fully rounded slab of the accent, which is
     * the mark Deepin uses where Material underlines and Breeze frames a page tab.
     */
    override fun tabs(theme: ResolvedTheme): TabsStyle = TabsStyle(
        container = theme.color(ColorRole.SurfaceVariant),
        shape = theme.shape(ShapeRole.Full),
        selectedContent = theme.color(ColorRole.OnPrimary),
        unselectedContent = theme.color(ColorRole.OnSurfaceVariant),
        selectedContainer = theme.color(ColorRole.Primary),
        selectedShape = theme.shape(ShapeRole.Full),
        indicator = Color.Transparent,
        indicatorHeight = 0.dp,
        indicatorShape = theme.shape(ShapeRole.None),
        indicatorFillsTab = true,
        horizontalPadding = theme.space(SpaceRole.Lg),
        verticalPadding = theme.space(SpaceRole.Xs),
        typeRole = TypeRole.Body,
    )

    /**
     * A large, soft icon grid: 24 dp drawn with a heavy rounded stroke and bevelled
     * joins, to match the rounded shapes rather than fight them.
     */
    override fun icon(role: IconRole, theme: ResolvedTheme): IconStyle = IconStyle(
        size = 24.dp,
        strokeWidth = 2.25.dp,
        cap = StrokeCap.Round,
        join = StrokeJoin.Bevel,
    )

    /** A calendar flyout, a dial for the time, and a drop down list for a choice. */
    override val pickers: PickerRules = PickerRules(
        date = DatePresentation.CalendarFlyout,
        time = TimePresentation.Dial,
        choice = ChoicePresentation.ExposedMenu,
    )

    override val motion: Motion = Motion(
        // The slowest and softest here, to match the rounded shapes.
        pressMillis = 250,
        releaseMillis = 200,
        easing = LinearOutSlowInEasing,
        tooltipDelayMillis = 600,
    )

    private const val SCRIM_ALPHA = 0.35f
    private const val PRESS_MIX = 0.35f
    private const val PRESS_SHADE = 0.2f
    private const val AMBIENT_ALPHA = 0.1f
    private const val SPOT_ALPHA = 0.18f
    private const val SPREAD = 1.6f
    private const val LIFT_FULL_DP = 24f
    private const val MAX_LIFT = 0.08f
    private val LIFT_DARK = Color(0xFFFFE9D2)
}
