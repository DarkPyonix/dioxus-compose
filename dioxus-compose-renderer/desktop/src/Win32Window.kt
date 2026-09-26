@file:JvmName("Win32Window")
@file:OptIn(androidx.compose.ui.InternalComposeUiApi::class)

package dioxus.compose.ui.platform

import org.graalvm.nativeimage.StackValue
import org.graalvm.nativeimage.c.function.CFunction
import org.graalvm.nativeimage.c.type.CCharPointer
import org.graalvm.nativeimage.c.type.CIntPointer
import org.graalvm.nativeimage.c.type.CFloatPointer
import org.graalvm.nativeimage.c.type.CTypeConversion
import androidx.compose.ui.graphics.asComposeCanvas
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.scene.ComposeScene
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntSize
import org.graalvm.word.Pointer
import org.graalvm.word.WordFactory

// A window that is ours on Windows, drawn into with Skia and with no toolkit in between.
//
// The pair of `AppKitWindow.kt`, and beside it for the same reason it is beside
// `NativeHostConnection`: these are the only places that name GraalVM types, so a
// development run on a JVM never loads them.
//
// The C side is `c/win32_window.c`. It owns the window, the Direct3D device, the queue,
// the adapter and the swapchain, and answers with the pointers. Nothing there draws.
//
// The same C symbols the macOS file calls, because the two C files are alternatives:
// exactly one of them is compiled into an image, and each answers for the window its own
// platform knows how to open. What the five pointers mean differs, which is why each
// platform reads them here rather than sharing a struct. What an event looks like does
// not differ, so `WindowEvent` and `drainWindowEvents` are the macOS file's and used as
// they are, and so are the shell's own turns: `pumpWindowEvents` and `isWindowClosed`
// name what every window does and are declared once, there.

@CFunction("dxc_native_window_open")
private external fun openWindow(
    title: CCharPointer?,
    width: Int,
    height: Int,
    out: Pointer?,
): Int

@CFunction("dxc_native_window_size")
private external fun windowSize(
    window: Pointer?,
    width: CIntPointer?,
    height: CIntPointer?,
    scale: CFloatPointer?,
)

@CFunction("dxc_native_frame_begin")
private external fun beginFrame(swapchain: Pointer?, resourceOut: Pointer?): Int

@CFunction("dxc_native_frame_end")
private external fun endFrame(queue: Pointer?)

/**
 * The five pointers a window is, once Win32 and DXGI have made one.
 *
 * Words rather than objects, because that is what crosses: native-image accepts a word
 * value in straight-line code inside one method and nowhere else, so each is read out
 * once, here, and carried as a plain `Long` after that.
 */
class Win32NativeWindow internal constructor(
    val window: Long,
    val device: Long,
    val queue: Long,
    val adapter: Long,
    val swapchain: Long,
) {

    // A word value is made where it is used and nowhere else. Native-image accepts one
    // in straight-line code inside a single method, so a helper that returned one, or a
    // variable that held one across a call, is rejected: `WordFactory.pointer` is
    // written out at each call rather than wrapped.

    /** The size of the client area in pixels, and how many of them go to a point. */
    fun measure(): WindowMeasurement {
        val width = StackValue.get<CIntPointer>(4)
        val height = StackValue.get<CIntPointer>(4)
        val scale = StackValue.get<CFloatPointer>(4)
        windowSize(WordFactory.pointer(window), width, height, scale)
        return WindowMeasurement(width.read(), height.read(), scale.read())
    }

    /**
     * The buffer this frame paints into, or zero where the swapchain had none to give.
     *
     * Zero is not a failure. The window may have been closed or minimised, or it may have
     * just been given a size the swapchain has yet to be made to fit. The answer to all
     * three is to skip the frame rather than to wait.
     */
    fun beginFrame(): Long {
        val resource = StackValue.get<Pointer>(8)
        if (beginFrame(WordFactory.pointer(swapchain), resource) != 0) return 0
        return resource.readWord<Pointer>(0).rawValue()
    }

    /** Puts the painted frame on the screen. */
    fun endFrame() = endFrame(WordFactory.pointer(queue))

}

/**
 * Opens a window, or null where this machine has no Direct3D 12 adapter.
 *
 * Null rather than an exception: a machine without one is not a mistake in this code, and
 * the caller has an older path it can take instead.
 */
fun openWin32Window(title: String, width: Int, height: Int): Win32NativeWindow? {
    val holder = CTypeConversion.toCString(title)
    try {
        // Five pointers, in the order the C struct declares them.
        val out = StackValue.get<Pointer>(WINDOW_STRUCT_BYTES)
        if (openWindow(holder.get(), width, height, out) != 0) {
            return null
        }
        return Win32NativeWindow(
            window = out.readWord<Pointer>(0).rawValue(),
            device = out.readWord<Pointer>(8).rawValue(),
            queue = out.readWord<Pointer>(16).rawValue(),
            adapter = out.readWord<Pointer>(24).rawValue(),
            swapchain = out.readWord<Pointer>(32).rawValue(),
        )
    } finally {
        holder.close()
    }
}

private const val WINDOW_STRUCT_BYTES = 40

/**
 * What the swapchain was made with, which Skia has to be told again.
 *
 * `DXGI_FORMAT_R8G8B8A8_UNORM`. Named by its number because the C side holds the header
 * this comes from and nothing on this side can see it, and repeated rather than asked for
 * because a format that disagrees between the two is a window of swapped colour channels
 * rather than a failure anything reports.
 */
private const val SWAPCHAIN_FORMAT = 28

/**
 * Draws the application into a window of our own, and holds it there until it closes.
 *
 * The Windows half of the step everything after it rests on: a scene that Compose
 * composed, painted by Skia into a swapchain buffer Direct3D gave us, reaching the screen
 * with no toolkit anywhere between.
 *
 * Reached by setting `DXC_WIN32_WINDOW`, so the ordinary path is untouched.
 */
internal fun runWin32Window() {
    // The Host is started before there is a window, because what the window should look
    // like is in its first batch and a window cannot be told afterwards. Started on this
    // thread, which is the one every later call to it is made from: the boundary is a
    // direct call on one thread and the Host keeps its state there.
    val host = dioxus.compose.runtime.DioxusHost(NativeHostConnection())
    host.start()
    val asked = host.table.window
    val window = openWin32Window(
        asked?.title?.takeIf { it.isNotEmpty() } ?: "dioxus-compose",
        if (asked != null && asked.width > 0) asked.width else 520,
        if (asked != null && asked.height > 0) asked.height else 360,
    )
    if (window == null) {
        System.err.println("dioxus-compose: this machine has no Direct3D 12 adapter")
        host.shutdown()
        return
    }
    val context = org.jetbrains.skia.DirectContext.makeDirect3D(
        window.adapter,
        window.device,
        window.queue,
    )
    val measured = window.measure()
    System.err.println(
        "dioxus-compose: a window of our own, ${measured.width}x${measured.height} " +
            "at ${measured.scale}x, with no toolkit in it",
    )

    val report = System.getenv("DXC_REPORT_INPUT") != null
    // Held rather than measured once. The window is resizable, and what the scene is told
    // about the window it sits in is read from here.
    var size = IntSize(measured.width, measured.height)
    val textInput = NativeTextInput()
    // What the window would tell a reader who cannot see it, read after each frame that
    // painted and handed on when it has changed. Where it is handed on to is the
    // platform's own accessibility, which is not in this file: on Windows that is UI
    // Automation, and it is being written against this call.
    val semantics = NativeSemantics { elements ->
        if (report) {
            System.err.println("dioxus-compose: the window has ${elements.size} things to say")
        }
    }
    // Kept rather than left to the scene. What a scene picks for itself is the toolkit's
    // queue, and the Host this renderer talks to is on this thread and invisible from
    // there: a list asking for the rows it is about to show asked from a thread with no
    // Host and was told nothing had been initialised.
    val work = FrameDispatcher()
    val scene = CanvasLayersComposeScene(
        density = Density(measured.scale),
        size = size,
        coroutineContext = work,
        platformContext = NativePlatformContext({ size }, textInput, semantics),
    )
    // The application's own tree, drawn by the same interpreter the toolkit path uses.
    // Nothing in it knows which of the two it is running on, which is the point.
    scene.setContent { dioxus.compose.runtime.DioxusContent(host) }

    var nanos = 0L
    var painted = false

    try {
        // Rests only when the last turn found nothing to do. A frame that drew has
        // already waited for the screen inside `Present`, and waiting again on top of
        // that would halve the rate of anything that animates.
        var busy = true
        while (!isWindowClosed()) {
            var drew = false
            // The window's own turn, before anything is read from it. This thread is the
            // one Windows delivers to, so the messages of this frame arrive here or not
            // at all.
            pumpWindowEvents(if (busy) 0.0 else FRAME_SECONDS)
            work.runPending()
            var heard = false
            for (event in drainWindowEvents()) {
                if (report && event.kind != WindowEvent.POINTER_MOVE) {
                    System.err.println("dioxus-compose: window heard $event")
                }
                scene.receive(event)
                textInput.receive(event)
                heard = true
            }
            // Only when there is something to draw. A window that is being looked at
            // rather than used should cost a comparison a frame.
            if (!painted || heard || scene.hasInvalidations()) {
                nanos += FRAME_NANOS
                val at = drawFrame(window, context, scene, nanos)
                if (at != null) {
                    // The size the buffer really is, which is the size the scene was just
                    // drawn at and the size the window is told it has.
                    size = at
                    painted = true
                    drew = true
                }
            }
            // Every frame, and after the drawing. After, because that is when what is in
            // the window has been placed and can say where it is. Every frame, because a
            // tree that changed on the last one is a tree nobody has been told about, and
            // a window that has gone still is exactly where that would be forgotten.
            semantics.pushIfChanged(afterDrawing = drew)
            busy = heard || drew
        }
    } finally {
        scene.close()
        context.close()
        host.shutdown()
    }
}

/**
 * One frame: take a buffer, let the scene paint it, give it to the screen.
 *
 * Answers the size it was drawn at, which is the swapchain's and not the size anything
 * asked for: a resize is taken inside the call that hands this frame its buffer, so the
 * size that comes back is the one the buffer really is. Null where there was nothing to
 * draw into. A closed window answers that, a minimised one answers it for as long as it
 * stays down, and a swapchain that could not be made to fit a size it was given answers
 * it once.
 */
private fun drawFrame(
    window: Win32NativeWindow,
    context: org.jetbrains.skia.DirectContext,
    scene: ComposeScene,
    nanos: Long,
): IntSize? {
    val resource = window.beginFrame()
    if (resource == 0L) return null
    // After the buffer and not before it, because that is where a swapchain waiting to be
    // refitted is refitted, and what is measured here is the buffer that came back.
    val measured = window.measure()
    val fitted = IntSize(measured.width, measured.height)
    val density = Density(measured.scale)
    // The window was resized, or moved onto a screen of another density. Told to the
    // scene here, because a buffer that fits and a scene that does not is a window drawing
    // its old size into a corner of its new one.
    if (scene.size != fitted || scene.density != density) {
        scene.density = density
        scene.size = fitted
    }
    val target = org.jetbrains.skia.BackendRenderTarget.makeDirect3D(
        measured.width,
        measured.height,
        resource,
        SWAPCHAIN_FORMAT,
        // One sample and one level: a swapchain buffer is neither multisampled nor
        // mipmapped, and saying otherwise would have Skia describe a buffer that is not
        // the one it was handed.
        1,
        1,
    )
    val surface = org.jetbrains.skia.Surface.makeFromBackendRenderTarget(
        context,
        target,
        org.jetbrains.skia.SurfaceOrigin.TOP_LEFT,
        org.jetbrains.skia.SurfaceColorFormat.RGBA_8888,
        org.jetbrains.skia.ColorSpace.sRGB,
        org.jetbrains.skia.SurfaceProps(org.jetbrains.skia.PixelGeometry.RGB_H),
    )
    if (surface == null) {
        target.close()
        window.endFrame()
        return null
    }
    scene.render(surface.canvas.asComposeCanvas(), nanos)
    // Submitted, not only recorded. Skia's Direct3D backend keeps the frame in a command
    // list of its own, and a buffer presented before that list runs is a buffer with
    // nothing in it: on macOS the same mistake made the window come up black with the
    // paint never reaching the GPU.
    surface.flushAndSubmit(true)
    surface.close()
    target.close()
    // Waits for the screen, so there is no sleep after this: presenting with an interval
    // of one is what paces a frame that was drawn.
    window.endFrame()
    return fitted
}

private const val FRAME_SECONDS = 0.016
private const val FRAME_NANOS = 16_000_000L
