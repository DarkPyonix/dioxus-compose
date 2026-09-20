package dioxus.compose.ui.platform

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import kotlinx.coroutines.delay
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.HostConnection
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.runtime.systemDarkObserver

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
    // Compose's own reading of the system appearance is taken once on this platform, so
    // the renderer is given one that keeps looking.
    LaunchedEffect(Unit) { systemDarkObserver = { rememberSystemDark().value } }

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
        DioxusContent(
            rememberDioxusHost(remember { connection() }),
            Modifier.fillMaxSize(),
            // Content runs under the caption on purpose, but a widget sitting where the
            // window buttons are would leave both unusable. The inset goes inside the
            // content's own background so the window has one continuous surface.
            contentPadding = windowContentInsets(window, chrome, hasTopAppBar = false),
        )
    }
}
