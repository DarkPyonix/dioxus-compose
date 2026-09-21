package dioxus.compose.ui.platform

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.window.WindowDraggableArea
import androidx.compose.ui.window.application
import kotlinx.coroutines.delay
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.HostConnection
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.runtime.LocalSystemDarkObserver
import dioxus.compose.runtime.LocalWindowActions

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
            // Only where we draw the caption ourselves. macOS keeps the system's own
            // traffic lights, and a second set drawn beside them would be two sets of
            // buttons on one window.
            LocalWindowActions provides windowActions(window, chrome),
        ) {
            val caption = windowCaption(window, chrome)
            Box(Modifier.fillMaxSize()) {
                // A strip across the top of the window that moves it when dragged, drawn
                // before the content rather than over it. Compose hit-tests front to
                // back, so a widget in the caption gets the press and the strip only sees
                // the empty room around it, which is what "drag the caption" has to mean.
                if (caption.height > 0.dp) {
                    WindowDraggableArea(Modifier.fillMaxWidth().height(caption.height)) {}
                }
                DioxusContent(
                    rememberDioxusHost(remember { connection() }),
                    Modifier.fillMaxSize(),
                    // Content runs under the caption on purpose, but a widget sitting
                    // where the window buttons are would leave both unusable. The strip
                    // goes inside the content's own background so the window has one
                    // continuous surface, and a tree that opens with a bar hands it to
                    // the bar instead.
                    caption = caption,
                )
            }
        }
    }
}
