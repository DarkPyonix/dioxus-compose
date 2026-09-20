package org.thisisthepy.dioxus.compose.renderer

import org.thisisthepy.dioxus.compose.protocol.HostEvent
import org.thisisthepy.dioxus.compose.protocol.Mutation

/**
 * Synchronous, same-thread view of the Host (SPEC PR-1, PR-2).
 *
 * Every call runs on the Renderer UI thread and returns on the same call stack: the batch a
 * call produces is handed to `onMutation` before the call returns, and is released by the
 * implementation immediately afterwards. There is no queue between the two sides.
 */
interface HostConnection {
    /** Performs the handshake and streams the initial tree batch. */
    fun init(onMutation: (Mutation) -> Unit)

    /**
     * Dispatches one event and streams the resulting diff batch.
     *
     * @return the handler's synchronous result (SPEC FR-12). Non-zero means "consumed".
     */
    fun dispatchEvent(event: HostEvent, onMutation: (Mutation) -> Unit): Long

    /** Applies state changes scheduled by Host workers before the current frame (SPEC PR-3). */
    fun renderFrame(frameTimeNanos: Long, onMutation: (Mutation) -> Unit)

    fun shutdown()
}
