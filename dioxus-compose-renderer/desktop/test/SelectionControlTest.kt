package dioxus.compose.test

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.runComposeUiTest
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import dioxus.compose.protocol.ColorScheme
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.Theme
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.ui.node.nodeTestTag

private const val CONTROL = 1
private const val HANDLER = 71L

private fun theme(system: DesignSystem) =
    Mutation.SetTheme(Theme(system, DesignSystem.Material3, ColorScheme.Light, false))

private fun toggleTree(
    widget: WidgetKind,
    checked: Boolean,
    system: DesignSystem = DesignSystem.Material3,
): List<Mutation> = listOf(
    theme(system),
    Mutation.Create(CONTROL, widget),
    Mutation.SetProp(CONTROL, PropertyKind.Checked, PropertyValue.Bool(checked)),
    Mutation.SetProp(CONTROL, PropertyKind.OnValueChange, PropertyValue.Integer(HANDLER)),
)

private fun sliderTree(value: Float, steps: Long): List<Mutation> = listOf(
    theme(DesignSystem.Material3),
    Mutation.Create(CONTROL, WidgetKind.Slider),
    Mutation.SetProp(CONTROL, PropertyKind.Value, PropertyValue.Float(value)),
    Mutation.SetProp(CONTROL, PropertyKind.Min, PropertyValue.Float(0f)),
    Mutation.SetProp(CONTROL, PropertyKind.Max, PropertyValue.Float(4f)),
    Mutation.SetProp(CONTROL, PropertyKind.Steps, PropertyValue.Integer(steps)),
    Mutation.SetProp(CONTROL, PropertyKind.OnValueChange, PropertyValue.Integer(HANDLER)),
)

/**
 * The selection controls and the indicators. The widget sends a state; the design system
 * decides every pixel of it, and these tests are what keeps that split honest.
 */
@OptIn(ExperimentalTestApi::class)
class SelectionControlTest {

    /** All six tags reach a composable rather than falling through the interpreter. */
    @Test
    fun fr15_2_4_every_selection_control_is_drawn() {
        val widgets = listOf(
            WidgetKind.Checkbox,
            WidgetKind.RadioButton,
            WidgetKind.Switch,
            WidgetKind.Slider,
            WidgetKind.ProgressIndicator,
            WidgetKind.Divider,
        )
        widgets.forEach { widget ->
            runComposeUiTest {
                val connection = FakeHostConnection(
                    listOf(theme(DesignSystem.Material3), Mutation.Create(CONTROL, widget)),
                )
                setContent { DioxusContent(rememberDioxusHost(connection)) }
                waitForIdle()
                onNodeWithTag(nodeTestTag(CONTROL)).assertIsDisplayed()
            }
        }
    }

    /**
     * A toggle is controlled. Pressing it reports the state the user asked for and leaves
     * what is on screen alone, so the Host stays the one that owns the value.
     */
    @Test
    fun fr15_2_4_pressing_a_toggle_reports_the_requested_state() = runComposeUiTest {
        val connection = FakeHostConnection(toggleTree(WidgetKind.Checkbox, checked = false))
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()

        onNodeWithTag(nodeTestTag(CONTROL)).performClick()
        waitForIdle()

        val changes = connection.events.filterIsInstance<HostEvent.ValueChanged>()
        assertEquals(1, changes.size, "one press is one event")
        assertEquals(CONTROL, changes.single().nodeId)
        assertEquals(HANDLER, changes.single().handlerId)
        assertEquals(1.0, changes.single().value, "an unchecked box asks to be checked")
    }

    /** Turning something off is the same event carrying the other value. */
    @Test
    fun fr15_2_4_a_checked_toggle_asks_to_be_turned_off() = runComposeUiTest {
        val connection = FakeHostConnection(toggleTree(WidgetKind.Switch, checked = true))
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()

        onNodeWithTag(nodeTestTag(CONTROL)).performClick()
        waitForIdle()

        assertEquals(
            0.0,
            connection.events.filterIsInstance<HostEvent.ValueChanged>().single().value,
        )
    }

    /**
     * The three systems do not merely recolour one switch. A Material track is wider than
     * an Apple one and both are wider than a Fluent toggle, and a design system that only
     * changed the colours would fail here.
     */
    @Test
    fun fr14_1_a_switch_has_its_own_dimensions_in_each_design_system() {
        val widths = DesignSystem.entries.associateWith { system ->
            var width = 0f
            runComposeUiTest {
                val connection = FakeHostConnection(
                    toggleTree(WidgetKind.Switch, checked = true, system = system),
                )
                setContent { DioxusContent(rememberDioxusHost(connection)) }
                waitForIdle()
                width = onNodeWithTag(nodeTestTag(CONTROL))
                    .getUnclippedBoundsInRoot()
                    .width
                    .value
            }
            width
        }
        assertEquals(
            widths.size,
            widths.values.distinct().size,
            "a switch is that system's own control, so no two can measure alike: $widths",
        )
    }

    /** A checkbox and a radio button are not the same control with a different corner. */
    @Test
    fun fr14_1_a_checkbox_and_a_radio_button_are_drawn_to_their_own_sizes() {
        fun sizeOf(widget: WidgetKind): Float {
            var size = 0f
            runComposeUiTest {
                val connection = FakeHostConnection(toggleTree(widget, checked = true))
                setContent { DioxusContent(rememberDioxusHost(connection)) }
                waitForIdle()
                size = onNodeWithTag(nodeTestTag(CONTROL))
                    .getUnclippedBoundsInRoot()
                    .height
                    .value
            }
            return size
        }
        assertTrue(
            sizeOf(WidgetKind.Checkbox) != sizeOf(WidgetKind.RadioButton),
            "Material draws an 18 dp box and a 20 dp ring",
        )
    }

    /**
     * A stepped slider settles on a stop. The position a drag passes through stays here,
     * so only the value it lands on crosses the boundary.
     */
    @Test
    fun fr15_2_4_a_stepped_slider_reports_the_nearest_stop() = runComposeUiTest {
        val connection = FakeHostConnection(sliderTree(value = 0f, steps = 3))
        setContent { DioxusContent(rememberDioxusHost(connection)) }
        waitForIdle()

        // The centre of the track, which is the stop at two out of four.
        onNodeWithTag(nodeTestTag(CONTROL)).performClick()
        waitForIdle()

        val changes = connection.events.filterIsInstance<HostEvent.ValueChanged>()
        assertEquals(1, changes.size)
        assertEquals(2.0, changes.single().value, "the middle of a 0 to 4 range with three stops")
    }

    /** A divider carries nothing but its axis, so the two run different ways. */
    @Test
    fun fr15_2_4_a_divider_runs_along_the_axis_it_was_given() {
        fun boundsOf(vertical: Boolean): Pair<Float, Float> {
            var size = 0f to 0f
            runComposeUiTest {
                val connection = FakeHostConnection(
                    listOf(
                        theme(DesignSystem.Material3),
                        Mutation.Create(CONTROL, WidgetKind.Divider),
                        Mutation.SetProp(
                            CONTROL,
                            PropertyKind.Vertical,
                            PropertyValue.Bool(vertical),
                        ),
                    ),
                )
                setContent { DioxusContent(rememberDioxusHost(connection)) }
                waitForIdle()
                val bounds = onNodeWithTag(nodeTestTag(CONTROL)).getUnclippedBoundsInRoot()
                size = bounds.width.value to bounds.height.value
            }
            return size
        }

        val horizontal = boundsOf(vertical = false)
        val vertical = boundsOf(vertical = true)
        assertTrue(horizontal.first > horizontal.second, "a horizontal rule is wide and thin")
        assertTrue(vertical.second > vertical.first, "a vertical rule is tall and thin")
    }
}
