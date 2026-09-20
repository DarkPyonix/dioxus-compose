package dioxus.compose.liquidglass

import androidx.compose.ui.graphics.Color
import dioxus.compose.SurfaceMaterial

/**
 * The one decision a glass surface makes before it draws: translucent, or opaque.
 *
 * It lives apart from the modifier that uses it because it is where two promises are
 * kept, and a promise that can only be checked by looking at a screen is not a promise.
 * Given the same inputs this returns the same colour, so a test can state the rule
 * directly: reduced transparency takes the opaque path, a build without blur takes the
 * opaque path, and the colour it takes is the one whose contrast was guaranteed when the
 * material was built.
 *
 * Translucency without blur is the case worth naming. It is not a milder version of the
 * effect; it is the same loss of contrast with none of the separation that made the loss
 * worth it, so a surface that would blur and cannot has no reason to stay see-through.
 */
fun glassFill(
    material: SurfaceMaterial.Glass,
    reduceTransparency: Boolean,
    blurAvailable: Boolean,
    depth: Int = 0,
): Color {
    val resolved = material.atDepth(depth)
    return if (drawsAsGlass(reduceTransparency, blurAvailable)) {
        resolved.tint.copy(alpha = resolved.tintAlpha)
    } else {
        resolved.fallback
    }
}

/** Whether these conditions leave a glass material drawing as glass. */
fun drawsAsGlass(reduceTransparency: Boolean, blurAvailable: Boolean): Boolean =
    !reduceTransparency && blurAvailable
