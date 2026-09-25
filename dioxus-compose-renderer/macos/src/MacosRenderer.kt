package dioxus.compose.ui.platform

import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.DioxusHost
import dioxus.compose.runtime.HostConnection
import platform.AppKit.NSApplication
import platform.AppKit.NSApplicationActivationPolicy

/**
 * Runs the renderer's Compose application. This is what `dioxus_compose_renderer_run`
 * calls.
 *
 * The window is this renderer's own. Compose opens one for this platform and it draws
 * correctly, but nothing it opens says what is in it: a reader is given the three title
 * bar buttons and the title, and every control the application drew is invisible to it.
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

    // Started before there is a window, because what the window should look like is in
    // the first batch and a window cannot be told afterwards: how big it is and what it
    // is called are settled when it is made.
    val host = DioxusHost(connection())
    host.start()
    // What the application asked for. A window that said nothing is listed under whatever
    // this renderer happens to be called, which is the library's name and not any
    // application's, and a measurement of zero means it did not ask.
    val asked = host.table.window
    val window = MacosWindow(
        name = asked?.title?.takeIf { it.isNotEmpty() } ?: "dioxus-compose",
        width = if (asked != null && asked.width > 0) asked.width else 520,
        height = if (asked != null && asked.height > 0) asked.height else 360,
    )
    window.setContent { DioxusContent(host) }

    application.activateIgnoringOtherApps(true)
    application.run()
    return 0
}
