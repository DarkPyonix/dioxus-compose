package dioxus.compose.test

import dioxus.compose.runtime.captionRowPlacement
import kotlin.test.Test
import kotlin.test.assertEquals

/**
 * Where a bar's content sits against the window buttons.
 *
 * macOS keeps its own buttons and centres them in the standard title bar, and nothing
 * reachable from here moves them. A bar that grew past that strip and then centred its
 * content in its own height put the title 15 pixels below the buttons, which is what was
 * measured on the calculator before this.
 */
class CaptionRowTest {

    @Test
    fun fr19_2_content_that_fits_the_strip_shares_the_buttons_line() {
        val placement = captionRowPlacement(contentHeight = 20, bandHeight = 28)
        assertEquals(4, placement.contentTop, "centred in the strip, so on the buttons' line")
        assertEquals(28, placement.height, "the bar is the strip and no taller")
    }

    @Test
    fun fr19_2_content_taller_than_the_strip_starts_below_it() {
        // The case that was wrong. 59 of content in a 28 strip used to centre at 29.5,
        // and the buttons stay at 14.
        val placement = captionRowPlacement(contentHeight = 59, bandHeight = 28)
        assertEquals(28, placement.contentTop, "it cannot share a line it does not fit on")
        assertEquals(87, placement.height, "the strip is above the content, not behind it")
    }

    @Test
    fun fr19_2_exactly_the_strip_shares_it() {
        val placement = captionRowPlacement(contentHeight = 28, bandHeight = 28)
        assertEquals(0, placement.contentTop)
        assertEquals(28, placement.height)
    }

    @Test
    fun fr19_2_a_bar_that_is_not_the_caption_is_left_alone() {
        val placement = captionRowPlacement(contentHeight = 56, bandHeight = 0)
        assertEquals(0, placement.contentTop, "there is nothing to line up with")
        assertEquals(56, placement.height)
    }
}
