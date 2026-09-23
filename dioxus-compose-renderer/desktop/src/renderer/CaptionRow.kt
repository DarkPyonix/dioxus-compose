package dioxus.compose.runtime

import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Dp

/**
 * Where a bar's content sits against the strip the window buttons occupy, and how tall
 * that makes the bar.
 *
 * The bar is the window's caption where the tree opens with one, so its content and the
 * window buttons are laid out against the same strip. On the one platform that keeps its
 * own buttons the strip is the system's and its height is fixed: macOS centres the close,
 * minimise and zoom buttons in the standard title bar and offers no way to move them. A
 * window with a toolbar does get them centred lower, but that is `NSWindow.toolbarStyle`,
 * and the only things reachable from here are a handful of AWT client properties that do
 * not include it.
 *
 * So the content is what moves. Centring it in whatever height the bar ends up with is
 * what was happening before, and it put the title 15 pixels below the buttons on a bar
 * that had grown past the strip. Two cases instead, which is what the platform's own
 * toolbar styles are:
 *
 * - Content that fits the strip shares it, centred, on the buttons' line.
 * - Content that does not fit starts below the strip, because there is no way to bring
 *   the buttons down to meet it. This is the arrangement Windows 11 uses throughout, a
 *   caption with the application's own row under it.
 *
 * Returned in pixels because a layout works in pixels; the caller converts.
 */
internal data class CaptionRowPlacement(
    /** How far down the content starts. */
    val contentTop: Int,
    /** How tall the bar is, which is the content plus whatever the strip forced. */
    val height: Int,
)

internal fun captionRowPlacement(contentHeight: Int, bandHeight: Int): CaptionRowPlacement =
    when {
        // No strip: nothing to line up with, and the bar is as tall as it is.
        bandHeight <= 0 -> CaptionRowPlacement(contentTop = 0, height = contentHeight)
        contentHeight <= bandHeight ->
            CaptionRowPlacement(contentTop = (bandHeight - contentHeight) / 2, height = bandHeight)
        else -> CaptionRowPlacement(contentTop = bandHeight, height = bandHeight + contentHeight)
    }

/**
 * Lays a bar's content against the strip the window buttons occupy.
 *
 * A modifier rather than a wrapper, so the decoration that paints the bar still covers the
 * whole of it including the strip. Painting only around the content would leave the
 * window's own background showing where the buttons are, which reads as a leftover title
 * bar rather than as a bar that runs to the top of the window.
 */
internal fun Modifier.captionRow(band: Dp): Modifier = layout { measurable, constraints ->
    // Measured without the incoming minimum, because the question is how tall the content
    // wants to be, and a minimum from the parent would answer it before it was asked.
    val placeable = measurable.measure(constraints.copy(minHeight = 0))
    val placement = captionRowPlacement(placeable.height, band.roundToPx())
    layout(placeable.width, placement.height) {
        placeable.place(0, placement.contentTop)
    }
}
