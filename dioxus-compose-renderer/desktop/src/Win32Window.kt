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
// The same five C symbols the macOS file calls, because the two C files are alternatives:
// exactly one of them is compiled into an image, and each answers for the window its own
// platform knows how to open. What the five pointers mean differs, which is why each
// platform reads them here rather than sharing a struct. What an event looks like does
// not differ, so `WindowEvent` and `drainWindowEvents` are the macOS file's and used as
// they are.

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

@CFunction("dxc_native_set_accessibility")
private external fun setAccessibility(elements: Pointer?, count: Int, window: Pointer?)

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
 * Hands the platform what the window would tell a reader who cannot see it.
 *
 * Written into stack storage and copied on the other side. The elements are few, they
 * change when the screen changes rather than when a frame is drawn, and the alternative
 * is the platform asking across threads at a moment nobody chose.
 */
fun Win32NativeWindow.describeTo(elements: List<AccessibleElement>) {
    val capped = if (elements.size > MAX_ELEMENTS) elements.take(MAX_ELEMENTS) else elements
    val records = StackValue.get<Pointer>(MAX_ELEMENTS * ELEMENT_BYTES)
    for ((index, element) in capped.withIndex()) {
        val at = index * ELEMENT_BYTES
        records.writeInt(at, element.role)
        records.writeFloat(at + 4, element.x)
        records.writeFloat(at + 8, element.y)
        records.writeFloat(at + 12, element.width)
        records.writeFloat(at + 16, element.height)
        val bytes = win32LabelBytes(element.label)
        for (offset in bytes.indices) {
            records.writeByte(at + ELEMENT_LABEL_OFFSET + offset, bytes[offset])
        }
        records.writeByte(at + ELEMENT_LABEL_OFFSET + bytes.size, ZERO)
    }
    setAccessibility(records, capped.size, WordFactory.pointer(window))
}

/**
 * How many things a screen may say it has.
 *
 * Enough for a screen and not for a document. A list of ten thousand rows is windowed
 * before it reaches the scene, so what is here is what is on screen.
 */
private const val MAX_ELEMENTS = 256
private const val ELEMENT_LABEL_OFFSET = 20
private const val ELEMENT_BYTES = 116
private const val ZERO: Byte = 0
private const val TEXT_BYTES = 96

/** Fits a label in the native record without cutting a UTF-8 character in half. */
internal fun win32LabelBytes(label: String): ByteArray {
    val bytes = label.toByteArray(Charsets.UTF_8)
    if (bytes.size < TEXT_BYTES) return bytes
    var length = TEXT_BYTES - 1
    while (length > 0 && (bytes[length].toInt() and 0xC0) == 0x80) {
        length--
    }
    return bytes.copyOf(length)
}

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
 * Draws a Compose scene into a window of our own, and holds it there.
 *
 * The Windows half of the step everything after it rests on: a scene that Compose
 * composed, painted by Skia into a swapchain buffer Direct3D gave us, reaching the screen
 * with no toolkit anywhere between.
 *
 * Reached by setting `DXC_WIN32_WINDOW`, so the ordinary path is untouched.
 */
internal fun runWin32Spike() {
    val window = openWin32Window("dioxus-compose", 520, 360)
    if (window == null) {
        System.err.println("dioxus-compose: this machine has no Direct3D 12 adapter")
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
    var size = androidx.compose.ui.unit.IntSize(measured.width, measured.height)
    val textInput = NativeTextInput()
    val semantics = NativeSemantics { elements ->
        if (report) {
            System.err.println("dioxus-compose: the window has ${elements.size} things to say")
        }
        window.describeTo(elements)
    }
    val scene = CanvasLayersComposeScene(
        density = androidx.compose.ui.unit.Density(measured.scale),
        size = size,
        platformContext = NativePlatformContext({ size }, textInput, semantics),
    )
    scene.setContent { SpikeContent() }

    try {
        // A plain loop rather than a clock. Pacing is the frame clock's work and comes
        // later; what this has to show is that what the window hears reaches the scene
        // and changes what the next frame draws.
        repeat(SPIKE_FRAMES) { frame ->
            for (event in drainWindowEvents()) {
                if (report && event.kind != WindowEvent.POINTER_MOVE) {
                    System.err.println("dioxus-compose: window heard $event")
                }
                if (event.kind == WindowEvent.RESIZE) {
                    size = androidx.compose.ui.unit.IntSize(event.x.toInt(), event.y.toInt())
                    scene.size = size
                }
                scene.receive(event)
                textInput.receive(event)
            }
            if (!drawFrame(window, context, scene, frame.toLong() * FRAME_NANOS)) {
                // Nothing was drawn, so nothing waited for the screen either. Without this
                // a minimised window would spend every frame it has in a few milliseconds.
                Thread.sleep(FRAME_MILLIS)
            }
        }
    } finally {
        scene.close()
        context.close()
    }
}

/**
 * One frame: take a buffer, let the scene paint it, give it to the screen.
 *
 * False where there was no buffer to take. A closed window answers that, a minimised one
 * answers it for as long as it stays down, and a swapchain that has just been asked to
 * fit a new size answers it once.
 */
private fun drawFrame(
    window: Win32NativeWindow,
    context: org.jetbrains.skia.DirectContext,
    scene: ComposeScene,
    nanos: Long,
): Boolean {
    val resource = window.beginFrame()
    if (resource == 0L) return false
    // After the buffer and not before it, because that is where a swapchain waiting to be
    // refitted is refitted, and what is measured here is the buffer that came back.
    val measured = window.measure()
    val fitted = androidx.compose.ui.unit.IntSize(measured.width, measured.height)
    val density = androidx.compose.ui.unit.Density(measured.scale)
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
        return true
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
    return true
}

/**
 * Puts what the input method produced into the field that asked to be typed into.
 *
 * The same function that lives in the macOS file, duplicated here because that one is
 * private to its file. The Win32 C side does not produce text events yet (IMM32 is
 * not wired), but the handler is ready for when it does.
 */
private fun NativeTextInput.receive(event: WindowEvent) {
    if (!isActive) return
    when (event.kind) {
        WindowEvent.TEXT_COMMIT -> commit(event.text)
        WindowEvent.TEXT_COMPOSE -> compose(event.text)
    }
}

private const val SPIKE_FRAMES = 1_200
private const val FRAME_MILLIS = 16L
private const val FRAME_NANOS = 16_000_000L
