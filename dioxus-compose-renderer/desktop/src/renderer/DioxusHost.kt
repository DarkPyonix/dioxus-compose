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
import dioxus.compose.protocol.ColorRole
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
    contentPadding: PaddingValues = PaddingValues(0.dp),
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
        Box(modifier.background(theme.color(ColorRole.Background))) {
            Box(Modifier.padding(contentPadding)) {
                host.roots.forEach { rootId ->
                    androidx.compose.runtime.key(rootId) { RenderNode(rootId, host.table, host) }
                }
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
