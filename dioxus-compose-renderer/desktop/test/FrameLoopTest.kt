package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.runComposeUiTest
import kotlin.test.Test
import org.thisisthepy.dioxus.compose.nativeimage.FrameRequests
import org.thisisthepy.dioxus.compose.protocol.Mutation
import org.thisisthepy.dioxus.compose.protocol.PropertyKind
import org.thisisthepy.dioxus.compose.protocol.PropertyValue
import org.thisisthepy.dioxus.compose.protocol.WidgetKind

private const val LABEL = 1

/** The frame loop: a worker's request turns into one `render_frame` call (SPEC PR-3). */
@OptIn(ExperimentalTestApi::class)
class FrameLoopTest {
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
