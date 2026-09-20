package org.thisisthepy.dioxus.compose

import androidx.compose.material3.Text
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.window.ComposeViewport

/**
 * Placeholder for the web host (SPEC PR-6, M7).
 *
 * The real one has Kotlin/Wasm own the single WebAssembly.Memory with Rust importing it,
 * and the two modules read the arena in place. This module exists now only to keep the
 * wasm-js product configuration.
 */
@OptIn(ExperimentalComposeUiApi::class)
fun main() {
    ComposeViewport { Text("dioxus-compose web host: not implemented yet (M7)") }
}
