@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

package dioxus.compose.ui.platform

import kotlin.test.Test
import kotlin.test.assertNull
import platform.posix.unsetenv

/**
 * What the window does when there is no display server to open one on.
 *
 * The only thing about the window itself that can be asserted where no X server is running, which
 * is every continuous integration machine and most of the machines this is developed on. It is
 * worth asserting because the alternative behaviours are both bad and both have happened to
 * windowing code: a throw that unwinds into the C entry point, which is undefined, and a process
 * that carries on with a null display and dies at the first call that dereferences it.
 *
 * Everything else about this window needs a screen and a hand: whether the drawing arrives with
 * the moved edge is a fact about what a compositor showed. `ResizeSyncTest` and
 * `WindowEventLogTest` on the JVM cover the two ways the code can get that wrong.
 */
class LinuxWindowTest {

    @Test
    fun nfr1_a_window_refuses_to_open_where_there_is_no_display_server() {
        // Taken away rather than pointed somewhere wrong. An empty DISPLAY and a DISPLAY naming a
        // server that is not there are different failures inside Xlib, and the one a machine with
        // no X server actually has is this one.
        unsetenv("DISPLAY")
        assertNull(
            LinuxWindow.open(title = "no display", width = 520, height = 360),
            "a window with no display server to open on answers null rather than throwing, " +
                "because what called in is a C entry point and a Kotlin exception must not " +
                "cross back over it",
        )
    }
}
