package dioxus.compose.ui.node

import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope

/**
 * Reading pictures on Android, which draws through its own graphics stack rather than
 * through Skia directly.
 *
 * There is no vector document here. Android's graphics stack has no SVG parser, so an SVG
 * registration is reported as unreadable rather than drawn wrong or silently skipped, and
 * nothing ever constructs this type. Icons are unaffected: a vector icon registers a
 * meaning, and the design system draws the shape.
 */
class VectorDocument private constructor()

internal fun decodeRasterAsset(bytes: ByteArray): ImageBitmap? =
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()

internal fun decodeVectorAsset(bytes: ByteArray): VectorDocument? = null

internal fun DrawScope.drawVectorDocument(document: VectorDocument) {
    // Unreachable: nothing produces a VectorDocument on this platform.
}
