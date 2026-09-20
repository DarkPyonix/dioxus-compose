package org.thisisthepy.dioxus.compose.nativeimage

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import kotlinx.coroutines.delay
import org.thisisthepy.dioxus.compose.renderer.DioxusContent
import org.thisisthepy.dioxus.compose.renderer.HostConnection
import org.thisisthepy.dioxus.compose.renderer.rememberDioxusHost

/**
 * Runs the renderer's Compose application on the calling thread until its window closes.
 *
 * The window draws the interpreted Host tree and nothing else: every widget on screen comes
 * from the mutation batches `connection` streams (SPEC FR-1, FR-2).
 *
 * `exitProcessOnExit` is off because the process belongs to the Rust host: closing the
 * window must return control to it, not terminate it.
 */
internal fun runRenderer(
    autoExitMillis: Long? = null,
    connection: () -> HostConnection,
) = application(exitProcessOnExit = false) {
    Window(onCloseRequest = ::exitApplication, title = "DioxusCompose") {
        // The tracing agent writes its output only on a clean shutdown, so unattended
        // metadata collection needs the window to close by itself.
        autoExitMillis?.let { timeout ->
            LaunchedEffect(Unit) {
                delay(timeout)
                exitApplication()
            }
        }
        DioxusContent(rememberDioxusHost(remember { connection() }), Modifier.fillMaxSize())
    }
}
