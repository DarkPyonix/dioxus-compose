package dioxus.compose.test

import androidx.compose.foundation.layout.requiredSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.runComposeUiTest
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dioxus.compose.protocol.ColorScheme
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.Theme
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.WindowCaption
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.ui.node.nodeTestTag
import dioxus.compose.ui.platform.FrameRequestSource
import dioxus.compose.ui.platform.LocalFrameRequests
import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertTrue

private const val SCREEN = 1
private const val BAR = 2
private const val TITLE = 3

/** The window buttons of a platform that keeps its own, at the size macOS uses. */
private val CAPTION = WindowCaption(height = 28.dp, buttonsWidth = 78.dp)

/** A screen that leads with a bar, which is the shape every sample in this repository has. */
private fun screenWithBar() = listOf(
    Mutation.SetTheme(
        Theme(DesignSystem.Material3, DesignSystem.Material3, ColorScheme.Light, adaptive = false),
    ),
    Mutation.Create(SCREEN, WidgetKind.Column),
    Mutation.Create(BAR, WidgetKind.TopAppBar),
    Mutation.Insert(SCREEN, BAR, 0),
    Mutation.Create(TITLE, WidgetKind.Text),
    Mutation.SetProp(TITLE, PropertyKind.Text, PropertyValue.Text("Calculator")),
    Mutation.Insert(BAR, TITLE, 0),
)

/** The same screen with nothing at the top that could be a caption. */
private fun screenWithoutBar() = listOf(
    Mutation.SetTheme(
        Theme(DesignSystem.Material3, DesignSystem.Material3, ColorScheme.Light, adaptive = false),
    ),
    Mutation.Create(SCREEN, WidgetKind.Column),
    Mutation.Create(TITLE, WidgetKind.Text),
    Mutation.SetProp(TITLE, PropertyKind.Text, PropertyValue.Text("Calculator")),
    Mutation.Insert(SCREEN, TITLE, 0),
)

private fun assertNear(expected: Dp, actual: Dp, what: String) {
    assertTrue(
        abs(expected.value - actual.value) <= 1f,
        "$what should be about ${expected.value}dp but was ${actual.value}dp",
    )
}

@OptIn(ExperimentalTestApi::class)
class WindowCaptionTest {
    private val frames = FrameRequestSource()

    /**
     * A tree that leads with a bar makes that bar the caption: it starts at the very top of
     * the window and its contents begin after the room the system's buttons take.
     *
     * The alternative is what this used to do, which was to push every tree below the
     * caption whatever it held. That leaves a strip of empty window above the bar on macOS,
     * with the traffic lights floating in it, which is exactly the decade-old look modern
     * chrome exists to avoid.
     */
    @Test
    fun fr19_a_screen_that_leads_with_a_bar_lays_it_out_around_the_window_buttons() =
        runComposeUiTest {
            setContent {
                CompositionLocalProvider(
                    LocalFrameRequests provides frames,
                    LocalDensity provides Density(1f),
                ) {
                    DioxusContent(
                        rememberDioxusHost(FakeHostConnection(screenWithBar())),
                        Modifier.requiredSize(900.dp, 600.dp),
                        caption = CAPTION,
                    )
                }
            }
            waitForIdle()

            val bar = onNodeWithTag(nodeTestTag(BAR)).getBoundsInRoot()
            assertNear(0.dp, bar.top, "the bar's top edge")
            assertNear(0.dp, bar.left, "the bar's leading edge")

            val title = onNodeWithTag(nodeTestTag(TITLE)).getBoundsInRoot()
            assertTrue(
                title.left.value >= CAPTION.buttonsWidth.value,
                "the title starts at ${title.left.value}dp, inside the room the window " +
                    "buttons take",
            )
        }

    /** A screen with no bar is pushed clear of the buttons instead. */
    @Test
    fun fr19_a_screen_with_no_bar_keeps_its_content_clear_of_the_window_buttons() =
        runComposeUiTest {
            setContent {
                CompositionLocalProvider(
                    LocalFrameRequests provides frames,
                    LocalDensity provides Density(1f),
                ) {
                    DioxusContent(
                        rememberDioxusHost(FakeHostConnection(screenWithoutBar())),
                        Modifier.requiredSize(900.dp, 600.dp),
                        caption = CAPTION,
                    )
                }
            }
            waitForIdle()

            val title = onNodeWithTag(nodeTestTag(TITLE)).getBoundsInRoot()
            assertNear(CAPTION.height, title.top, "the content's top edge")
        }

    /** Nothing is in the way when the platform draws its own title bar. */
    @Test
    fun fr19_system_chrome_leaves_the_content_where_it_is() = runComposeUiTest {
        setContent {
            CompositionLocalProvider(
                LocalFrameRequests provides frames,
                LocalDensity provides Density(1f),
            ) {
                DioxusContent(
                    rememberDioxusHost(FakeHostConnection(screenWithoutBar())),
                    Modifier.requiredSize(900.dp, 600.dp),
                    caption = WindowCaption.None,
                )
            }
        }
        waitForIdle()

        val title = onNodeWithTag(nodeTestTag(TITLE)).getBoundsInRoot()
        assertNear(0.dp, title.top, "the content's top edge")
    }
}
