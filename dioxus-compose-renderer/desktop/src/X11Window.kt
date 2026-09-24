@file:JvmName("X11Window")
@file:OptIn(androidx.compose.ui.InternalComposeUiApi::class)

package dioxus.compose.ui.platform

import androidx.compose.ui.graphics.asComposeCanvas
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.scene.ComposeScene
import org.graalvm.nativeimage.StackValue
import org.graalvm.nativeimage.c.function.CFunction
import org.graalvm.nativeimage.c.type.CCharPointer
import org.graalvm.nativeimage.c.type.CFloatPointer
import org.graalvm.nativeimage.c.type.CIntPointer
import org.graalvm.nativeimage.c.type.CTypeConversion
import org.graalvm.word.Pointer
import org.graalvm.word.WordFactory

// X11 and GLX own the window and framebuffer. The event record, scene input conversion
// and spike content are shared with the other desktop windows.
@CFunction("dxc_native_window_open")
private external fun openWindow(title: CCharPointer?, width: Int, height: Int, out: Pointer?): Int

@CFunction("dxc_native_window_size")
private external fun windowSize(window: Pointer?, width: CIntPointer?, height: CIntPointer?, scale: CFloatPointer?)

@CFunction("dxc_native_frame_begin")
private external fun beginFrame(window: Pointer?): Int

@CFunction("dxc_native_frame_end")
private external fun endFrame(display: Pointer?)

class X11NativeWindow internal constructor(
    val window: Long,
    val view: Long,
    val device: Long,
    val queue: Long,
    val layer: Long,
) {
    fun measure(): WindowMeasurement {
        val width = StackValue.get<CIntPointer>(4)
        val height = StackValue.get<CIntPointer>(4)
        val scale = StackValue.get<CFloatPointer>(4)
        windowSize(WordFactory.pointer(window), width, height, scale)
        return WindowMeasurement(width.read(), height.read(), scale.read())
    }

    fun beginFrame(): Boolean = beginFrame(WordFactory.pointer(window)) == 0
    fun endFrame() = endFrame(WordFactory.pointer(queue))
}

fun openX11Window(title: String, width: Int, height: Int): X11NativeWindow? {
    val holder = CTypeConversion.toCString(title)
    try {
        val out = StackValue.get<Pointer>(WINDOW_STRUCT_BYTES)
        if (openWindow(holder.get(), width, height, out) != 0) return null
        return X11NativeWindow(
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
private const val GL_RGBA8 = 0x8058

internal fun runX11Spike() {
    val host = dioxus.compose.runtime.DioxusHost(NativeHostConnection())
    host.start()
    val asked = host.table.window
    val window = openX11Window(
        asked?.title?.takeIf { it.isNotEmpty() } ?: "dioxus-compose",
        if (asked != null && asked.width > 0) asked.width else 520,
        if (asked != null && asked.height > 0) asked.height else 360,
    )
    if (window == null) {
        System.err.println("dioxus-compose: X11 or a GLX visual is unavailable")
        host.shutdown()
        return
    }
    val context = org.jetbrains.skia.DirectContext.makeGL()
    val measured = window.measure()
    val report = System.getenv("DXC_REPORT_INPUT") != null
    val scene = CanvasLayersComposeScene(
        density = androidx.compose.ui.unit.Density(measured.scale),
        size = androidx.compose.ui.unit.IntSize(measured.width, measured.height),
    )
    scene.setContent { dioxus.compose.runtime.DioxusContent(host) }
    try {
        repeat(SPIKE_FRAMES) { frame ->
            for (event in drainWindowEvents()) {
                if (report && event.kind != WindowEvent.POINTER_MOVE) {
                    System.err.println("dioxus-compose: window heard $event")
                }
                scene.receive(event)
            }
            drawFrame(window, context, scene, frame.toLong() * FRAME_NANOS)
            Thread.sleep(FRAME_MILLIS)
        }
    } finally {
        scene.close()
        context.close()
        host.shutdown()
    }
}

private fun drawFrame(
    window: X11NativeWindow,
    context: org.jetbrains.skia.DirectContext,
    scene: ComposeScene,
    nanos: Long,
): Boolean {
    if (!window.beginFrame()) return false
    val measured = window.measure()
    val fitted = androidx.compose.ui.unit.IntSize(measured.width, measured.height)
    val density = androidx.compose.ui.unit.Density(measured.scale)
    if (scene.size != fitted || scene.density != density) {
        scene.density = density
        scene.size = fitted
    }
    val target = org.jetbrains.skia.BackendRenderTarget.makeGL(
        measured.width, measured.height, 0, 0, 0, GL_RGBA8,
    )
    val surface = org.jetbrains.skia.Surface.makeFromBackendRenderTarget(
        context,
        target,
        org.jetbrains.skia.SurfaceOrigin.BOTTOM_LEFT,
        org.jetbrains.skia.SurfaceColorFormat.RGBA_8888,
        org.jetbrains.skia.ColorSpace.sRGB,
        org.jetbrains.skia.SurfaceProps(org.jetbrains.skia.PixelGeometry.RGB_H),
    )
    if (surface == null) {
        target.close()
        return false
    }
    scene.render(surface.canvas.asComposeCanvas(), nanos)
    surface.flushAndSubmit(true)
    surface.close()
    target.close()
    window.endFrame()
    return true
}

private const val SPIKE_FRAMES = 1_200
private const val FRAME_MILLIS = 16L
private const val FRAME_NANOS = 16_000_000L
