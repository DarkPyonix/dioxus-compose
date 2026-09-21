package dioxus.compose

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.remember
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.window.ComposeViewport
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.m0DemoHost

/**
 * The web renderer's entry point.
 *
 * `ComposeViewport` is the browser's counterpart of the desktop `Window` and of the iOS
 * `ComposeUIViewController`: it takes over the page's canvas and composes into it. The
 * screen is drawn entirely from the mutation batches the Host streams, exactly as it is on
 * every other target, because the interpreter under it is the same source.
 *
 * The Host is the scripted [m0DemoHost] until the Rust module is wired in, which is the
 * same development shell the JVM target has and is what NFR-5 asks for: the renderer must
 * be workable without building the other side.
 */
@OptIn(ExperimentalComposeUiApi::class)
fun main() {
    ComposeViewport {
        val connection = remember { m0DemoHost() }
        DioxusContent(rememberDioxusHost(connection), Modifier.fillMaxSize())
    }
}
