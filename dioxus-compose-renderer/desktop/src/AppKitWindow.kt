@file:JvmName("AppKitWindow")

package dioxus.compose.ui.platform

import org.graalvm.nativeimage.StackValue
import org.graalvm.nativeimage.c.function.CFunction
import org.graalvm.nativeimage.c.type.CCharPointer
import org.graalvm.nativeimage.c.type.CIntPointer
import org.graalvm.nativeimage.c.type.CFloatPointer
import org.graalvm.nativeimage.c.type.CTypeConversion
import org.graalvm.word.Pointer
import org.graalvm.word.WordFactory

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

@CFunction("dxc_native_frame_begin")
private external fun beginFrame(layer: Pointer?, textureOut: Pointer?): Int

@CFunction("dxc_native_frame_end")
private external fun endFrame(queue: Pointer?)

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
    val layer: Long,
) {

    // A word value is made where it is used and nowhere else. Native-image accepts one
    // in straight-line code inside a single method, so a helper that returned one, or a
    // variable that held one across a call, is rejected: `WordFactory.pointer` is
    // written out at each call rather than wrapped.

    /** The size of the drawable in pixels, and how many of them go to a point. */
    fun measure(): WindowMeasurement {
        val width = StackValue.get<CIntPointer>(4)
        val height = StackValue.get<CIntPointer>(4)
        val scale = StackValue.get<CFloatPointer>(4)
        windowSize(WordFactory.pointer(layer), width, height, scale)
        return WindowMeasurement(width.read(), height.read(), scale.read())
    }

    /**
     * The texture this frame paints into, or zero where the system had none to give.
     *
     * Zero is not a failure. It means frames are being produced faster than the screen
     * takes them, and the answer to that is to skip one rather than to wait.
     */
    fun beginFrame(): Long {
        val texture = StackValue.get<Pointer>(8)
        if (beginFrame(WordFactory.pointer(layer), texture) != 0) return 0
        return texture.readWord<Pointer>(0).rawValue()
    }

    /** Puts the painted frame on the screen. */
    fun endFrame() = endFrame(WordFactory.pointer(queue))

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
            layer = out.readWord<Pointer>(32).rawValue(),
        )
    } finally {
        holder.close()
    }
}

private const val WINDOW_STRUCT_BYTES = 40

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
    val texture = window.beginFrame()
    if (texture == 0L) {
        System.err.println("dioxus-compose: no drawable to paint into")
        return
    }
    val target = org.jetbrains.skia.BackendRenderTarget.makeMetal(
        measured.width,
        measured.height,
        texture,
    )
    val surface = org.jetbrains.skia.Surface.makeFromBackendRenderTarget(
        context,
        target,
        org.jetbrains.skia.SurfaceOrigin.TOP_LEFT,
        org.jetbrains.skia.SurfaceColorFormat.BGRA_8888,
        org.jetbrains.skia.ColorSpace.sRGB,
        org.jetbrains.skia.SurfaceProps(org.jetbrains.skia.PixelGeometry.RGB_H),
    )!!
    // A colour nothing else on this desktop is, so a screenshot cannot be read as a
    // window that happened to be there.
    surface.canvas.clear(0xFF2E7D32.toInt())
    // Submitted, not only recorded. Skia's Metal backend records the frame into a command
    // buffer of its own, and a drawable presented before that buffer runs is a drawable
    // with nothing in it: the window came up black with the colour never reaching the GPU.
    surface.flushAndSubmit(true)
    surface.close()
    target.close()
    window.endFrame()
    // Held on screen while a screenshot is taken. The main thread is already running
    // AppKit; this one has nothing left to do but wait, and the process ends when it
    // stops waiting.
    Thread.sleep(SPIKE_HOLD_MILLIS)
}

private const val SPIKE_HOLD_MILLIS = 20_000L
