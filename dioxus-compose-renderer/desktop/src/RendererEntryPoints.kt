@file:JvmName("RendererEntryPoints")

package dioxus.compose.ui.platform

import org.graalvm.nativeimage.IsolateThread
import org.graalvm.nativeimage.c.function.CEntryPoint
import org.graalvm.nativeimage.c.type.CCharPointer
import org.graalvm.nativeimage.c.type.CTypeConversion
import dioxus.compose.ui.platform.NativeHostConnection

// C entry points of the renderer shared library (SPEC PR-2).
//
// The public symbols `dioxus_compose_renderer_run` and `dioxus_compose_renderer_request_frame`
// take no isolate argument; the C shim in `c/renderer_entry.c` owns the isolate and calls
// these `_impl` functions with the current isolate thread.
//
// These are top-level functions because @CEntryPoint needs genuinely static methods: an
// `object` with @JvmStatic compiles to a static bridge that passes the word-typed isolate
// thread on to an instance method, which native-image rejects.
//
// Word-typed parameters are declared nullable for the same reason: Kotlin inserts a
// `checkNotNullParameter` call for every non-null reference parameter, and that call would
// pass the word value as an Object.

@CEntryPoint(name = "dioxus_compose_renderer_run_impl")
fun rendererRun(thread: IsolateThread?, libraryDir: CCharPointer?): Int =
    try {
        configureRuntimeLayout(CTypeConversion.toJavaString(libraryDir))
        // Lets automated smoke tests close the window; unset in normal use.
        runRenderer(System.getenv("DIOXUS_COMPOSE_AUTOEXIT_MS")?.toLongOrNull()) {
            NativeHostConnection()
        }
        0
    } catch (t: Throwable) {
        // Nothing may unwind across the C boundary (NFR-7).
        t.printStackTrace()
        1
    }

@CEntryPoint(name = "dioxus_compose_renderer_request_frame_impl")
fun rendererRequestFrame(thread: IsolateThread?) {
    FrameRequests.request()
}
