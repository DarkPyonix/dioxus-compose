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

    /**
     * Puts the counter back to zero, for a test that is about to install a fresh host.
     *
     * This object is global because the C entry point that feeds it takes no host: one
     * process, one isolate, one host. That is right at run time and leaves tests sharing
     * it, so a request left behind by one can drive another's frame loop. Resetting is
     * cheaper than making the counter per-host and pretending the boundary has a handle
     * it does not have.
     */
    fun resetForTest() = requests.update { 0L }
}
