package dioxus.compose.runtime

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.withFrameNanos
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import dioxus.compose.protocol.ColorRole
import dioxus.compose.ui.platform.FrameRequests
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

/**
 * Sends one event to the Host and reports whether the Host consumed it (SPEC FR-3).
 *
 * SPEC-GAP: event consumption is tracked as FR-12 in the task backlog but has no SPEC item
 * yet. `MutationBatch.result` carries the handler's return value (PR-4), and the Renderer
 * treats a non-zero result as "consumed"; a Host that always returns 0 simply never
 * consumes. This needs an FR-12 entry in docs/SPEC.md with its acceptance criteria.
 */
fun interface EventDispatcher {
    fun dispatch(event: HostEvent): Boolean
}

/**
 * Owns the interpreted tree and the boundary calls for one Host (SPEC PR-1, PR-2).
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

    /** Dispatches synchronously and returns the Host's consumption result (SPEC FR-12). */
    override fun dispatch(event: HostEvent): Boolean {
        var result = 0L
        applyTransaction { apply -> result = connection.dispatchEvent(event, apply) }
        return result != 0L
    }

    /** Called once per frame after a Host worker asked for one (SPEC PR-3). */
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
                // A malformed batch must not take the process down (SPEC NFR-7).
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
        // A Host is free to reject the report itself: PR-2 gives the report no success
        // contract, and the Rust Host answers a `ProtocolError` event with a protocol-error
        // status because no handler owns it. Letting that failure out of here would turn a
        // reported error into a crashed composition, which is exactly what NFR-7 forbids,
        // so the report is best effort and the original error is what gets printed.
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
 * Where a protocol error goes when the Host will not take the report (SPEC NFR-7).
 *
 * Tests replace it to assert on what was reported; production leaves it printing.
 */
internal var onProtocolError: (TableError) -> Unit = { error ->
    System.err.println("dioxus-compose protocol error ${error.code}: ${error.message}")
}

/** Creates a Host bound to the composition's lifetime (SPEC PR-7 naming). */
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
 * Draws the Host's tree and runs the frame loop (SPEC PR-3).
 *
 * Frame requests coming from Host worker threads coalesce into at most one `render_frame`
 * per frame, inside `withFrameNanos`.
 */
@Composable
fun DioxusContent(host: DioxusHost, modifier: Modifier = Modifier) {
    LaunchedEffect(host) {
        var applied = FrameRequests.counter.value
        FrameRequests.counter.collect { requested ->
            if (requested == applied) return@collect
            applied = requested
            withFrameNanos { frameTimeNanos -> host.renderFrame(frameTimeNanos) }
        }
    }
    // FR-14.4: the theme is resolved once here, and every node reads it from the
    // CompositionLocal. A `SetTheme` is therefore one record on the wire and one
    // invalidation in Compose, not a SetProp per node.
    val platform = remember { detectHostPlatform() }
    val systemDark = systemDarkOverride ?: isSystemInDarkTheme()
    val theme = resolveTheme(host.table.theme, platform, systemDark)
    CompositionLocalProvider(LocalDesignTheme provides theme) {
        Box(modifier.background(theme.color(ColorRole.Background))) {
            host.roots.forEach { rootId ->
                androidx.compose.runtime.key(rootId) { RenderNode(rootId, host.table, host) }
            }
        }
    }
}

/**
 * Overrides the platform's dark mode reading, for tests and for the dev harness.
 *
 * `ColorScheme.FollowSystem` reads the platform (FR-14.3); nothing else consults this.
 */
var systemDarkOverride: Boolean? = null
