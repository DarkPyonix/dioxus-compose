package dioxus.compose.tooling

import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.Mutation
import dioxus.compose.runtime.HostConnection

/** What a scripted Host answers to one event: a diff batch plus the handler result. */
data class HostResponse(
    val mutations: List<Mutation> = emptyList(),
    val result: Long = 0,
)

/**
 * In-memory Host used by previews, the JVM dev shell, and tests. Working on the renderer
 * must not require building the Rust side or a native image.
 *
 * It is a fake, not a mock: it behaves like the real Host from the interpreter's point of
 * view - batches arrive on the same call stack, and events are recorded in arrival order.
 */
class FakeHostConnection(
    private val initialBatch: List<Mutation> = emptyList(),
) : HostConnection {
    private val recordedEvents = mutableListOf<HostEvent>()
    private val frameBatches = ArrayDeque<List<Mutation>>()
    private var responder: (HostEvent) -> HostResponse = { HostResponse() }

    var initialized: Boolean = false
        private set
    var shutdownCalled: Boolean = false
        private set

    /** Events the interpreter has sent, in order. */
    val events: List<HostEvent> get() = recordedEvents

    /** Scripts the answer to every event. */
    fun respondWith(responder: (HostEvent) -> HostResponse) {
        this.responder = responder
    }

    /** Queues a batch to be returned by the next `renderFrame`. */
    fun scheduleFrame(mutations: List<Mutation>) {
        frameBatches.addLast(mutations)
    }

    override fun init(onMutation: (Mutation) -> Unit) {
        initialized = true
        initialBatch.forEach(onMutation)
    }

    override fun dispatchEvent(event: HostEvent, onMutation: (Mutation) -> Unit): Long {
        recordedEvents += event
        val response = responder(event)
        response.mutations.forEach(onMutation)
        return response.result
    }

    override fun renderFrame(frameTimeNanos: Long, onMutation: (Mutation) -> Unit) {
        frameBatches.removeFirstOrNull()?.forEach(onMutation)
    }

    override fun shutdown() {
        shutdownCalled = true
    }
}
