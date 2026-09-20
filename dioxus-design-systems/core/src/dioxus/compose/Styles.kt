package dioxus.compose

import androidx.compose.animation.core.Easing
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp

/**
 * What a raised surface looks like, as the three systems each understand raising.
 *
 * A system uses the parts it believes in and leaves the rest at their defaults. Material
 * sets [tint] and [shadow]; Cupertino sets [shadow] alone, wide and soft; Fluent sets
 * [shadow] and [strokeTop].
 */
@Immutable
data class ElevationStyle(
    /** The surface colour after the system has applied whatever tint it uses. */
    val surface: Color,
    val shadowElevation: Dp,
    val shadowColor: Color,
    /** A lighter edge along the top, which Fluent uses to suggest a lit surface. */
    val strokeTop: Color? = null,
)

/** How a button is painted in each state a pointer can put it in. */
@Immutable
data class ButtonStyle(
    val container: Color,
    val content: Color,
    val border: Color?,
    val borderWidth: Dp,
    val shape: ShapeRole,
    val pressedContainer: Color,
    val pressedContent: Color,
    val pressedBorder: Color?,
    /**
     * Whether pressing should spread a ripple from the touch point.
     *
     * A Material button does; a Cupertino button dims instead, and adding a ripple to it
     * would be a visible mistake rather than a detail.
     */
    val ripple: Boolean,
)

/** Durations and easing for state transitions. */
@Immutable
data class Motion(
    val pressMillis: Int,
    val releaseMillis: Int,
    val easing: Easing,
)

/**
 * What a surface is made of.
 *
 * Most surfaces are a flat colour. Cupertino's are not: since iOS 26 they sample and blur
 * what is behind them and catch light along their edges.
 */
@Immutable
sealed interface SurfaceMaterial {

    /** A flat fill. What Material 3, Fluent and the Linux systems use. */
    @Immutable
    data class Opaque(val color: Color) : SurfaceMaterial

    /**
     * A translucent, blurred, edge-lit surface: Liquid Glass.
     *
     * The blur applies to content this application drew. Sampling the system background
     * behind the window is not something a Compose surface can do, so this describes a
     * real effect over app content rather than a claim about the desktop behind it.
     */
    @Immutable
    data class Glass(
        val tint: Color,
        val tintAlpha: Float,
        val blurRadius: Dp,
        /** The bright edge where light would catch the top of the surface. */
        val highlight: Color,
        /** The darker edge along the bottom, which gives the surface thickness. */
        val shade: Color,
        /**
         * The opaque colour to use instead when blur is unavailable or the reader asked
         * for reduced transparency. It has to meet contrast on its own.
         */
        val fallback: Color,
    ) : SurfaceMaterial
}
