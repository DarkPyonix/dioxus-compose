package dioxus.compose.ui.platform

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import kotlinx.coroutines.delay
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.HostConnection
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.runtime.WindowCaption
import dioxus.compose.runtime.LocalSystemDarkObserver

/**
 * Runs the renderer's Compose application on the calling thread until its window closes.
 *
 * The window draws the interpreted Host tree and nothing else: every widget on screen comes
 * from the mutation batches `connection` streams.
 *
 * `exitProcessOnExit` is off because the process belongs to the Rust host: closing the
 * window must return control to it, not terminate it.
 */
internal fun runRenderer(
    autoExitMillis: Long? = null,
    chrome: WindowChrome = WindowChrome.Modern,
    connection: () -> HostConnection,
) = application(exitProcessOnExit = false) {
    // Undecorated everywhere the platform will not hand us a transparent title bar, which
    // is everywhere except macOS. There we keep the real one and make it see through, so
    // the close, minimise and zoom buttons stay the system's own.
    val undecorated = chrome == WindowChrome.Modern && !platformDrawsWindowButtons
    Window(
        onCloseRequest = ::exitApplication,
        title = "DioxusCompose",
        undecorated = undecorated,
    ) {
        // AWT reads the macOS client properties when the peer is realised, so this runs
        // once the window exists rather than as a constructor argument.
        LaunchedEffect(chrome) { applyWindowChrome(window, chrome) }

        // The tracing agent writes its output only on a clean shutdown, so unattended
        // metadata collection needs the window to close by itself.
        autoExitMillis?.let { timeout ->
            LaunchedEffect(Unit) {
                delay(timeout)
                exitApplication()
            }
        }
        // Compose's own reading of the system appearance is taken once on this platform, so
        // the window is given one that keeps looking. It is provided here, around this
        // window's content, rather than stored anywhere a later composition could inherit
        // it: the observer polls in a loop that never ends, which a window wants and a
        // test clock cannot survive.
        CompositionLocalProvider(
            LocalSystemDarkObserver provides { rememberSystemDark().value },
        ) {
            DioxusContent(
                rememberDioxusHost(remember { connection() }),
                Modifier.fillMaxSize(),
                // Content runs under the caption on purpose, but a widget sitting where
                // the window buttons are would leave both unusable. What to do about that
                // depends on what the Host declared, so the window says how much room the
                // buttons take and the content decides: a tree that leads with a bar makes
                // that bar the caption, and one that does not is pushed clear of them.
                caption = windowCaption(window, chrome),
            )
        }
    }
}
