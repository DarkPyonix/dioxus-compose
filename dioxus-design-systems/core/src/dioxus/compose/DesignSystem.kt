package dioxus.compose

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp

/**
 * The contract every design system in this project implements.
 *
 * A caller names a role. The design system answers with pixels. That direction is the
 * whole point of the split: a widget never says "blue" or "8dp corner", it says
 * `ColorRole.Primary` and `ShapeRole.Medium`, so the same widget code draws a Material
 * button, a Cupertino one and a Fluent one without knowing that any of them exist.
 *
 * Adding a seventh design system therefore means writing one more implementation of this
 * interface. It does not mean touching a widget.
 *
 * Everything here is resolved for one colour scheme. Light and dark are two instances,
 * not a flag threaded through every call, because a design system is free to change more
 * than colours between them.
 */
@Immutable
interface DesignSystem {

    val id: DesignSystemId

    /** True when this instance resolves its roles for a dark scheme. */
    val isDark: Boolean

    fun color(role: ColorRole): Color

    fun type(role: TypeRole): TextStyle

    fun shape(role: ShapeRole): Shape

    fun space(role: SpaceRole): Dp

    /**
     * How a raised surface reads at [elevation].
     *
     * Elevation crosses the boundary as a single dp value and nothing else, because the
     * three systems disagree about what raising something looks like: Material tints the
     * surface and casts a shadow, Cupertino prefers a wide soft shadow with no tint,
     * Fluent draws a layered shadow with a light stroke along the top edge. A caller that
     * could specify the shadow colour would be making that decision for them.
     */
    fun elevation(elevation: Dp, base: Color): ElevationStyle

    /** How a button of [variant] is painted, in each of its interaction states. */
    fun button(variant: ButtonVariant): ButtonStyle

    /** Durations and easing for state transitions. */
    val motion: Motion

    /**
     * The material a surface is made of, if this system has one.
     *
     * Cupertino answers with Liquid Glass. The others answer [SurfaceMaterial.Opaque],
     * which is not a fallback but their actual design language: a Material 3 card is a
     * solid tinted surface, and drawing it as glass would be wrong rather than plainer.
     */
    fun material(role: ColorRole): SurfaceMaterial = SurfaceMaterial.Opaque(color(role))
}

/**
 * The design system in effect for this part of the tree.
 *
 * Static because it changes rarely and reading it must not cost a recomposition scope on
 * every widget that asks.
 */
val LocalDesignSystem = staticCompositionLocalOf<DesignSystem> {
    error(
        "No design system is in scope. Wrap this content in a DesignSystemProvider, or in " +
            "the renderer's host composable, which installs the one the application asked " +
            "for.",
    )
}
