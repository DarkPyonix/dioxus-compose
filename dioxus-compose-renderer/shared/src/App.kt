package org.thisisthepy.dioxus.compose

import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import org.jetbrains.compose.reload.DevelopmentEntryPoint

/**
 * The JVM development shell's screen (INTENT D7).
 *
 * This is not the renderer. The renderer is `native/`, which the Host drives over the C
 * boundary and which is built as a native image. This module exists only so hot reload
 * and `@Preview` are available while working on Compose side behaviour, and the IME
 * checklist (docs/SPEC.md section 6) is the thing worth having there.
 */
@Composable
@Preview
@DevelopmentEntryPoint
fun App() {
    ImeTestScreen()
}
