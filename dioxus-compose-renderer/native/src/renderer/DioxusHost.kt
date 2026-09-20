package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.withFrameNanos
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.Modifier
import org.thisisthepy.dioxus.compose.nativeimage.FrameRequests
import org.thisisthepy.dioxus.compose.protocol.HostEvent
import org.thisisthepy.dioxus.compose.protocol.Mutation

/** Sends one event to the Host and reports whether the Host consumed it (SPEC FR-3, FR-12). */
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
                    error.message ?: error::class.java.name,
                )
            }
            protocolErrors += table.drainErrors()
        }
        // Reported after the transaction so the Host is never re-entered mid-batch.
        protocolErrors.forEach { error ->
            connection.dispatchEvent(
                HostEvent.ProtocolError(
                    nodeId = NodeTable.ROOT_ID,
                    handlerId = 0,
                    code = error.code,
                    message = error.message,
                ),
            ) { mutation -> Snapshot.withMutableSnapshot { table.apply(mutation) } }
        }
    }

    private companion object {
        const val PROTOCOL_DECODE_ERROR = 100
    }
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
    Box(modifier) {
        host.roots.forEach { rootId ->
            androidx.compose.runtime.key(rootId) { RenderNode(rootId, host.table, host) }
        }
    }
}
