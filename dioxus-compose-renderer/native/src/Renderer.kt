package org.thisisthepy.dioxus.compose.nativeimage

import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import kotlinx.coroutines.delay
import org.thisisthepy.dioxus.compose.App

/**
 * Runs the renderer's Compose application on the calling thread until its window closes.
 *
 * `exitProcessOnExit` is off because the process belongs to the Rust host: closing the
 * window must return control to it, not terminate it.
 */
internal fun runRenderer(autoExitMillis: Long? = null) = application(exitProcessOnExit = false) {
    Window(onCloseRequest = ::exitApplication, title = "DioxusCompose") {
        // The tracing agent writes its output only on a clean shutdown, so unattended
        // metadata collection needs the window to close by itself.
        autoExitMillis?.let { timeout ->
            LaunchedEffect(Unit) {
                delay(timeout)
                exitApplication()
            }
        }
        App()
    }
}
