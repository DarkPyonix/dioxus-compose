@file:JvmName("AppKitWindow")
@file:OptIn(androidx.compose.ui.InternalComposeUiApi::class)

package dioxus.compose.ui.platform

import org.graalvm.nativeimage.StackValue
import org.graalvm.nativeimage.c.function.CFunction
import org.graalvm.nativeimage.c.type.CCharPointer
import org.graalvm.nativeimage.c.type.CIntPointer
import org.graalvm.nativeimage.c.type.CFloatPointer
import org.graalvm.nativeimage.c.type.CTypeConversion
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asComposeCanvas
import androidx.compose.ui.input.pointer.PointerButton
import androidx.compose.ui.input.pointer.PointerButtons
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.scene.ComposeScene
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.graalvm.word.Pointer
import org.graalvm.word.WordFactory

// A window that is ours, drawn into with Skia and with no toolkit in between.
//
// Beside `NativeHostConnection` rather than among the renderer's own files, and for the
// same reason: this is the only other place that names GraalVM types, so a development
// run on a JVM never loads them.
//
// The C side is `c/appkit_window.m`. It owns the window, the view, the layer, the Metal
// device and the queue, and answers with the pointers. Nothing there draws.
//
// The scene this drives is Compose's own, reached through an interface the library marks
// as being for its own modules. There is no other way in: the supported entry builds a
// toolkit window, and a toolkit window is the thing being removed. Kept to this one file
// and pinned to the version in the module file, which is what the rule about unstable
// APIs asks for. A Compose upgrade changes this file or it changes nothing.

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

@CFunction("dxc_native_poll_event")
private external fun pollEvent(out: Pointer?): Int

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
 * What happened in the window, as the shell recorded it.
 *
 * Plain numbers rather than a platform event. What Compose is given is built on this
 * side, so nothing of AppKit's reaches the scene and the same record could be filled in
 * by another platform without the scene noticing.
 */
data class WindowEvent(
    val kind: Int,
    val x: Float,
    val y: Float,
    val buttons: Int,
    val modifiers: Int,
    val keyCode: Int,
    val codePoint: Int,
    /** What an input method produced, and empty for everything that is not text. */
    val text: String,
) {

    companion object {
        const val POINTER_MOVE = 1
        const val POINTER_DOWN = 2
        const val POINTER_UP = 3
        const val SCROLL = 4
        const val KEY_DOWN = 5
        const val KEY_UP = 6
        const val TEXT_COMMIT = 7
        const val TEXT_COMPOSE = 8
    }
}

/**
 * Takes everything the window has heard since the last frame.
 *
 * Drained rather than delivered. AppKit answers on its own thread and the Host keeps its
 * state on the one that draws, so an event that arrived as a call would arrive on the
 * wrong thread; the shell writes them down and this reads them where they can be used.
 */
fun drainWindowEvents(): List<WindowEvent> {
    val record = StackValue.get<Pointer>(EVENT_STRUCT_BYTES)
    val events = ArrayList<WindowEvent>()
    // Everything about the record is read here. A word value may not leave the method it
    // was made in, so the text is copied out byte by byte rather than by handing the
    // pointer to something that knows how to read a string.
    val bytes = ByteArray(TEXT_BYTES)
    while (pollEvent(record) != 0) {
        var length = 0
        while (length < TEXT_BYTES) {
            val byte = record.readByte(TEXT_OFFSET + length)
            if (byte == ZERO) break
            bytes[length] = byte
            length++
        }
        events.add(
            WindowEvent(
                kind = record.readInt(0),
                x = record.readFloat(4),
                y = record.readFloat(8),
                buttons = record.readInt(12),
                modifiers = record.readInt(16),
                keyCode = record.readInt(20),
                codePoint = record.readInt(24),
                text = if (length == 0) "" else String(bytes, 0, length, Charsets.UTF_8),
            ),
        )
    }
    return events
}

private const val ZERO: Byte = 0
private const val TEXT_OFFSET = 28
private const val TEXT_BYTES = 96
private const val EVENT_STRUCT_BYTES = 124

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
 * Draws a Compose scene into a window of our own, and holds it there.
 *
 * The step worth taking first, and the one everything after it rests on: a scene that
 * Compose composed, painted by Skia into a drawable AppKit gave us, reaching the screen
 * with no toolkit anywhere between. What follows is input, text and the rest, and none of
 * it means anything until this does.
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

    val report = System.getenv("DXC_REPORT_INPUT") != null
    val size = androidx.compose.ui.unit.IntSize(measured.width, measured.height)
    val textInput = NativeTextInput()
    val scene = CanvasLayersComposeScene(
        density = androidx.compose.ui.unit.Density(measured.scale),
        size = size,
        platformContext = NativePlatformContext({ size }, textInput),
    )
    scene.setContent { SpikeContent() }

    try {
        // A plain loop rather than a clock. Pacing is the frame clock's work and comes
        // later; what this has to show is that what the window hears reaches the scene
        // and changes what the next frame draws.
        var painted = false
        repeat(SPIKE_FRAMES) { frame ->
            var heard = false
            for (event in drainWindowEvents()) {
                if (report && event.kind != WindowEvent.POINTER_MOVE) {
                    System.err.println("dioxus-compose: window heard $event")
                }
                scene.receive(event)
                textInput.receive(event)
                heard = true
            }
            // Only when there is something to draw. Every frame reaches the window by
            // asking the main thread for a drawable and waiting for it, and the main
            // thread is where AppKit answers everything else: sixty of those a second
            // left the input method unable to reach this process at all, which showed up
            // as every letter being committed on its own instead of composing.
            if (!painted || heard || scene.hasInvalidations()) {
                drawFrame(window, context, scene, frame.toLong() * FRAME_NANOS, size)
                painted = true
            }
            Thread.sleep(FRAME_MILLIS)
        }
    } finally {
        scene.close()
        context.close()
    }
}

/** One frame: take a drawable, let the scene paint it, give it to the screen. */
private fun drawFrame(
    window: NativeWindow,
    context: org.jetbrains.skia.DirectContext,
    scene: ComposeScene,
    nanos: Long,
    size: androidx.compose.ui.unit.IntSize,
) {
    val texture = window.beginFrame()
    // Zero means the system had no drawable to give, which happens when frames are made
    // faster than the screen takes them. The answer is to skip one.
    if (texture == 0L) return
    val target = org.jetbrains.skia.BackendRenderTarget.makeMetal(size.width, size.height, texture)
    val surface = org.jetbrains.skia.Surface.makeFromBackendRenderTarget(
        context,
        target,
        org.jetbrains.skia.SurfaceOrigin.TOP_LEFT,
        org.jetbrains.skia.SurfaceColorFormat.BGRA_8888,
        org.jetbrains.skia.ColorSpace.sRGB,
        org.jetbrains.skia.SurfaceProps(org.jetbrains.skia.PixelGeometry.RGB_H),
    )
    if (surface == null) {
        target.close()
        window.endFrame()
        return
    }
    scene.render(surface.canvas.asComposeCanvas(), nanos)
    // Submitted, not only recorded. Skia's Metal backend keeps the frame in a command
    // buffer of its own, and a drawable presented before that buffer runs is a drawable
    // with nothing in it: the window came up black with the paint never reaching the GPU.
    surface.flushAndSubmit(true)
    surface.close()
    target.close()
    window.endFrame()
}

/**
 * Something recognisably Compose, so that a screenshot answers a question.
 *
 * Shared with the window Windows opens for itself, which is why it is not private to this
 * file: what a spike draws is not platform work, and two copies of it would drift.
 *
 * Text and a shape: text because it is the part that needs a font manager, a shaper and a
 * layout pass, and a shape because a page of text alone would leave it unclear whether
 * anything was drawn or the window simply stayed empty.
 */
@Composable
internal fun SpikeContent() {
    var clicks by remember { mutableStateOf(0) }
    val hover = remember { MutableInteractionSource() }
    val hovered by hover.collectIsHoveredAsState()
    Box(Modifier.fillMaxSize().background(Color(0xFF12321A))) {
        Column(Modifier.padding(top = 40.dp, start = 24.dp)) {
            BasicText("no toolkit here", style = TextStyle(color = Color.White, fontSize = 24.sp))
            BasicText(
                "composed, painted by Skia, shown by AppKit",
                style = TextStyle(color = Color(0xFF9CCC9C), fontSize = 14.sp),
            )
            // A field, because typing is what the next step has to carry and this is
            // where it will first show. It holds its own text, the way every field in
            // this renderer does.
            var typed by remember { mutableStateOf("") }
            BasicTextField(
                value = typed,
                onValueChange = { typed = it },
                modifier = Modifier
                    .padding(top = 16.dp)
                    .size(220.dp, 32.dp)
                    .background(Color(0xFF1E4620)),
                textStyle = TextStyle(color = Color.White, fontSize = 16.sp),
                cursorBrush = SolidColor(Color.White),
            )
            Box(
                Modifier
                    .padding(top = 24.dp)
                    .size(220.dp, 56.dp)
                    // Hover and click are what this frame is for. A control that changes
                    // under the pointer is the difference between a window that was
                    // drawn and a window that is running.
                    .background(if (hovered) Color(0xFF66BB6A) else Color(0xFF2E7D32))
                    .hoverable(hover)
                    .clickable { clicks++ },
            ) {
                BasicText(
                    if (clicks == 0) "click me" else "clicked $clicks",
                    Modifier.padding(16.dp),
                    style = TextStyle(color = Color.White, fontSize = 18.sp),
                )
            }
        }
    }
}

/**
 * Hands one thing the window heard to the scene.
 *
 * Shared with the Windows path for the same reason the scene's content is. Both platforms
 * record an event into the same fields, so turning one into something Compose understands
 * is written once.
 *
 * The pointer's place arrives from the top left of the content, in whatever unit that
 * platform's scene measures in, so nothing is converted here beyond naming which kind of
 * event it was.
 */
internal fun ComposeScene.receive(event: WindowEvent) {
    when (event.kind) {
        // Built from parts rather than from a platform event. The toolkit's own key
        // event is what the supported path converts, and there is none here to convert.
        WindowEvent.KEY_DOWN, WindowEvent.KEY_UP -> sendKeyEvent(
            KeyEvent(
                key = composeKey(event.keyCode),
                type = if (event.kind == WindowEvent.KEY_DOWN) {
                    KeyEventType.KeyDown
                } else {
                    KeyEventType.KeyUp
                },
                codePoint = event.codePoint,
                isAltPressed = event.modifiers and MODIFIER_OPTION != 0,
                isCtrlPressed = event.modifiers and MODIFIER_CONTROL != 0,
                isMetaPressed = event.modifiers and MODIFIER_COMMAND != 0,
                isShiftPressed = event.modifiers and MODIFIER_SHIFT != 0,
            ),
        )

        WindowEvent.POINTER_MOVE -> sendPointerEvent(
            eventType = PointerEventType.Move,
            position = Offset(event.x, event.y),
            buttons = PointerButtons(isPrimaryPressed = event.buttons and 1 != 0),
        )

        WindowEvent.POINTER_DOWN -> sendPointerEvent(
            eventType = PointerEventType.Press,
            position = Offset(event.x, event.y),
            button = PointerButton.Primary,
            buttons = PointerButtons(isPrimaryPressed = true),
        )

        WindowEvent.POINTER_UP -> sendPointerEvent(
            eventType = PointerEventType.Release,
            position = Offset(event.x, event.y),
            button = PointerButton.Primary,
            buttons = PointerButtons(isPrimaryPressed = false),
        )

        // The wheel's travel arrives where a position usually is, because a scroll
        // happens wherever the pointer already was.
        WindowEvent.SCROLL -> sendPointerEvent(
            eventType = PointerEventType.Scroll,
            position = Offset.Zero,
            scrollDelta = Offset(event.x, event.y),
        )
    }
}

/**
 * The Compose key a platform key number means.
 *
 * A table because the two numberings have nothing to do with each other: the platform
 * numbers keys by where they sit on the board, and Compose names them by what they are.
 * Only the keys that have a meaning of their own are here. A key that types a character
 * carries that character in the event beside it, and a screen reading text wants the
 * character rather than the position.
 *
 * Unknown is a real answer. A key nobody mapped still reaches the scene with its
 * character, so typing works before every key in the world has a line here.
 */
private fun composeKey(platformKey: Int): Key = when (platformKey) {
    0x24 -> Key.Enter
    0x30 -> Key.Tab
    0x31 -> Key.Spacebar
    0x33 -> Key.Backspace
    0x35 -> Key.Escape
    0x75 -> Key.Delete
    0x7B -> Key.DirectionLeft
    0x7C -> Key.DirectionRight
    0x7D -> Key.DirectionDown
    0x7E -> Key.DirectionUp
    0x73 -> Key.MoveHome
    0x77 -> Key.MoveEnd
    0x74 -> Key.PageUp
    0x79 -> Key.PageDown
    else -> Key.Unknown
}

// From NSEvent.h. The bits a modifier flag word carries.
private const val MODIFIER_SHIFT = 1 shl 17
private const val MODIFIER_CONTROL = 1 shl 18
private const val MODIFIER_OPTION = 1 shl 19
private const val MODIFIER_COMMAND = 1 shl 20

/**
 * Puts what the input method produced into the field that asked to be typed into.
 *
 * Text does not arrive in Compose through key events. A focused field opens a session and
 * waits to be handed text, and what hands it over is the input method: `insertText` for a
 * letter that is finished and `setMarkedText` while a syllable is still being built.
 *
 * The keys themselves went to the scene already and are read there as keys: arrows, Enter
 * and backspace. Nothing is committed from a key's character, because a key that types
 * one has already produced it through the path above and doing both would type it twice.
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
