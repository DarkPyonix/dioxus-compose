package dioxus.compose.test

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.runComposeUiTest
import kotlin.test.Test
import dioxus.compose.ui.platform.FrameRequests
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.ui.node.nodeTestTag
import kotlin.test.BeforeTest

private const val LABEL = 1

/** The frame loop: however many requests a worker makes, one `render_frame` call follows. */
@OptIn(ExperimentalTestApi::class)
class FrameLoopTest {
    @BeforeTest
    fun resetTheSharedFrameCounter() {
        // The frame counter is global, because the C entry point that feeds it takes no
        // host. Tests therefore share it, and a request left behind by one can drive
        // another's frame loop and keep it from ever going idle.
        FrameRequests.resetForTest()
    }

    @Test
    fun pr3_frame_request_applies_exactly_one_frame_batch() = runComposeUiTest {
        val connection = FakeHostConnection(
            listOf(
                Mutation.Create(LABEL, WidgetKind.Text),
                Mutation.SetProp(LABEL, PropertyKind.Text, PropertyValue.Text("initial")),
            ),
        )
        connection.scheduleFrame(
            listOf(Mutation.SetProp(LABEL, PropertyKind.Text, PropertyValue.Text("first frame"))),
        )
        connection.scheduleFrame(
            listOf(Mutation.SetProp(LABEL, PropertyKind.Text, PropertyValue.Text("second frame"))),
        )
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()
        onNodeWithTag(nodeTestTag(LABEL)).assertTextEquals("initial")

        FrameRequests.request()
        waitForIdle()
        mainClock.advanceTimeByFrame()
        waitForIdle()

        // Only the first queued batch is consumed: one request, one frame.
        onNodeWithTag(nodeTestTag(LABEL)).assertTextEquals("first frame")
    }
}
