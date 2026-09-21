package dioxus.compose.test

import androidx.compose.foundation.layout.requiredSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.runComposeUiTest
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.protocol.WindowSizeClass
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.runtime.WindowSizeReporter
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.runtime.windowSizeClassOf
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.ui.platform.FrameRequestSource
import dioxus.compose.ui.platform.LocalFrameRequests
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

private const val LABEL = 1

/** Recording dispatcher: the Host is not involved in what the reporter decides to send. */
private class RecordingDispatcher : EventDispatcher {
    val events = mutableListOf<HostEvent>()

    override fun dispatch(event: HostEvent): Boolean {
        events += event
        return false
    }
}

/**
 * The window size class: which class a width belongs to, and the rule that the Host hears
 * about it only when the class changes.
 */
@OptIn(ExperimentalTestApi::class)
class WindowSizeTest {
    private val frames = FrameRequestSource()

    @Test
    fun fr20_size_classes_split_at_six_hundred_and_eight_hundred_and_forty_dp() {
        assertEquals(WindowSizeClass.Compact, windowSizeClassOf(0f))
        assertEquals(WindowSizeClass.Compact, windowSizeClassOf(599.9f))
        assertEquals(WindowSizeClass.Medium, windowSizeClassOf(600f))
        assertEquals(WindowSizeClass.Medium, windowSizeClassOf(839.9f))
        assertEquals(WindowSizeClass.Expanded, windowSizeClassOf(840f))
    }

    @Test
    fun fr20_crossing_a_boundary_sends_one_event_and_resizing_within_a_class_sends_none() {
        val dispatcher = RecordingDispatcher()
        val reporter = WindowSizeReporter()

        // A window that opens phone-sized says nothing: the Host already assumes Compact.
        // Nor does any width from 300dp to 599dp, all of them the same class.
        for (widthDp in 300..599) {
            reporter.report(widthDp.toFloat(), 800f, dispatcher)
        }
        assertEquals(0, dispatcher.events.size)

        // 599dp to 601dp crosses the boundary: exactly one event.
        assertTrue(reporter.report(601f, 800f, dispatcher))
        assertEquals(1, dispatcher.events.size)
        val event = dispatcher.events[0] as HostEvent.WindowSizeChanged
        assertEquals(WindowSizeClass.Medium, event.sizeClass)
        assertEquals(601f, event.widthDp)
        assertEquals(800f, event.heightDp)
        assertEquals(0, event.nodeId)
        assertEquals(0L, event.handlerId)

        // And back down is another single event, not a stream of them.
        for (widthDp in 600 downTo 300) {
            reporter.report(widthDp.toFloat(), 800f, dispatcher)
        }
        assertEquals(2, dispatcher.events.size)
        assertEquals(
            WindowSizeClass.Compact,
            (dispatcher.events[1] as HostEvent.WindowSizeChanged).sizeClass,
        )
    }

    @Test
    fun fr20_root_content_reports_its_measured_size_to_the_host() = runComposeUiTest {
        val connection = FakeHostConnection(
            listOf(
                Mutation.Create(LABEL, WidgetKind.Text),
                Mutation.SetProp(LABEL, PropertyKind.Text, PropertyValue.Text("hello")),
            ),
        )
        var width by mutableStateOf(500.dp)
        setContent {
            CompositionLocalProvider(
                LocalFrameRequests provides frames,
                // Pinned so the test asserts on dp, not on whatever density the machine
                // running it happens to have.
                LocalDensity provides Density(1f),
            ) {
                DioxusContent(
                    rememberDioxusHost(connection),
                    Modifier.requiredSize(width, 800.dp),
                )
            }
        }
        waitForIdle()
        val sizes = { connection.events.filterIsInstance<HostEvent.WindowSizeChanged>() }
        // A Compact window is what the Host already assumes, so nothing was sent.
        assertEquals(0, sizes().size)

        // Still Compact: the window changed size and the Host is not told.
        width = 560.dp
        waitForIdle()
        assertEquals(0, sizes().size)

        width = 700.dp
        waitForIdle()
        assertEquals(1, sizes().size)
        assertEquals(WindowSizeClass.Medium, sizes()[0].sizeClass)
        assertEquals(700f, sizes()[0].widthDp)

        width = 900.dp
        waitForIdle()
        assertEquals(2, sizes().size)
        assertEquals(WindowSizeClass.Expanded, sizes()[1].sizeClass)
    }
}
