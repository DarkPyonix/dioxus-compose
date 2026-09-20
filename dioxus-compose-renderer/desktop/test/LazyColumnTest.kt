package dioxus.compose.test

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.runComposeUiTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.Modifier as ProtocolModifier
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.foundation.requestedRange
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.DioxusHost
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.tooling.HostResponse
import dioxus.compose.ui.node.nodeTestTag
import dioxus.compose.foundation.LAZY_COLUMN_BUFFER

private const val LIST = 1
private const val RANGE_HANDLER = 91L
private const val ITEM_COUNT = 10_000

/** First node id of the materialised window; two nodes per item (Box wrapper plus Text). */
private const val FIRST_ITEM_NODE = 100

/**
 * What the Host answers to a `RangeRequested`: exactly `count` items starting at `start`,
 * replacing whatever window was materialised before.
 */
private fun window(start: Int, count: Int, previousCount: Int): List<Mutation> = buildList {
    for (offset in 0 until previousCount) {
        add(Mutation.Remove(FIRST_ITEM_NODE + offset * 2))
    }
    for (offset in 0 until count) {
        val box = FIRST_ITEM_NODE + offset * 2
        val text = box + 1
        val index = start + offset
        add(Mutation.Create(box, WidgetKind.Box))
        add(Mutation.SetProp(box, PropertyKind.ItemKey, PropertyValue.Text("key-$index")))
        add(Mutation.Create(text, WidgetKind.Text))
        add(Mutation.SetProp(text, PropertyKind.Text, PropertyValue.Text("message $index")))
        add(Mutation.Insert(box, text, 0))
        add(Mutation.Insert(LIST, box, offset))
    }
}

private fun lazyList() = listOf(
    Mutation.Create(LIST, WidgetKind.LazyColumn),
    Mutation.SetModifier(LIST, 0, ProtocolModifier.Size(200f, 200f)),
    Mutation.SetProp(LIST, PropertyKind.ItemCount, PropertyValue.Integer(ITEM_COUNT.toLong())),
    Mutation.SetProp(LIST, PropertyKind.OnRangeRequested, PropertyValue.Integer(RANGE_HANDLER)),
)

@OptIn(ExperimentalTestApi::class)
class LazyColumnTest {
    /**
     * FR-8: the Renderer asks for the visible range plus its own buffer, and the number of
     * nodes that exist is proportional to that window, not to the 10,000 item list.
     */
    @Test
    fun fr8_the_visible_range_is_requested_and_only_that_window_exists() = runComposeUiTest {
        val connection = FakeHostConnection(lazyList())
        var materialised = 0
        connection.respondWith { event ->
            if (event is HostEvent.RangeRequested) {
                val batch = window(event.start, event.count, materialised)
                materialised = event.count
                HostResponse(batch)
            } else {
                HostResponse()
            }
        }
        lateinit var host: DioxusHost
        setContent { host = rememberDioxusHost(connection) ; DioxusContent(host) }
        waitForIdle()

        val requests = connection.events.filterIsInstance<HostEvent.RangeRequested>()
        assertTrue(requests.isNotEmpty(), "the Renderer must ask for a range: ${connection.events}")
        val request = requests.last()
        assertEquals(LIST, request.nodeId)
        assertEquals(RANGE_HANDLER, request.handlerId)
        assertEquals(0, request.start, "the list starts at the top")
        assertTrue(
            request.count in 1 until ITEM_COUNT / 10,
            "a window of ${request.count} is not proportional to the visible range",
        )
        assertTrue(
            host.table.node(LIST)!!.children.size <= request.count,
            "the Host materialised more than the window it was asked for",
        )
    }

    /** FR-8: the first materialised child is the item at the requested `start`. */
    @Test
    fun fr8_the_window_is_drawn_at_the_requested_global_offset() = runComposeUiTest {
        val connection = FakeHostConnection(lazyList())
        var materialised = 0
        connection.respondWith { event ->
            if (event is HostEvent.RangeRequested) {
                val batch = window(event.start, event.count, materialised)
                materialised = event.count
                HostResponse(batch)
            } else {
                HostResponse()
            }
        }
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()

        val start = connection.events.filterIsInstance<HostEvent.RangeRequested>().last().start
        onNodeWithTag(nodeTestTag(FIRST_ITEM_NODE + 1)).assertTextEquals("message $start")
    }

    /** FR-8: the requested window is the visible range widened by the Renderer's buffer. */
    @Test
    fun fr8_the_request_widens_the_visible_range_by_the_buffer() {
        assertEquals(
            0 to 20 + LAZY_COLUMN_BUFFER,
            requestedRange(firstVisible = 0, visibleCount = 20, itemCount = ITEM_COUNT),
        )
        assertEquals(
            100 - LAZY_COLUMN_BUFFER to 20 + 2 * LAZY_COLUMN_BUFFER,
            requestedRange(firstVisible = 100, visibleCount = 20, itemCount = ITEM_COUNT),
        )
        // Clamped at both ends, and an empty list asks for nothing.
        assertEquals(
            ITEM_COUNT - 20 - LAZY_COLUMN_BUFFER to 20 + LAZY_COLUMN_BUFFER,
            requestedRange(firstVisible = ITEM_COUNT - 20, visibleCount = 20, itemCount = ITEM_COUNT),
        )
        assertEquals(0 to 0, requestedRange(firstVisible = 0, visibleCount = 20, itemCount = 0))
        assertEquals(0 to 0, requestedRange(firstVisible = 0, visibleCount = 0, itemCount = 10))
    }
}
