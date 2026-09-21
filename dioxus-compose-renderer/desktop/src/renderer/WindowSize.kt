package dioxus.compose.runtime

import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.WindowSizeClass

/**
 * The width in dp at which each size class begins.
 *
 * These are the Material 3 window size class boundaries, and the Host uses the same two
 * numbers. If the two sides ever disagree, the Compose components adapt at one width and
 * the layout the Host authored switches at another, which is visible on screen.
 */
const val MEDIUM_MIN_WIDTH_DP: Float = 600f
const val EXPANDED_MIN_WIDTH_DP: Float = 840f

/** The class a window of this width belongs to. Height does not take part. */
fun windowSizeClassOf(widthDp: Float): WindowSizeClass = when {
    widthDp >= EXPANDED_MIN_WIDTH_DP -> WindowSizeClass.Expanded
    widthDp >= MEDIUM_MIN_WIDTH_DP -> WindowSizeClass.Medium
    else -> WindowSizeClass.Compact
}

/**
 * Tells the Host which size class the window is in, and only when that changes.
 *
 * Dragging a window edge produces a layout pass many times a second. Sending the size on
 * each of them would run the Host's VirtualDom just as often, for a value almost every
 * layout would report as unchanged. So the last class sent is remembered and compared, and
 * nothing crosses the boundary until it differs.
 *
 * The remembered class starts at [WindowSizeClass.Compact] because that is what a Host
 * assumes before it hears anything. A window that opens phone-sized is therefore silent:
 * telling the Host what it already believes would cost a boundary call and a VirtualDom
 * pass and change nothing. A window that opens wider reports once, on its first
 * measurement.
 *
 * The dp measurements ride along with the class rather than being reported on their own,
 * because a Host that needs pixel-by-pixel sizes is asking for work that belongs in
 * Compose's own layout.
 */
class WindowSizeReporter {
    private var lastClass: WindowSizeClass = WindowSizeClass.Compact

    /**
     * Reports one measurement. Returns whether an event was sent.
     *
     * Nothing is allocated when the class is unchanged, which is the common case.
     */
    fun report(widthDp: Float, heightDp: Float, dispatcher: EventDispatcher): Boolean {
        val sizeClass = windowSizeClassOf(widthDp)
        if (sizeClass == lastClass) return false
        lastClass = sizeClass
        dispatcher.dispatch(
            HostEvent.WindowSizeChanged(
                nodeId = 0,
                handlerId = 0,
                widthDp = widthDp,
                heightDp = heightDp,
                sizeClass = sizeClass,
            ),
        )
        return true
    }
}
