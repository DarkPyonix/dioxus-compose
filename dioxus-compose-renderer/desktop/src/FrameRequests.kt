package org.thisisthepy.dioxus.compose.nativeimage

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * Frame requests from Host worker threads (SPEC PR-3).
 *
 * Any number of requests between two frames coalesce into one: the renderer observes the
 * counter from the UI thread and calls `dioxus_compose_host_render_frame` once per change
 * it sees inside the frame clock.
 */
internal object FrameRequests {
    private val requests = MutableStateFlow(0L)

    val counter: StateFlow<Long> = requests.asStateFlow()

    /** Thread-safe; called from `dioxus_compose_renderer_request_frame`. */
    fun request() = requests.update { it + 1 }
}
