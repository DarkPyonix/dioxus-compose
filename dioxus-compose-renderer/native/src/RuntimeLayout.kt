package org.thisisthepy.dioxus.compose.nativeimage

import java.io.File

/**
 * Points AWT and Skiko at the directory the renderer library was loaded from.
 *
 * `java.home` is unset in a native image, and AWT resolves its libraries against it:
 * `<java.home>/lib/libjawt.dylib` on macOS. The distribution keeps every runtime file in
 * one `lib` directory, so its parent is a valid `java.home`.
 *
 * Skiko normally unpacks Skia from its jar into `~/.skiko`; a native image has no jar, so
 * the library is staged next to the renderer and Skiko is told where it is.
 *
 * Must run before anything touches AWT.
 */
internal fun configureRuntimeLayout(libraryDir: String) {
    val lib = File(libraryDir)
    if (System.getProperty("java.home") == null) {
        System.setProperty("java.home", lib.parentFile.absolutePath)
    }
    System.setProperty("skiko.library.path", lib.absolutePath)
    System.setProperty("skiko.data.path", lib.absolutePath)
}
