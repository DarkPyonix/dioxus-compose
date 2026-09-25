@file:OptIn(
    androidx.compose.ui.InternalComposeUiApi::class,
    kotlinx.cinterop.ExperimentalForeignApi::class,
)

package dioxus.compose.ui.platform

import androidx.compose.runtime.Composable
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asComposeCanvas
import androidx.compose.ui.input.pointer.PointerButton
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.platform.DefaultArchitectureComponentsOwner
import androidx.compose.ui.platform.PlatformContext
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.platform.WindowInfo
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntSize
import kotlinx.cinterop.CValue
import kotlinx.cinterop.useContents
import platform.CoreGraphics.CGPoint
import platform.Foundation.NSPointInRect
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.enableSavedStateHandles
import kotlinx.coroutines.Dispatchers
import org.jetbrains.skia.Canvas
import org.jetbrains.skiko.SkiaLayer
import org.jetbrains.skiko.SkikoRenderDelegate
import platform.AppKit.NSBackingStoreBuffered
import platform.AppKit.NSEvent
import platform.AppKit.NSEventModifierFlagCommand
import platform.AppKit.NSEventModifierFlagControl
import platform.AppKit.NSEventModifierFlagOption
import platform.AppKit.NSEventModifierFlagShift
import platform.AppKit.NSTrackingActiveAlways
import platform.AppKit.NSTrackingActiveInKeyWindow
import platform.AppKit.NSTrackingAssumeInside
import platform.AppKit.NSTrackingArea
import platform.AppKit.NSTrackingInVisibleRect
import platform.AppKit.NSTrackingMouseEnteredAndExited
import platform.AppKit.NSTrackingMouseMoved
import platform.AppKit.NSView
import platform.AppKit.NSWindow
import platform.AppKit.NSWindowStyleMaskClosable
import platform.AppKit.NSWindowStyleMaskMiniaturizable
import platform.AppKit.NSWindowStyleMaskResizable
import platform.AppKit.NSWindowStyleMaskFullSizeContentView
import platform.AppKit.NSWindowStyleMaskTitled
import platform.AppKit.NSWindowTitleHidden
import platform.AppKit.NSAccessibilityButtonRole
import platform.AppKit.NSAccessibilityCheckBoxRole
import platform.AppKit.NSAccessibilityElement
import platform.AppKit.NSAccessibilityGroupRole
import platform.AppKit.NSAccessibilityImageRole
import platform.AppKit.NSAccessibilityStaticTextRole
import platform.AppKit.NSAccessibilityTextFieldRole
import platform.CoreGraphics.CGRectMake
import platform.Foundation.NSMakeRect

/**
 * A window of this renderer's own, rather than the one Compose opens for this platform.
 *
 * Compose's is smaller than this by a little and misses one thing entirely: nothing it
 * opens tells a screen reader what is in it. Its window publishes its three title bar
 * buttons and its title, and every control the application drew is invisible. That is not
 * an oversight in one place that could be worked around from outside, because the tree a
 * reader wants arrives through the scene's platform context, and a window that builds its
 * own leaves no way in.
 *
 * So the scene is built here, with a context that listens. Everything else is what
 * Compose's own window does and is kept close to it deliberately.
 */
internal class MacosWindow(private val name: String, width: Int, height: Int) {
    private var measured = IntSize(width, height)
    private val components = DefaultArchitectureComponentsOwner()
    private val windowInfo = object : WindowInfo {
        override val isWindowFocused: Boolean get() = true
        override val containerSize: IntSize get() = measured
    }
    private val skiaLayer = SkiaLayer()

    // What the window says about itself, kept in step with the scene by the listener the
    // context carries. Pushed on change rather than asked for.
    private val semantics = NativeSemantics { elements -> describeToReader(elements) }

    private val platformContext: PlatformContext =
        object : PlatformContext by PlatformContext.Empty() {
            override val windowInfo get() = this@MacosWindow.windowInfo
            override val architectureComponentsOwner get() = components
            override val semanticsOwnerListener get() = semantics
        }

    private val scene = CanvasLayersComposeScene(
        coroutineContext = Dispatchers.Main,
        platformContext = platformContext,
        invalidate = skiaLayer::needRender,
    )

    private val renderDelegate = object : SkikoRenderDelegate {
        override fun onRender(canvas: Canvas, width: Int, height: Int, nanoTime: Long) {
            val size = IntSize(width, height)
            measured = size
            scene.size = size
            scene.render(canvas.asComposeCanvas(), nanoTime)
            // After the drawing, because that is when what is in the window has been
            // placed and can say where it is. Asked before, every control answers with an
            // empty rectangle and a reader finds the screen stacked in one corner.
            semantics.pushIfChanged(afterDrawing = true)
        }
    }

    val window = object : NSWindow(
        contentRect = NSMakeRect(0.0, 0.0, width.toDouble(), height.toDouble()),
        styleMask = NSWindowStyleMaskTitled or NSWindowStyleMaskMiniaturizable or
            NSWindowStyleMaskClosable or NSWindowStyleMaskResizable or
            // The screen reaches the top of the window rather than starting under a bar
            // of the system's. The bar is still there and still the system's, which is
            // what keeps the three buttons and the drag and the double click to zoom;
            // it is see-through, and what shows through is the application.
            NSWindowStyleMaskFullSizeContentView,
        backing = NSBackingStoreBuffered,
        defer = true,
    ) {
        override fun canBecomeKeyWindow() = true
        override fun canBecomeMainWindow() = true
    }

    private val view = object : NSView(window.frame) {
        private var tracking: NSTrackingArea? = null

        override fun wantsUpdateLayer() = true
        override fun acceptsFirstResponder() = true

        // The click is ours wherever it lands inside us. Skia's layer puts a view of
        // its own on top of this one, and the press goes to whatever is on top: the
        // keyboard arrived because that follows the responder, and no press ever did
        // because that follows what is under the pointer.
        override fun hitTest(aPoint: CValue<CGPoint>): NSView? =
            if (NSPointInRect(convertPoint(aPoint, fromView = superview), bounds)) this
            else null

        override fun acceptsFirstMouse(event: NSEvent?) = true
        override fun viewWillMoveToWindow(newWindow: NSWindow?) = updateTrackingAreas()

        override fun updateTrackingAreas() {
            tracking?.let { removeTrackingArea(it) }
            val area = NSTrackingArea(
                rect = bounds,
                options = NSTrackingActiveAlways or NSTrackingMouseEnteredAndExited or
                    NSTrackingMouseMoved or NSTrackingActiveInKeyWindow or
                    NSTrackingAssumeInside or NSTrackingInVisibleRect,
                owner = this,
                userInfo = null,
            )
            tracking = area
            addTrackingArea(area)
        }

        override fun mouseDown(event: NSEvent) =
            send(event, PointerEventType.Press, PointerButton.Primary)

        override fun mouseUp(event: NSEvent) =
            send(event, PointerEventType.Release, PointerButton.Primary)

        override fun rightMouseDown(event: NSEvent) =
            send(event, PointerEventType.Press, PointerButton.Secondary)

        override fun rightMouseUp(event: NSEvent) =
            send(event, PointerEventType.Release, PointerButton.Secondary)

        override fun mouseMoved(event: NSEvent) = send(event, PointerEventType.Move)

        override fun mouseDragged(event: NSEvent) = send(event, PointerEventType.Move)

        override fun scrollWheel(event: NSEvent) = send(event, PointerEventType.Scroll)

        override fun keyDown(event: NSEvent) {
            // Only what the screen did not want reaches the system, which is what makes
            // it sound the alert for a key nothing took.
            if (!scene.sendKeyEvent(event.compose(KeyEventType.KeyDown))) super.keyDown(event)
        }

        override fun keyUp(event: NSEvent) {
            if (!scene.sendKeyEvent(event.compose(KeyEventType.KeyUp))) super.keyUp(event)
        }
    }

    fun setContent(content: @Composable () -> Unit) {
        // Carried but not drawn. A window with no title is listed as nothing in the
        // switcher and in Mission Control, so it is set; it is hidden because the
        // application draws its own heading where the bar would have written it.
        window.setTitle(name)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = NSWindowTitleHidden
        window.contentView = view

        skiaLayer.renderDelegate = renderDelegate
        // After the view is in the window, which is what skiko asks for.
        skiaLayer.attachTo(view)

        window.center()
        window.makeKeyAndOrderFront(null)
        window.makeFirstResponder(view)

        // After the window is on screen, and in this order: the density is the screen's
        // and is not known until the window is on one, and a scene given content before
        // it has a size composes into nothing and draws a blank window.
        scene.density = Density(window.backingScaleFactor.toFloat())
        scene.setContent(content)

        // Said, rather than assumed. Compose composes for something that is alive, and a
        // scene nobody has resumed stays where it started, which is a window that opens
        // and never draws.
        components.enableSavedStateHandles()
        components.lifecycle.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)
    }

    /**
     * Hands the reader what the scene last said, as elements it can ask about.
     *
     * Replaced whole rather than edited. The tree arrives whole, a reader asks for it on
     * its own thread at moments nobody chose, and one list swapped for another is a thing
     * that has either happened or not.
     *
     * The frames are the scene's, which count down from the top left of the window; the
     * reader's count up from the bottom left of the screen, so each one is turned twice.
     */
    private fun describeToReader(elements: List<AccessibleElement>) {
        val scale = window.backingScaleFactor
        val built = elements.map { element ->
            val made = NSAccessibilityElement.accessibilityElementWithRole(
                role = element.role.readerRole,
                frame = CGRectMake(0.0, 0.0, 0.0, 0.0),
                label = element.label,
                parent = view,
            )
            val inView = CGRectMake(
                x = element.x.toDouble() / scale,
                y = element.y.toDouble() / scale,
                width = element.width.toDouble() / scale,
                height = element.height.toDouble() / scale,
            )
            val flipped = view.frame.useContents {
                CGRectMake(
                    x = inView.useContents { origin.x },
                    y = size.height - inView.useContents { origin.y + size.height },
                    width = inView.useContents { size.width },
                    height = inView.useContents { size.height },
                )
            }
            (made as NSAccessibilityElement).setAccessibilityFrame(
                view.window?.convertRectToScreen(view.convertRect(flipped, toView = null))
                    ?: flipped,
            )
            made
        }
        view.setAccessibilityChildren(built)
    }

    private fun send(event: NSEvent, kind: PointerEventType, button: PointerButton? = null) {
        scene.sendPointerEvent(
            eventType = kind,
            position = event.offsetInView,
            scrollDelta = Offset(event.deltaX.toFloat(), event.deltaY.toFloat()),
            nativeEvent = event,
            button = button,
        )
    }

    // The window's coordinates count up from the bottom and the scene's count down from
    // the top, so one is the other subtracted from the height. In pixels on both sides:
    // the layer is asked to draw at the screen's density and the scene is told that size,
    // so nothing here divides by it.
    private val NSEvent.offsetInView: Offset
        get() {
            val where = locationInWindow.useContents { Offset(x.toFloat(), y.toFloat()) }
            val height = view.frame.useContents { size.height.toFloat() }
            val scale = view.window?.backingScaleFactor?.toFloat() ?: 1f
            return Offset(where.x * scale, (height - where.y) * scale)
        }

    // Built from parts rather than converted: what converts a platform key event is
    // internal to Compose, and the parts are the same ones the native image path builds
    // from because it has no platform event to convert either.
    private fun NSEvent.compose(type: KeyEventType): KeyEvent =
        KeyEvent(
            key = composeKey(keyCode.toInt()),
            type = type,
            codePoint = characters?.firstOrNull()?.code ?: 0,
            isAltPressed = modifierFlags and NSEventModifierFlagOption != 0uL,
            isCtrlPressed = modifierFlags and NSEventModifierFlagControl != 0uL,
            isMetaPressed = modifierFlags and NSEventModifierFlagCommand != 0uL,
            isShiftPressed = modifierFlags and NSEventModifierFlagShift != 0uL,
        )
}

/** What a reader calls the kind of control this is. */
private val Int.readerRole: String
    get() = when (this) {
        ElementRole.BUTTON -> NSAccessibilityButtonRole
        ElementRole.TEXT -> NSAccessibilityStaticTextRole
        ElementRole.FIELD -> NSAccessibilityTextFieldRole
        ElementRole.CHECKBOX -> NSAccessibilityCheckBoxRole
        ElementRole.IMAGE -> NSAccessibilityImageRole
        else -> NSAccessibilityGroupRole
    } ?: NSAccessibilityGroupRole ?: "AXGroup"
