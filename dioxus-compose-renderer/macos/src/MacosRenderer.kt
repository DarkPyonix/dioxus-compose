package dioxus.compose.ui.platform

import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.HostConnection
import dioxus.compose.runtime.rememberDioxusHost
import platform.AppKit.NSApplication
import platform.AppKit.NSApplicationActivationPolicy

/**
 * Runs the renderer's Compose application. This is what `dioxus_compose_renderer_run`
 * calls.
 *
 * The window is Compose's own for this platform, which is a real `NSWindow` drawing
 * through Skia, so the input method, the cursor and the tracking areas are the ones
 * Compose already has rather than ones written here again. The native image path has to
 * build all of that itself because on that path Compose is the toolkit's.
 *
 * Returns when the application stops, which is when the window closes.
 */
internal fun runRenderer(connection: () -> HostConnection): Int {
    val application = NSApplication.sharedApplication()
    // An executable that is not inside a bundle is not, by default, something the system
    // will put in front of anything else: it has no place in the dock and cannot take the
    // keyboard. Said before the window is made, because what kind of application this is
    // decides what its windows are allowed to be sent.
    application.setActivationPolicy(
        NSApplicationActivationPolicy.NSApplicationActivationPolicyRegular,
    )

    val host = connection()
    Window(title = "DioxusCompose", size = DpSize(800.dp, 600.dp)) {
        DioxusContent(rememberDioxusHost(host))
    }

    application.activateIgnoringOtherApps(true)
    application.run()
    return 0
}
