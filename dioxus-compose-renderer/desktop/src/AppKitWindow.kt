@file:JvmName("AppKitWindow")

package dioxus.compose.ui.platform

import org.graalvm.nativeimage.StackValue
import org.graalvm.nativeimage.c.function.CFunction
import org.graalvm.nativeimage.c.type.CCharPointer
import org.graalvm.nativeimage.c.type.CIntPointer
import org.graalvm.nativeimage.c.type.CFloatPointer
import org.graalvm.nativeimage.c.type.CTypeConversion
import org.graalvm.word.Pointer

// A window that is ours, drawn into with Skia and with no toolkit in between.
//
// Beside `NativeHostConnection` rather than among the renderer's own files, and for the
// same reason: this is the only other place that names GraalVM types, so a development
// run on a JVM never loads them.
//
// The C side is `c/appkit_window.m`. It owns the window, the view, the Metal device and
// the queue, and answers with the four pointers. Nothing there draws.

@CFunction("dxc_native_window_open")
private external fun openWindow(
    title: CCharPointer?,
    width: Int,
    height: Int,
    out: Pointer?,
): Int

@CFunction("dxc_native_window_size")
private external fun windowSize(
    view: Pointer?,
    width: CIntPointer?,
    height: CIntPointer?,
    scale: CFloatPointer?,
)

@CFunction("dxc_native_window_present")
private external fun presentWindow(view: Pointer?, queue: Pointer?)

@CFunction("dxc_native_window_run")
private external fun runEventLoop()

/**
 * The four pointers a window is, once AppKit has made one.
 *
 * Words rather than objects, because that is what crosses: native-image accepts a word
 * value in straight-line code inside one method and nowhere else, so each is read out
 * once, here, and carried as a plain `Long` after that.
 */
class NativeWindow internal constructor(
    val window: Long,
    val view: Long,
    val device: Long,
    val queue: Long,
) {

    /** The size of the drawable in pixels, and how many of them go to a point. */
    fun measure(): WindowMeasurement {
        val width = StackValue.get<CIntPointer>(4)
        val height = StackValue.get<CIntPointer>(4)
        val scale = StackValue.get<CFloatPointer>(4)
        windowSize(pointerOf(view), width, height, scale)
        return WindowMeasurement(width.read(), height.read(), scale.read())
    }

    /** Ends the frame Skia has just painted. */
    fun present() = presentWindow(pointerOf(view), pointerOf(queue))

    /** Hands the thread to AppKit. Returns when the window closes. */
    fun run() = runEventLoop()
}

data class WindowMeasurement(val width: Int, val height: Int, val scale: Float)

/**
 * Opens a window, or null where this machine has no Metal device.
 *
 * Null rather than an exception: a machine without Metal is not a mistake in this code,
 * and the caller has an older path it can take instead.
 */
fun openNativeWindow(title: String, width: Int, height: Int): NativeWindow? {
    val holder = CTypeConversion.toCString(title)
    try {
        // Four pointers, in the order the C struct declares them.
        val out = StackValue.get<Pointer>(WINDOW_STRUCT_BYTES)
        if (openWindow(holder.get(), width, height, out) != 0) {
            return null
        }
        return NativeWindow(
            window = out.readWord<Pointer>(0).rawValue(),
            view = out.readWord<Pointer>(8).rawValue(),
            device = out.readWord<Pointer>(16).rawValue(),
            queue = out.readWord<Pointer>(24).rawValue(),
        )
    } finally {
        holder.close()
    }
}

private const val WINDOW_STRUCT_BYTES = 32

private fun pointerOf(value: Long): Pointer = org.graalvm.word.WordFactory.pointer(value)

/**
 * Draws one colour into a window of our own, and holds it there.
 *
 * The first step of standing the renderer up without a toolkit, and the one worth taking
 * first: if Skia can be handed a drawable that AppKit gave us and the result reaches the
 * screen, everything after it is plumbing. If it cannot, nothing after it matters.
 *
 * Reached by setting `DXC_APPKIT_WINDOW`, so the ordinary path is untouched.
 */
internal fun runAppKitSpike() {
    val window = openNativeWindow("dioxus-compose", 520, 360)
    if (window == null) {
        System.err.println("dioxus-compose: this machine has no Metal device")
        return
    }
    val context = org.jetbrains.skia.DirectContext.makeMetal(window.device, window.queue)
    val measured = window.measure()
    System.err.println(
        "dioxus-compose: a window of our own, ${measured.width}x${measured.height} " +
            "at ${measured.scale}x, with no toolkit in it",
    )
    val surface = org.jetbrains.skia.Surface.makeFromMTKView(
        context,
        window.view,
        org.jetbrains.skia.SurfaceOrigin.TOP_LEFT,
        1,
        org.jetbrains.skia.SurfaceColorFormat.BGRA_8888,
        org.jetbrains.skia.ColorSpace.sRGB,
        org.jetbrains.skia.SurfaceProps(org.jetbrains.skia.PixelGeometry.RGB_H),
    )
    // A colour nothing else on a desktop is, so a screenshot cannot be read as a window
    // that happened to be there.
    surface.canvas.clear(0xFF2E7D32.toInt())
    context.flush()
    surface.close()
    window.present()
    window.run()
}
