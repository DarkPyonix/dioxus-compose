package dioxus.compose.test

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.runComposeUiTest
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dioxus.compose.foundation.navigationStripTestTag
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.IconRole
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.Theme
import dioxus.compose.protocol.ColorScheme
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.ui.node.nodeTestTag
import dioxus.compose.ui.platform.FrameRequestSource
import dioxus.compose.ui.platform.LocalFrameRequests
import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

private const val NAVIGATION = 1
private const val FIRST = 2
private const val SECOND = 3
private const val SCREEN = 4
private const val FIRST_HANDLER = 91L
private const val SECOND_HANDLER = 92L

/** One navigation with two destinations and a screen, under one named design system. */
private fun navigationBatch(system: DesignSystem = DesignSystem.Material3) = listOf(
    Mutation.SetTheme(Theme(system, system, ColorScheme.Light, adaptive = false)),
    Mutation.Create(NAVIGATION, WidgetKind.Navigation),
    Mutation.SetProp(NAVIGATION, PropertyKind.SelectedIndex, PropertyValue.Integer(0)),
    Mutation.Create(FIRST, WidgetKind.NavigationItem),
    Mutation.SetProp(FIRST, PropertyKind.Text, PropertyValue.Text("Tasks")),
    Mutation.SetProp(FIRST, PropertyKind.Icon, PropertyValue.Integer(IconRole.List.ordinal + 1L)),
    Mutation.SetProp(FIRST, PropertyKind.OnClick, PropertyValue.Integer(FIRST_HANDLER)),
    Mutation.Insert(NAVIGATION, FIRST, 0),
    Mutation.Create(SECOND, WidgetKind.NavigationItem),
    Mutation.SetProp(SECOND, PropertyKind.Text, PropertyValue.Text("Done")),
    Mutation.SetProp(SECOND, PropertyKind.Icon, PropertyValue.Integer(IconRole.Check.ordinal + 1L)),
    Mutation.SetProp(SECOND, PropertyKind.OnClick, PropertyValue.Integer(SECOND_HANDLER)),
    Mutation.Insert(NAVIGATION, SECOND, 1),
    Mutation.Create(SCREEN, WidgetKind.Text),
    Mutation.SetProp(SCREEN, PropertyKind.Text, PropertyValue.Text("the screen")),
    Mutation.Insert(NAVIGATION, SCREEN, 2),
)

private fun assertNear(expected: Dp, actual: Dp, what: String) {
    assertTrue(
        abs(expected.value - actual.value) <= 1f,
        "$what should be about ${expected.value}dp but was ${actual.value}dp",
    )
}

@OptIn(ExperimentalTestApi::class)
class NavigationTest {
    private val frames = FrameRequestSource()

    /**
     * The same records, at three widths, come out as three different shapes.
     *
     * Nothing in the batch changes between them: the Host declared one navigation and the
     * Renderer, which is the side that measured the window, chose the bar, the rail and
     * the drawer. A responsive claim made from one width is not a claim.
     */
    @Test
    fun fr21_one_declaration_is_a_bar_a_rail_and_a_drawer() = runComposeUiTest {
        var width by mutableStateOf(500.dp)
        setContent {
            CompositionLocalProvider(
                LocalFrameRequests provides frames,
                // Pinned, so the assertions are about dp rather than about this machine.
                LocalDensity provides Density(1f),
            ) {
                DioxusContent(
                    rememberDioxusHost(FakeHostConnection(navigationBatch())),
                    Modifier.requiredSize(width, 800.dp),
                )
            }
        }
        waitForIdle()

        // Compact: a bar across the bottom, as wide as the window.
        var whole = onNodeWithTag(nodeTestTag(NAVIGATION)).getBoundsInRoot()
        val bar = onNodeWithTag(navigationStripTestTag(NAVIGATION)).getBoundsInRoot()
        assertNear(500.dp, bar.right - bar.left, "the bar's width")
        assertNear(80.dp, bar.bottom - bar.top, "the bar's height")
        assertNear(whole.bottom, bar.bottom, "the bar's bottom edge")

        // Medium: a rail down the side, as tall as the window.
        width = 700.dp
        waitForIdle()
        whole = onNodeWithTag(nodeTestTag(NAVIGATION)).getBoundsInRoot()
        val rail = onNodeWithTag(navigationStripTestTag(NAVIGATION)).getBoundsInRoot()
        assertNear(80.dp, rail.right - rail.left, "the rail's width")
        assertNear(800.dp, rail.bottom - rail.top, "the rail's height")
        assertNear(whole.left, rail.left, "the rail's leading edge")

        // Expanded: the same column, wide enough to hold the labels beside the icons.
        width = 1100.dp
        waitForIdle()
        whole = onNodeWithTag(nodeTestTag(NAVIGATION)).getBoundsInRoot()
        val drawer = onNodeWithTag(navigationStripTestTag(NAVIGATION)).getBoundsInRoot()
        assertNear(280.dp, drawer.right - drawer.left, "the drawer's width")
        assertNear(800.dp, drawer.bottom - drawer.top, "the drawer's height")
        assertNear(whole.left, drawer.left, "the drawer's leading edge")

        // And the screen behind the destinations was drawn at every one of them.
        onNodeWithTag(nodeTestTag(SCREEN)).assertTextEquals("the screen")
    }

    /**
     * Choosing a destination is that destination's own click, once, and the selection moves
     * here rather than waiting for the Host to put it somewhere.
     */
    @Test
    fun fr21_choosing_a_destination_sends_one_click_and_moves_the_selection_here() =
        runComposeUiTest {
            val connection = FakeHostConnection(navigationBatch())
            setContent {
                CompositionLocalProvider(LocalFrameRequests provides frames) {
                    DioxusContent(rememberDioxusHost(connection))
                }
            }
            waitForIdle()
            onNodeWithTag(nodeTestTag(FIRST)).assertIsSelected()
            onNodeWithTag(nodeTestTag(SECOND)).assertIsNotSelected()

            onNodeWithTag(nodeTestTag(SECOND)).performClick()
            waitForIdle()

            val clicks = connection.events.filterIsInstance<HostEvent.Clicked>()
            assertEquals(1, clicks.size, "choosing a destination must cost exactly one event")
            assertEquals(SECOND, clicks[0].nodeId)
            assertEquals(SECOND_HANDLER, clicks[0].handlerId)
            // The Host answered with nothing at all, and the selection moved anyway.
            onNodeWithTag(nodeTestTag(SECOND)).assertIsSelected()
            onNodeWithTag(nodeTestTag(FIRST)).assertIsNotSelected()
        }

    /**
     * A destination that is not one of this design system's is still one of its own.
     *
     * Fluent's navigation view is narrower than Material's rail and marks its selection
     * differently; the point of the assertion is that the widget did not decide either.
     */
    @Test
    fun fr21_each_design_system_gives_the_rail_its_own_measurements() = runComposeUiTest {
        var system by mutableStateOf(DesignSystem.Material3)
        setContent {
            CompositionLocalProvider(
                LocalFrameRequests provides frames,
                LocalDensity provides Density(1f),
            ) {
                DioxusContent(
                    rememberDioxusHost(FakeHostConnection(navigationBatch(system))),
                    Modifier.requiredSize(700.dp, 800.dp),
                )
            }
        }
        waitForIdle()
        assertNear(
            80.dp,
            onNodeWithTag(navigationStripTestTag(NAVIGATION)).getBoundsInRoot().let { it.right - it.left },
            "Material's rail",
        )

        system = DesignSystem.Fluent
        waitForIdle()
        assertNear(
            48.dp,
            onNodeWithTag(navigationStripTestTag(NAVIGATION)).getBoundsInRoot().let { it.right - it.left },
            "Fluent's compact navigation pane",
        )
    }

    /** A destination on its own is still a destination, and draws what it was given. */
    @Test
    fun fr21_a_destination_outside_a_navigation_still_draws() = runComposeUiTest {
        val batch = listOf(
            Mutation.Create(FIRST, WidgetKind.NavigationItem),
            Mutation.SetProp(FIRST, PropertyKind.Text, PropertyValue.Text("Alone")),
        )
        setContent {
            CompositionLocalProvider(LocalFrameRequests provides frames) {
                DioxusContent(rememberDioxusHost(FakeHostConnection(batch)))
            }
        }
        waitForIdle()
        onNodeWithTag(nodeTestTag(FIRST)).assertIsDisplayed()
        onNodeWithTag(nodeTestTag(FIRST)).assertIsNotSelected()
    }
}
