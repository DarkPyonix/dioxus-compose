package dioxus.compose.ui.platform

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

    // Setting skiko.buffering here does nothing in a native image, measured: IOSurface
    // stays at 9408KB across 11 regions either way, while the same property on the JVM
    // drops it to 7696KB. Skiko's property holder is initialised when the image is built,
    // so a value written at startup arrives too late to be read.
}
