package dioxus.compose.ui.platform

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * Frame requests from Host worker threads.
 *
 * Domain work (network, file I/O, process management) runs on Host worker threads, never on
 * the UI thread. This counter is the one signal those threads send across: they update their
 * state and ask for a frame, and the UI thread does the rest.
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
