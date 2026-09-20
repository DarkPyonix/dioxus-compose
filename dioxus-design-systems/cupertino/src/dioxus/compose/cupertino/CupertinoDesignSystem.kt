package dioxus.compose.cupertino

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dioxus.compose.ButtonStyle
import dioxus.compose.ButtonVariant
import dioxus.compose.ColorRole
import dioxus.compose.DesignSystem
import dioxus.compose.DesignSystemId
import dioxus.compose.ElevationStyle
import dioxus.compose.Motion
import dioxus.compose.ShapeRole
import dioxus.compose.SpaceRole
import dioxus.compose.SurfaceMaterial
import dioxus.compose.TypeRole
import dioxus.compose.liquidglass.CapsuleShape
import dioxus.compose.liquidglass.ContinuousCornerShape
import dioxus.compose.liquidglass.GlassProminence
import dioxus.compose.liquidglass.LiquidGlass
import dioxus.compose.liquidglass.compositeOver

/**
 * Apple's design language, as of the Liquid Glass revision that arrived with macOS 26 and
 * iOS 26.
 *
 * Four things make a screen read as Apple's rather than as a rounded-rectangle theme, and
 * each is a deliberate decision here rather than a value:
 *
 * - **Continuous curvature.** Corners are superellipse quarters, not circular arcs. See
 *   [ContinuousCornerShape] for why the difference is visible.
 * - **No ripple.** A press dims; it does not spread a circle from the touch point. This is
 *   the single most recognisable tell of a Material widget wearing an Apple palette, so
 *   every button style here sets ripple off and supplies its own pressed colours.
 * - **Material rather than fill.** Surfaces are glass: translucent over the app content
 *   behind them, lit along the top edge, shaded along the bottom. They change when what is
 *   behind them changes.
 * - **Type that is set, not sized.** San Francisco's optical tracking, negative at reading
 *   sizes and positive at display sizes, carried across even though the font itself is not
 *   redistributable.
 *
 * Light and dark are two instances rather than a flag, because the palette is two
 * published palettes rather than one transformed.
 */
@Immutable
class CupertinoDesignSystem private constructor(
    override val isDark: Boolean,
) : DesignSystem {

    private val palette = if (isDark) CupertinoPalette.Dark else CupertinoPalette.Light

    override val id: DesignSystemId = DesignSystemId.Cupertino

    override fun color(role: ColorRole): Color = palette.resolve(role)

    override fun type(role: TypeRole): TextStyle = cupertinoType(role)

    /**
     * Corner radii, as continuous corners.
     *
     * The values are the ones Apple's controls use: 6 for a small field, 10 for a control,
     * 13 for a grouped row, 20 for a card or sheet. [ShapeRole.Full] is a capsule, which is
     * what Apple's pill buttons and search fields are, not a circle inscribed in a
     * rectangle.
     *
     * A caller nesting one of these inside another should re-cut the inner one with
     * `inset()` so the two stay concentric. A single radius constant cannot express that
     * relationship, which is why the inset function lives next to the shape.
     */
    override fun shape(role: ShapeRole): Shape = when (role) {
        ShapeRole.None -> RectangleShape
        ShapeRole.ExtraSmall -> ContinuousCornerShape(6.dp)
        ShapeRole.Small -> ContinuousCornerShape(10.dp)
        ShapeRole.Medium -> ContinuousCornerShape(13.dp)
        ShapeRole.Large -> ContinuousCornerShape(20.dp)
        ShapeRole.Full -> CapsuleShape
    }

    /** Apple's spacing is a 4 point grid, with 8 and 16 doing most of the work. */
    override fun space(role: SpaceRole): Dp = when (role) {
        SpaceRole.None -> 0.dp
        SpaceRole.Xs -> 2.dp
        SpaceRole.Sm -> 4.dp
        SpaceRole.Md -> 8.dp
        SpaceRole.Lg -> 16.dp
        SpaceRole.Xl -> 24.dp
        SpaceRole.Xxl -> 32.dp
    }

    /**
     * Raising a surface here casts a wide, soft, nearly colourless shadow and does not
     * tint the surface.
     *
     * Material raises a surface by tinting it towards the primary colour, which encodes
     * height as hue. Apple does not: a raised Apple surface is the same colour, separated
     * from what is under it by a shadow that is broad and faint rather than tight and
     * dark, and increasingly by the layering of the glass itself. Tinting on elevation
     * here would look like Material with Apple's blue in it.
     *
     * There is no top stroke: that is Fluent's way of suggesting a lit surface, and the
     * equivalent here is the glass edge, which belongs to the material and not to the
     * elevation.
     */
    override fun elevation(elevation: Dp, base: Color): ElevationStyle = ElevationStyle(
        surface = base,
        // Apple's shadows spread further than their dp height suggests, which is what
        // makes them read as soft light rather than as a drop shadow.
        shadowElevation = elevation * 1.5f,
        shadowColor = Color.Black.copy(alpha = if (isDark) 0.44f else 0.16f),
        strokeTop = null,
    )

    /**
     * Button styles, all of them capsules and none of them rippling.
     *
     * Pressing dims. A filled button's container darkens in the light scheme and lightens
     * in the dark one, always moving away from the surrounding surface so the change is
     * visible against it; a button with no container dims its label instead, which is
     * exactly what Apple's plain buttons do.
     */
    override fun button(variant: ButtonVariant): ButtonStyle = when (variant) {
        ButtonVariant.Filled -> ButtonStyle(
            container = palette.blue,
            content = Color.White,
            border = null,
            borderWidth = 0.dp,
            shape = ShapeRole.Full,
            pressedContainer = dim(palette.blue),
            pressedContent = Color.White,
            pressedBorder = null,
            ripple = false,
        )

        ButtonVariant.Tonal -> ButtonStyle(
            container = palette.controlFill,
            content = palette.blue,
            border = null,
            borderWidth = 0.dp,
            shape = ShapeRole.Full,
            pressedContainer = dim(palette.controlFill),
            pressedContent = palette.blue,
            pressedBorder = null,
            ripple = false,
        )

        ButtonVariant.Outlined -> ButtonStyle(
            container = Color.Transparent,
            content = palette.blue,
            border = palette.blue,
            borderWidth = 1.dp,
            shape = ShapeRole.Full,
            // Nothing to darken when the container is clear, so the whole control fades,
            // outline and label together.
            pressedContainer = palette.blue.copy(alpha = PRESSED_FADE_FILL),
            pressedContent = palette.blue.copy(alpha = PRESSED_FADE),
            pressedBorder = palette.blue.copy(alpha = PRESSED_FADE),
            ripple = false,
        )

        ButtonVariant.Text -> ButtonStyle(
            container = Color.Transparent,
            content = palette.blue,
            border = null,
            borderWidth = 0.dp,
            shape = ShapeRole.Full,
            pressedContainer = Color.Transparent,
            pressedContent = palette.blue.copy(alpha = PRESSED_FADE),
            pressedBorder = null,
            ripple = false,
        )
    }

    /**
     * A press lands almost immediately and the release eases back over a longer stretch.
     *
     * The asymmetry is the point: a control that takes as long to depress as to recover
     * feels unresponsive, and one that snaps back feels brittle. The curve is the ease
     * Apple uses for interface state changes, slow at both ends and quick through the
     * middle.
     */
    override val motion: Motion = Motion(
        pressMillis = 100,
        releaseMillis = 250,
        easing = CubicBezierEasing(0.25f, 0.1f, 0.25f, 1.0f),
    )

    /**
     * Surfaces are glass; the page behind them and every foreground colour are not.
     *
     * [ColorRole.Background] stays opaque because it is the bottom of the stack: glass
     * with nothing behind it is a flat fill drawn the expensive way. Content roles stay
     * opaque because they are ink, and ink is not a material.
     *
     * The fallback carried by each glass value is built here against the content colour
     * that will be drawn on it, so the promise it makes is about this design system's own
     * pairings rather than a generic one.
     */
    override fun material(role: ColorRole): SurfaceMaterial = when (role) {
        ColorRole.Surface -> glass(GlassProminence.Regular, palette.surface, palette.label)
        ColorRole.SurfaceVariant ->
            glass(GlassProminence.Clear, palette.surfaceVariant, palette.secondaryLabel)
        else -> SurfaceMaterial.Opaque(color(role))
    }

    private fun glass(
        prominence: GlassProminence,
        backdrop: Color,
        content: Color,
    ): SurfaceMaterial.Glass = LiquidGlass.material(
        dark = isDark,
        prominence = prominence,
        backdrop = backdrop,
        content = content,
    )

    /**
     * [color] moved a step away from the scheme's background, which is what a press looks
     * like without a ripple.
     *
     * Darkening in both schemes would make a dark-scheme press nearly invisible against an
     * almost black page, so the direction follows the scheme.
     */
    private fun dim(color: Color): Color = compositeOver(
        (if (isDark) Color.White else Color.Black).copy(alpha = PRESSED_DIM),
        color,
    )

    override fun equals(other: Any?): Boolean =
        other is CupertinoDesignSystem && other.isDark == isDark

    override fun hashCode(): Int = if (isDark) 1 else 0

    override fun toString(): String = "CupertinoDesignSystem(${if (isDark) "dark" else "light"})"

    companion object {
        val Light: CupertinoDesignSystem = CupertinoDesignSystem(isDark = false)
        val Dark: CupertinoDesignSystem = CupertinoDesignSystem(isDark = true)

        fun of(dark: Boolean): CupertinoDesignSystem = if (dark) Dark else Light

        /** How far a filled container moves when pressed. */
        const val PRESSED_DIM: Float = 0.14f

        /** How far a label or outline fades when there is no container to dim. */
        const val PRESSED_FADE: Float = 0.4f

        /** The faint wash a bordered button gains while held. */
        const val PRESSED_FADE_FILL: Float = 0.12f
    }
}
