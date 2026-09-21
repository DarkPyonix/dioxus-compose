package dioxus.compose.test

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.runComposeUiTest
import androidx.compose.ui.unit.dp
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.WindowCaption
import dioxus.compose.runtime.opensWithABar
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.ui.node.NodeTable
import dioxus.compose.ui.node.nodeTestTag
import dioxus.compose.ui.platform.FrameRequestSource
import dioxus.compose.ui.platform.LocalFrameRequests
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

private const val ROOT = 1
private const val BAR = 2
private const val LABEL = 3

/** The height of the strip the window buttons sit in, for these tests. */
private val CAPTION = WindowCaption(height = 40.dp, buttonsWidth = 78.dp)

private fun label(id: Int, parent: Int, index: Int, value: String) = listOf(
    Mutation.Create(id, WidgetKind.Text),
    Mutation.SetProp(id, PropertyKind.Text, PropertyValue.Text(value)),
    Mutation.Insert(parent, id, index),
)

/** A window whose tree opens with a bar: `Column { TopAppBar { Text } }`. */
private val TREE_WITH_A_BAR =
    listOf(Mutation.Create(ROOT, WidgetKind.Column), Mutation.Create(BAR, WidgetKind.TopAppBar)) +
        label(LABEL, BAR, 0, "Title") +
        listOf(Mutation.Insert(ROOT, BAR, 0))

/** The same window without one: `Column { Text }`. */
private val TREE_WITHOUT_A_BAR =
    listOf(Mutation.Create(ROOT, WidgetKind.Column)) + label(LABEL, ROOT, 0, "Title")

@OptIn(ExperimentalTestApi::class)
class WindowCaptionTest {
    private val frames = FrameRequestSource()

    private fun tableOf(batch: List<Mutation>): NodeTable {
        val table = NodeTable()
        batch.forEach(table::apply)
        return table
    }

    /**
     * The bar is what runs across the top of the window, so it is the piece that has to
     * carry the caption. Anything else in the tree is not at the top of the window.
     */
    @Test
    fun fr19_2_a_tree_that_opens_with_a_bar_hands_it_the_caption() {
        val withBar = tableOf(TREE_WITH_A_BAR)
        assertTrue(withBar.opensWithABar(withBar.roots))

        val withoutBar = tableOf(TREE_WITHOUT_A_BAR)
        assertFalse(withoutBar.opensWithABar(withoutBar.roots))
    }

    /**
     * A bar that has the caption steps its content below the window buttons and clear of
     * them, rather than drawing the application's title where the traffic lights are.
     */
    @Test
    fun fr19_2_a_bar_lays_its_content_out_clear_of_the_window_buttons() = runComposeUiTest {
        setContent {
            CompositionLocalProvider(LocalFrameRequests provides frames) {
                DioxusContent(
                    rememberDioxusHost(FakeHostConnection(TREE_WITH_A_BAR)),
                    caption = CAPTION,
                )
            }
        }
        waitForIdle()

        val bar = onNodeWithTag(nodeTestTag(BAR)).getBoundsInRoot()
        val title = onNodeWithTag(nodeTestTag(LABEL)).getBoundsInRoot()
        // The bar itself starts at the very top of the window: its surface covers the
        // strip instead of beginning underneath it.
        assertTrue(bar.top.value <= 0.5f, "the bar starts at ${bar.top}, not at the top of the window")
        assertTrue(
            title.top >= CAPTION.height,
            "the title starts at ${title.top}, inside the ${CAPTION.height} caption",
        )
        assertTrue(
            title.left >= CAPTION.buttonsWidth,
            "the title starts at ${title.left}, over the window buttons",
        )
    }

    /** Without a bar the page keeps the caption, so nothing is drawn under the buttons. */
    @Test
    fun fr19_2_content_without_a_bar_starts_below_the_caption() = runComposeUiTest {
        setContent {
            CompositionLocalProvider(LocalFrameRequests provides frames) {
                DioxusContent(
                    rememberDioxusHost(FakeHostConnection(TREE_WITHOUT_A_BAR)),
                    caption = CAPTION,
                )
            }
        }
        waitForIdle()

        val title = onNodeWithTag(nodeTestTag(LABEL)).getBoundsInRoot()
        assertTrue(
            title.top >= CAPTION.height,
            "the title starts at ${title.top}, inside the ${CAPTION.height} caption",
        )
    }
}
