package dioxus.compose.design

// A copy of GlassMaterial.kt from the liquid-glass module of the dioxus-design-systems
// project, with the package changed and SurfaceMaterial taken from this module rather
// than from that project's core. That project is published on its own and must never
// depend on the renderer, so the material exists twice deliberately. Copies run in one
// direction: change the design systems file first, then bring the change here.

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * What a surface is made of.
 *
 * Most surfaces are a flat colour. Apple's are not: since iOS 26 and macOS 26 they
 * translucently tint what is behind them and catch light along their edges.
 *
 * This mirrors the type of the same name in the design systems project's core module.
 * The two are kept identical by hand because that project must stay publishable without
 * the renderer, and the renderer must stay buildable without it.
 */
@Immutable
sealed interface SurfaceMaterial {

    /** A flat fill. What Material 3, Fluent and the Linux systems use. */
    @Immutable
    data class Opaque(val color: Color) : SurfaceMaterial

    /**
     * A translucent, edge-lit surface: Liquid Glass.
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

/**
 * How much the material asserts itself over what is behind it.
 *
 * Apple's two glass recipes differ in exactly this: one is for surfaces that carry
 * controls and text and therefore has to win against a busy backdrop, the other is for
 * surfaces floating over media where the content underneath is the point.
 */
enum class GlassProminence {
    /** Carries text and controls. More tint, more blur, legible over anything. */
    Regular,

    /** Floats over content that should stay visible. Barely tinted. */
    Clear,
}

/**
 * The scope of what this module can and cannot draw, stated once so that no caller has to
 * guess.
 *
 * Compose Multiplatform cannot produce the real Liquid Glass material. On iOS 26 the
 * system draws it, through native SwiftUI navigation containers, and a Compose app only
 * gets it by hosting its content inside a SwiftUI shell; the glass then belongs to that
 * shell's chrome, not to anything drawn here. On the desktop, sampling the wallpaper
 * behind the window needs a platform compositing view outside the Compose surface.
 *
 * What this module draws is the part whose backdrop is content this application drew
 * itself: continuous and concentric corners, a translucent tinted surface over app
 * content, an edge lit along the top and shaded along the bottom, depth from layers
 * overlapping, and Compose's own blur applied to the content underneath. Those are
 * really drawn, not approximated, and they are most of what makes a screen read as iOS
 * 26. The wallpaper behind the window is not among them and is not claimed.
 */
object LiquidGlass {

    /**
     * A glass material with a guaranteed readable fallback.
     *
     * [backdrop] is the colour the material expects to sit over most of the time, and is
     * used only to work out what the translucent surface will actually look like once it
     * is composited. [content] is the colour that will be drawn on top of the material,
     * and is what the fallback has to stay legible against: the fallback is computed by
     * compositing the tint over the backdrop and then, if that does not reach
     * [minContrast], moving it away from [content] until it does.
     *
     * The translucent path carries no such guarantee and cannot, because the thing behind
     * it changes every frame. That asymmetry is the reason the fallback exists as a
     * stored colour rather than as a runtime alpha adjustment.
     */
    fun material(
        dark: Boolean,
        prominence: GlassProminence = GlassProminence.Regular,
        backdrop: Color,
        content: Color,
        minContrast: Float = MIN_CONTRAST_BODY,
    ): SurfaceMaterial.Glass {
        val tint = if (dark) DARK_TINT else LIGHT_TINT
        // High enough that the surface is a surface. A translucent layer whose colour is
        // within a few levels of the page behind it does not read as glass, it reads as
        // nothing: the first version of this was 0.70 over a 0xF2F2F7 page and the only
        // thing visible on screen was the rim.
        val alpha = when (prominence) {
            GlassProminence.Regular -> if (dark) 0.74f else 0.85f
            GlassProminence.Clear -> if (dark) 0.38f else 0.45f
        }
        val blur = when (prominence) {
            GlassProminence.Regular -> 30.dp
            GlassProminence.Clear -> 18.dp
        }
        // Light arrives from above, as it does in every Apple surface: the top edge
        // catches it and the bottom edge falls into shadow. A single flat stroke all the
        // way round reads as a drawn border; this reads as thickness.
        val highlight = if (dark) Color.White.copy(alpha = 0.34f) else Color.White.copy(alpha = 0.80f)
        val shade = if (dark) Color.Black.copy(alpha = 0.46f) else Color.Black.copy(alpha = 0.14f)

        val composited = compositeOver(tint.copy(alpha = alpha), backdrop)
        return SurfaceMaterial.Glass(
            tint = tint,
            tintAlpha = alpha,
            blurRadius = blur,
            highlight = highlight,
            shade = shade,
            fallback = ensureContrast(composited, content, minContrast),
        )
    }

    /**
     * The tint of the glass itself, before anything shows through it.
     *
     * White in light, and in dark a grey lighter than any page the surface can land on
     * and than any fill that can land on the surface. The tint has to differ from both,
     * or the material has nothing to say: glass reads as glass because it is lighter than
     * what is behind it and because the rim catches light, and a tint the colour of the
     * page cancels the first of those. The dark value was two levels from the secondary
     * fill grey once, and a tinted button on a bar disappeared into the bar.
     */
    val LIGHT_TINT: Color = Color(0xFFFFFFFF)
    val DARK_TINT: Color = Color(0xFF3A3A3C)

    /**
     * The ratio the opaque fallback is held to: WCAG 2.2 AA for body text.
     *
     * Body text is the hardest case a surface has to carry, so meeting it means every
     * lighter demand is met too.
     */
    const val MIN_CONTRAST_BODY: Float = 4.5f

    /**
     * How far a glass layer is pushed by each layer already beneath it.
     *
     * Stacked glass in iOS 26 does not gain a heavier shadow; it gains tint. Each layer
     * sees slightly less of the original backdrop through it, which is what separates a
     * sheet over a panel over the background without drawing three shadows.
     */
    const val DEPTH_TINT_STEP: Float = 0.08f

    /** The shadow a glass layer casts at [depth], wide and soft rather than tight. */
    fun depthShadow(depth: Int): Dp = when (depth.coerceAtLeast(0)) {
        0 -> 0.dp
        1 -> 8.dp
        2 -> 20.dp
        else -> 36.dp
    }
}

/**
 * [this] with its translucency increased for a layer sitting at [depth] in a stack.
 *
 * Depth 0 is unchanged. The alpha is capped below 1 so that a deeply stacked surface is
 * still glass rather than quietly becoming a flat fill.
 */
fun SurfaceMaterial.Glass.atDepth(depth: Int): SurfaceMaterial.Glass {
    if (depth <= 0) return this
    val raised = (tintAlpha + LiquidGlass.DEPTH_TINT_STEP * depth).coerceAtMost(0.94f)
    return copy(tintAlpha = raised)
}
