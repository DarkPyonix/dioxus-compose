package dioxus.compose.test

import dioxus.compose.ui.platform.WindowChrome
import dioxus.compose.ui.platform.platformDrawsWindowButtons
import dioxus.compose.ui.platform.windowCaption
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * What a Windows window keeps, and why it keeps it.
 *
 * An undecorated window on Windows is not a window without a title bar. It is a window
 * without a drop shadow, without a resize border and without Snap Layouts, because the
 * frame provides all three and nothing here can draw them. The first person to run this
 * on Windows said the borders looked crude, and that is what they were looking at.
 *
 * So the frame stays and the system draws the bar. The tests below fix the two
 * consequences that would otherwise be easy to undo by accident: nothing of ours is drawn
 * over the system's buttons, and no content is laid out under a bar that is really there.
 */
class WindowsChromeTest {

    @Test
    fun fr19_windows_keeps_its_frame_and_the_buttons_that_come_with_it() {
        assertTrue(
            platformDrawsWindowButtons("Windows 11"),
            "an undecorated window on Windows loses the shadow, the resize border and " +
                "Snap Layouts with the bar, and a second set of buttons drawn beside the " +
                "system's would be the visible half of that mistake",
        )
        assertTrue(platformDrawsWindowButtons("Mac OS X"), "the traffic lights stay")
        assertFalse(
            platformDrawsWindowButtons("Linux"),
            "Linux has no equivalent of the macOS client properties, so the caption is " +
                "ours to draw there and the buttons with it",
        )
    }

    @Test
    fun fr19_a_window_whose_bar_the_system_drew_reserves_no_strip() {
        assertEquals(
            0.0f,
            windowCaption(null, WindowChrome.Modern, "Windows 11").height.value,
            "content was laid out under a bar, and on Windows that bar is really there",
        )
        assertEquals(
            0.0f,
            windowCaption(null, WindowChrome.System, "Linux").height.value,
            "a window that asked for the system bar never had a strip to give away",
        )
        // Linux still paints its own, so the strip it reserves is not zero.
        assertTrue(
            windowCaption(null, WindowChrome.Modern, "Linux").height.value > 0f,
            "Linux draws the caption itself and needs the room for it",
        )
    }
}
