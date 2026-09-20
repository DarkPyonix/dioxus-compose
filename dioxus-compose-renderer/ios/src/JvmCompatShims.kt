package org.thisisthepy.dioxus.compose.renderer

import kotlin.reflect.KClass
import platform.Foundation.NSLog

/**
 * The handful of JVM names the interpreter sources use, supplied for Kotlin/Native.
 *
 * The interpreter in `desktop/src/renderer/` is written for the desktop native image and is
 * compiled unchanged for iOS (`src/shared/` symlinks it). It is ordinary Compose code apart
 * from these four names, which have no Kotlin/Native equivalent. Declaring them here in the
 * interpreter's own package keeps the shared sources free of `expect`/`actual` scaffolding
 * that only one target would ever need.
 *
 * Each is a real implementation, not a stub:
 * - `System.getProperty("os.name")` answers `ios`, which is what `detectHostPlatform` reads
 *   to pick the Cupertino design system on this platform (SPEC FR-14.3).
 * - `System.err.println` goes to `NSLog`, so a protocol error reaches the device console.
 * - `KClass.java.name` answers the qualified class name, as it does on the JVM.
 * - `InterruptedException` exists so the `catch` clauses that re-throw it still compile.
 *   Nothing on iOS throws it: there is no `Thread.interrupt`.
 */
object System {
    val err: ErrorStream = ErrorStream()

    fun getProperty(name: String): String? = when (name) {
        "os.name" -> "ios"
        else -> null
    }

    fun getenv(name: String): String? = null

    class ErrorStream internal constructor() {
        fun println(line: String) = NSLog("%s", line)
    }
}

class InterruptedException(message: String? = null) : Exception(message)

/**
 * A boundary call that returned a non-zero status (SPEC NFR-7: reported, never fatal).
 *
 * The same type the desktop renderer declares in `NativeHostConnection.kt`, which is a
 * GraalVM-only file and so is not shared with this target.
 */
class HostCallException(message: String) : RuntimeException(message)

class JavaClassName internal constructor(val name: String)

val KClass<*>.java: JavaClassName
    get() = JavaClassName(qualifiedName ?: simpleName ?: "unknown")
