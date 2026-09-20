package dioxus.compose.test

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.runComposeUiTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import dioxus.compose.protocol.HostEvent
import dioxus.compose.ui.platform.FrameRequests
import dioxus.compose.protocol.Modifier as ProtocolModifier
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.tooling.HostResponse
import dioxus.compose.ui.node.nodeTestTag

private const val ROOT = 1
private const val DISMISS_HANDLER = 61L
private const val FIRST_TAB_HANDLER = 71L
private const val SECOND_TAB_HANDLER = 72L
private const val RANGE_HANDLER = 73L

private fun text(id: Int, parent: Int, index: Int, value: String) = listOf(
    Mutation.Create(id, WidgetKind.Text),
    Mutation.SetProp(id, PropertyKind.Text, PropertyValue.Text(value)),
    Mutation.Insert(parent, id, index),
)

@OptIn(ExperimentalTestApi::class)
class ContainerWidgetsTest {
    /**
     * A Card is a container and nothing else on the wire: the Host sends children, never a
     * colour or a corner, and the children are drawn.
     */
    @Test
    fun fr15_a_card_draws_its_children_and_the_host_sends_no_appearance() = runComposeUiTest {
        val batch = listOf(Mutation.Create(ROOT, WidgetKind.Card)) + text(2, ROOT, 0, "grouped")
        setContent { DioxusContent(rememberDioxusHost(FakeHostConnection(batch))) }
        waitForIdle()

        onNodeWithTag(nodeTestTag(2)).assertTextEquals("grouped")
        onNodeWithTag(nodeTestTag(ROOT)).assertIsDisplayed()
    }

    /** A Surface is the same contract with a different meaning, and draws its children too. */
    @Test
    fun fr15_a_surface_draws_its_children() = runComposeUiTest {
        val batch = listOf(Mutation.Create(ROOT, WidgetKind.Surface)) + text(2, ROOT, 0, "plain")
        setContent { DioxusContent(rememberDioxusHost(FakeHostConnection(batch))) }
        waitForIdle()

        onNodeWithTag(nodeTestTag(2)).assertTextEquals("plain")
    }

    /** A bar lays its children out across itself, left to right. */
    @Test
    fun fr15_a_top_app_bar_draws_the_children_it_is_given() = runComposeUiTest {
        val batch = listOf(Mutation.Create(ROOT, WidgetKind.TopAppBar)) +
            text(2, ROOT, 0, "Inbox") +
            text(3, ROOT, 1, "Edit")
        setContent { DioxusContent(rememberDioxusHost(FakeHostConnection(batch))) }
        waitForIdle()

        onNodeWithTag(nodeTestTag(2)).assertTextEquals("Inbox")
        onNodeWithTag(nodeTestTag(3)).assertTextEquals("Edit")
    }

    /**
     * A Dialog that was never opened draws nothing, and the property the Host sends is what
     * opens it. Closing it again is a property too, not an event the Renderer waits for.
     */
    @Test
    fun fr15_a_dialog_shows_only_while_the_host_property_says_it_is_open() = runComposeUiTest {
        val closed = listOf(
            Mutation.Create(ROOT, WidgetKind.Dialog),
            Mutation.SetProp(ROOT, PropertyKind.OnDismiss, PropertyValue.Integer(DISMISS_HANDLER)),
        ) + text(2, ROOT, 0, "delete everything?")
        val connection = FakeHostConnection(closed)
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()
        onNodeWithTag(nodeTestTag(2)).assertDoesNotExist()

        connection.scheduleFrame(
            listOf(Mutation.SetProp(ROOT, PropertyKind.Open, PropertyValue.Bool(true))),
        )
        FrameRequests.request()
        waitForIdle()
        mainClock.advanceTimeByFrame()
        waitForIdle()
        onNodeWithTag(nodeTestTag(2)).assertTextEquals("delete everything?")
    }

    /**
     * The menu's first child is its anchor and is always drawn; the entries exist only
     * while it is expanded, which is the Renderer's own state.
     */
    @Test
    fun fr15_a_menu_draws_its_anchor_and_its_entries_only_when_expanded() = runComposeUiTest {
        val batch = listOf(Mutation.Create(ROOT, WidgetKind.Menu)) +
            text(2, ROOT, 0, "File") +
            text(3, ROOT, 1, "Open")
        val connection = FakeHostConnection(batch)
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()
        onNodeWithTag(nodeTestTag(2)).assertTextEquals("File")
        onNodeWithTag(nodeTestTag(3)).assertDoesNotExist()

        connection.scheduleFrame(
            listOf(Mutation.SetProp(ROOT, PropertyKind.Open, PropertyValue.Bool(true))),
        )
        FrameRequests.request()
        waitForIdle()
        mainClock.advanceTimeByFrame()
        waitForIdle()
        onNodeWithTag(nodeTestTag(3)).assertTextEquals("Open")
    }

    /**
     * Choosing a tab is that tab's own click and nothing else: the strip does not need the
     * Host to send the selection back for the new tab to look selected.
     */
    @Test
    fun fr15_choosing_a_tab_reports_that_tab_s_own_click() = runComposeUiTest {
        val batch = listOf(
            Mutation.Create(ROOT, WidgetKind.Tabs),
            Mutation.SetProp(ROOT, PropertyKind.SelectedIndex, PropertyValue.Integer(0)),
        ) + text(2, ROOT, 0, "One") + text(3, ROOT, 1, "Two") +
            listOf(
                Mutation.SetProp(2, PropertyKind.OnClick, PropertyValue.Integer(FIRST_TAB_HANDLER)),
                Mutation.SetProp(3, PropertyKind.OnClick, PropertyValue.Integer(SECOND_TAB_HANDLER)),
            )
        val connection = FakeHostConnection(batch)
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()

        onNodeWithTag(nodeTestTag(3)).performClick()
        waitForIdle()

        val clicks = connection.events.filterIsInstance<HostEvent.Clicked>()
        assertEquals(1, clicks.size, "one tab tap is one event: ${connection.events}")
        assertEquals(3, clicks.single().nodeId, "the node id says which tab was chosen")
        assertEquals(SECOND_TAB_HANDLER, clicks.single().handlerId)
    }

    /** The explanation is the child's description, so a pointer is not the only way to it. */
    @Test
    fun fr15_a_tooltip_describes_its_child_in_the_accessibility_tree() = runComposeUiTest {
        val batch = listOf(
            Mutation.Create(ROOT, WidgetKind.Tooltip),
            Mutation.SetProp(ROOT, PropertyKind.Text, PropertyValue.Text("Save the document")),
        ) + text(2, ROOT, 0, "Save")
        setContent { DioxusContent(rememberDioxusHost(FakeHostConnection(batch))) }
        waitForIdle()

        onNodeWithTag(nodeTestTag(ROOT)).assertContentDescriptionEquals("Save the document")
        onNodeWithTag(nodeTestTag(2)).assertTextEquals("Save")
    }

    /**
     * The horizontal list is the same windowing protocol as the vertical one: the Renderer
     * asks for a range, the Host answers it exactly, and only that window exists as nodes.
     */
    @Test
    fun fr15_a_lazy_row_asks_for_a_window_and_only_that_window_exists() = runComposeUiTest {
        val itemCount = 10_000
        val firstItemNode = 100
        val list = listOf(
            Mutation.Create(ROOT, WidgetKind.LazyRow),
            Mutation.SetModifier(ROOT, 0, ProtocolModifier.Size(200f, 60f)),
            Mutation.SetProp(ROOT, PropertyKind.ItemCount, PropertyValue.Integer(itemCount.toLong())),
            Mutation.SetProp(ROOT, PropertyKind.OnRangeRequested, PropertyValue.Integer(RANGE_HANDLER)),
        )
        val connection = FakeHostConnection(list)
        var materialised = 0
        connection.respondWith { event ->
            if (event is HostEvent.RangeRequested) {
                val batch = buildList {
                    for (offset in 0 until materialised) {
                        add(Mutation.Remove(firstItemNode + offset * 2))
                    }
                    for (offset in 0 until event.count) {
                        val box = firstItemNode + offset * 2
                        val label = box + 1
                        val index = event.start + offset
                        add(Mutation.Create(box, WidgetKind.Box))
                        add(Mutation.SetProp(box, PropertyKind.ItemKey, PropertyValue.Text("key-$index")))
                        add(Mutation.Create(label, WidgetKind.Text))
                        add(Mutation.SetProp(label, PropertyKind.Text, PropertyValue.Text("item $index")))
                        add(Mutation.Insert(box, label, 0))
                        add(Mutation.Insert(ROOT, box, offset))
                    }
                }
                materialised = event.count
                HostResponse(batch)
            } else {
                HostResponse()
            }
        }
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()

        val requests = connection.events.filterIsInstance<HostEvent.RangeRequested>()
        assertTrue(requests.isNotEmpty(), "the Renderer must ask for a range: ${connection.events}")
        val request = requests.last()
        assertEquals(ROOT, request.nodeId)
        assertEquals(RANGE_HANDLER, request.handlerId)
        assertEquals(0, request.start, "the list starts at its beginning")
        assertTrue(
            request.count in 1 until itemCount / 10,
            "a window, not the whole list: ${request.count}",
        )
        assertEquals(request.count, materialised, "the Host answers the range exactly")
        onNodeWithTag(nodeTestTag(firstItemNode + 1)).assertTextEquals("item 0")
    }
}
