package dioxus.compose.test

import dioxus.compose.design.NavigationPresentation
import dioxus.compose.foundation.ScaffoldFrame
import dioxus.compose.foundation.floatingActionFloats
import dioxus.compose.foundation.scaffoldFrame
import dioxus.compose.protocol.WindowSizeClass
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * How a screen's frame changes with the width of the window.
 *
 * The application fills slots and says nothing about what they become, so this is the
 * whole of what it gets in return and the part worth fixing. A phone stacks the bars
 * above and below the page; anything wider puts the destinations down the leading edge
 * beside it.
 */
class ScaffoldTest {

    @Test
    fun fr31_a_phone_stacks_the_bars_and_anything_wider_sets_them_beside() {
        assertEquals(
            ScaffoldFrame.Stacked,
            scaffoldFrame(NavigationPresentation.Bar, destinationsCanTurn = true),
            "a bar across the bottom is the phone frame",
        )
        assertEquals(ScaffoldFrame.SideBySide, scaffoldFrame(NavigationPresentation.Rail, destinationsCanTurn = true))
        assertEquals(ScaffoldFrame.SideBySide, scaffoldFrame(NavigationPresentation.Drawer, destinationsCanTurn = true))
    }

    @Test
    fun fr31_the_frame_follows_the_design_systems_own_answer() {
        // Read off the navigation presentation rather than decided again. A frame that
        // decided separately could put a rail down the side and still leave room for a
        // bar at the bottom, and nothing would report that as wrong.
        for (presentation in NavigationPresentation.entries) {
            val expected = if (presentation == NavigationPresentation.Bar) {
                ScaffoldFrame.Stacked
            } else {
                ScaffoldFrame.SideBySide
            }
            assertEquals(expected, scaffoldFrame(presentation, destinationsCanTurn = true), "$presentation")
        }
    }

    @Test
    fun fr31_a_bar_the_application_drew_itself_stays_a_bar() {
        // Only a navigation is one declaration the renderer can stand on end. A row of
        // icons an application laid out is a row: down the leading edge it keeps its own
        // width, takes the page's, and leaves a strip across the top with nothing under
        // it, which is what two samples drew the first time this ran.
        for (presentation in NavigationPresentation.entries) {
            assertEquals(
                ScaffoldFrame.Stacked,
                scaffoldFrame(presentation, destinationsCanTurn = false),
                "$presentation",
            )
        }
    }

    @Test
    fun fr31_the_floating_action_only_floats_where_a_thumb_reaches_it() {
        assertTrue(floatingActionFloats(WindowSizeClass.Compact))
        assertFalse(
            floatingActionFloats(WindowSizeClass.Medium),
            "on a wide window the pointer is already at the top, and a button floating " +
                "over the bottom corner is a phone habit",
        )
        assertFalse(floatingActionFloats(WindowSizeClass.Expanded))
    }
}
