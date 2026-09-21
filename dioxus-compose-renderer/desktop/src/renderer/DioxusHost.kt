package dioxus.compose.runtime

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.withFrameNanos
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dioxus.compose.foundation.HostMessages
import dioxus.compose.protocol.ColorRole
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.protocol.WindowSizeClass
import dioxus.compose.ui.platform.LocalFrameRequests
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.Mutation
import dioxus.compose.design.LocalDesignTheme
import dioxus.compose.design.detectHostPlatform
import dioxus.compose.design.resolveTheme
import dioxus.compose.ui.node.NodeTable
import dioxus.compose.ui.node.RenderNode
import dioxus.compose.ui.node.TableError
import java.lang.InterruptedException
import java.lang.System
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.runtime.getValue
import androidx.compose.foundation.isSystemInDarkTheme

/**
 * Sends one event to the Host and reports whether the Host consumed it.
 *
 * Consumption is the same model as the web's `preventDefault()` and Compose's
 * `PointerInputChange.consume()`: the handler marks the event, and the boundary returns that
 * mark in `MutationBatch.result`. The Renderer treats a non-zero result as "consumed", so a
 * Host that always returns 0 simply never consumes. It is what lets Enter in a multiline
 * field submit without also inserting a newline.
 */
fun interface EventDispatcher {
    fun dispatch(event: HostEvent): Boolean
}

/**
 * Owns the interpreted tree and the boundary calls for one Host.
 *
 * The calls are synchronous and on this thread: there is no queue between the two sides, and
 * a batch is consumed inside the call that produced it.
 *
 * Every batch is applied inside a single `Snapshot.withMutableSnapshot` transaction, so the
 * intermediate states of a batch are never drawn.
 */
class DioxusHost(private val connection: HostConnection) : EventDispatcher {
    val table: NodeTable = NodeTable()

    val roots: List<Int> get() = table.roots

    fun start() {
        applyTransaction { apply -> connection.init(apply) }
    }

    /** Dispatches synchronously and returns the Host's consumption result. */
    override fun dispatch(event: HostEvent): Boolean {
        var result = 0L
        applyTransaction { apply -> result = connection.dispatchEvent(event, apply) }
        return result != 0L
    }

    /** Called once per frame after a Host worker asked for one. */
    fun renderFrame(frameTimeNanos: Long) {
        applyTransaction { apply -> connection.renderFrame(frameTimeNanos, apply) }
    }

    fun shutdown() = connection.shutdown()

    private fun applyTransaction(call: ((Mutation) -> Unit) -> Unit) {
        val protocolErrors = mutableListOf<TableError>()
        Snapshot.withMutableSnapshot {
            try {
                call { mutation -> table.apply(mutation) }
            } catch (error: Throwable) {
                // A malformed batch must not take the process down: it becomes a reported
                // protocol error instead.
                if (error is InterruptedException) throw error
                protocolErrors += TableError(
                    PROTOCOL_DECODE_ERROR,
                    error.message ?: error::class.qualifiedName ?: "unknown error",
                )
            }
            protocolErrors += table.drainErrors()
        }
        // Reported after the transaction so the Host is never re-entered mid-batch.
        //
        // A Host is free to reject the report itself: nothing promises that reporting an
        // error succeeds, and the Rust Host answers a `ProtocolError` event with a
        // protocol-error status because no handler owns it. Letting that failure out of here
        // would turn a reported error into a crashed composition, which is the outcome the
        // report exists to prevent, so the report is best effort and the original error is
        // what gets printed.
        protocolErrors.forEach { error ->
            try {
                connection.dispatchEvent(
                    HostEvent.ProtocolError(
                        nodeId = NodeTable.ROOT_ID,
                        handlerId = 0,
                        code = error.code,
                        message = error.message,
                    ),
                ) { mutation -> Snapshot.withMutableSnapshot { table.apply(mutation) } }
            } catch (report: Throwable) {
                if (report is InterruptedException) throw report
                onProtocolError(error)
            }
        }
    }

    private companion object {
        const val PROTOCOL_DECODE_ERROR = 100
    }
}

/**
 * Where a protocol error goes when the Host will not take the report.
 *
 * Tests replace it to assert on what was reported; production leaves it printing.
 */
internal var onProtocolError: (TableError) -> Unit = { error ->
    System.err.println("dioxus-compose protocol error ${error.code}: ${error.message}")
}

/**
 * The strip of the window the system's own buttons occupy.
 *
 * Content is allowed to run underneath it, which is the point of modern chrome, but a
 * widget placed where the macOS traffic lights are would leave both unusable. The Host
 * never sees this: the safe area is a fact about the window rather than a decision the
 * application makes.
 *
 * `buttonsWidth` is the horizontal room the buttons take at the leading edge, which is
 * zero on the platforms where the caption is ours to draw.
 */
data class WindowCaption(val height: Dp = 0.dp, val buttonsWidth: Dp = 0.dp) {
    companion object {
        /** A window with no system buttons over its content. */
        val None = WindowCaption()
    }
}

/** The caption of the window this content is in. */
internal val LocalWindowCaption = staticCompositionLocalOf { WindowCaption.None }

/**
 * The node that is acting as the caption, or null where nothing is.
 *
 * Only one bar can be the caption, and which one is decided where the whole tree can be
 * seen. A bar further down the tree reads null here and lays itself out normally.
 */
internal val LocalCaptionBar = staticCompositionLocalOf<Int?> { null }

/**
 * The bar a tree leads with, if it leads with one.
 *
 * A `TopAppBar` at the top of the window is the caption: it lays itself out around the
 * system buttons and the content is not pushed below them. Applications do not put a bar
 * at the root, they put it first inside the column that is the screen, so this walks the
 * first child of each layout it meets rather than looking only at the root.
 *
 * A `Navigation` stops the walk. With a rail or a drawer the bar does not reach the
 * window's leading edge, so it cannot be what the buttons sit in, and the content keeps
 * the inset instead.
 */
private fun captionBarOf(table: NodeTable, id: Int): Int? {
    val node = table.node(id) ?: return null
    return when (node.widget) {
        WidgetKind.TopAppBar -> node.id
        WidgetKind.Column, WidgetKind.Box -> node.children
            .firstOrNull()
            ?.let { captionBarOf(table, it) }

        else -> null
    }
}

/** Creates a Host bound to the composition's lifetime. */
@Composable
fun rememberDioxusHost(connection: HostConnection): DioxusHost {
    val host = remember(connection) { DioxusHost(connection) }
    DisposableEffect(host) {
        host.start()
        onDispose { host.shutdown() }
    }
    return host
}

/**
 * Draws the Host's tree and runs the frame loop.
 *
 * Frame requests coming from Host worker threads coalesce into at most one `render_frame`
 * per frame, inside `withFrameNanos`.
 */
@Composable
fun DioxusContent(
    host: DioxusHost,
    modifier: Modifier = Modifier,
    caption: WindowCaption = WindowCaption.None,
) {
    val frames = LocalFrameRequests.current
    LaunchedEffect(host, frames) {
        var applied = frames.counter.value
        frames.counter.collect { requested ->
            if (requested == applied) return@collect
            applied = requested
            withFrameNanos { frameTimeNanos -> host.renderFrame(frameTimeNanos) }
        }
    }
    // The theme is resolved once here, and every node reads it from the CompositionLocal. A `SetTheme` is therefore one record on the wire and one
    // invalidation in Compose, not a SetProp per node.
    val platform = remember { detectHostPlatform() }
    // Compose's isSystemInDarkTheme() reads the setting once on the desktop and never
    // notices it change, so a window there keeps its original colours while the rest of
    // the screen switches. The desktop installs an observer that does follow the system;
    // where nobody installs one, Compose's own answer is correct and is used.
    val observedDark = LocalSystemDarkObserver.current?.invoke() ?: isSystemInDarkTheme()
    val systemDark = systemDarkOverride ?: observedDark
    val theme = resolveTheme(host.table.theme, platform, systemDark)
    CompositionLocalProvider(LocalDesignTheme provides theme) {
        // The background fills the whole window and the inset is applied inside it. Putting
        // the inset outside instead leaves the window's own background showing through the
        // strip the title bar used to occupy, which reads as a leftover title bar rather
        // than as content extending underneath one.
        // The window's size is measured here, where the root content is, and reported to
        // the Host only when it crosses a size class boundary. onSizeChanged already fires
        // only when the measured size differs, and the reporter drops everything that does
        // not change the class, so a drag across one class costs no boundary calls.
        val reporter = remember(host) { WindowSizeReporter() }
        val density = LocalDensity.current
        // The same measurement answers two questions. The Host is told when the class
        // changes so a component can choose what to put on the screen; the widgets that
        // change shape with the window read it from the CompositionLocal, because they are
        // drawn on this side and a round trip to ask would cost a boundary call, a
        // VirtualDom pass and a rebuilt subtree for a layout this side can already reach.
        var sizeClass by remember(host) { mutableStateOf(WindowSizeClass.Compact) }
        val measured = Modifier.onSizeChanged { size ->
            with(density) {
                val widthDp = size.width.toDp().value
                sizeClass = windowSizeClassOf(widthDp)
                reporter.report(widthDp, size.height.toDp().value, host)
            }
        }
        // A tree that leads with a bar puts that bar in the caption and lays it out around
        // the window buttons; a tree that does not is pushed clear of them. The decision is
        // made here because it is the only place the whole tree is in view.
        val captionBar = host.roots.firstNotNullOfOrNull { captionBarOf(host.table, it) }
        val contentPadding = if (captionBar == null) {
            PaddingValues(top = caption.height)
        } else {
            PaddingValues(0.dp)
        }
        CompositionLocalProvider(
            LocalWindowSizeClass provides sizeClass,
            LocalWindowCaption provides caption,
            LocalCaptionBar provides captionBar,
        ) {
            Box(modifier.then(measured).background(theme.color(ColorRole.Background))) {
                Box(Modifier.padding(contentPadding)) {
                    host.roots.forEach { rootId ->
                        androidx.compose.runtime.key(rootId) {
                            RenderNode(rootId, host.table, host)
                        }
                    }
                }
                // Over the content rather than in it: a message is not part of the tree,
                // and it covers whatever it has to for as long as it is up.
                HostMessages(host.table.messages, host.table.insets, host, theme)
            }
        }
    }
}

/**
 * Overrides the platform's dark mode reading, for tests and for the dev harness.
 *
 * `ColorScheme.FollowSystem` reads the platform; nothing else consults this.
 */
var systemDarkOverride: Boolean? = null

/**
 * How to find out whether the system is in dark mode, when the platform knows better than
 * Compose does.
 *
 * Installed by the platform at startup. The desktop installs a poller, because Compose's
 * own reading there is taken once and never revisited. iOS and Android leave it null,
 * where Compose observes the change itself and a second mechanism would be noise.
 *
 * It lives here rather than in the desktop module because this file is compiled for every
 * target: the iOS renderer symlinks it, and Skiko, which the desktop observer reads, does
 * not exist on Kotlin/Native.
 *
 * It is a CompositionLocal rather than a global, and that is not a matter of taste. The
 * desktop observer polls in a loop that never finishes, which is correct in a window and
 * fatal under a test clock: a composition with a coroutine forever waiting on a delay never
 * goes idle, so waitForIdle spins until the test times out. As a global, one composition
 * installing it silently did that to every composition created afterwards in the same
 * process, including tests that had nothing to do with it.
 */
val LocalSystemDarkObserver = staticCompositionLocalOf<(@Composable () -> Boolean)?> { null }
