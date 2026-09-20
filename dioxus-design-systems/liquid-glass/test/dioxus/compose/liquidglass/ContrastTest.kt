package dioxus.compose.liquidglass

import androidx.compose.ui.graphics.Color
import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The colour arithmetic the fallback guarantee rests on.
 *
 * If these are wrong then every contrast claim in this module is wrong in the same
 * direction and nothing downstream would notice, which is why they are checked against
 * the published WCAG numbers rather than against themselves.
 */
class ContrastTest {

    @Test
    fun fr14_relative_luminance_matches_the_wcag_endpoints() {
        assertEquals(0f, relativeLuminance(Color.Black), 1e-6f)
        assertEquals(1f, relativeLuminance(Color.White), 1e-6f)
        // The sRGB primaries carry the coefficients in the formula.
        assertEquals(0.2126f, relativeLuminance(Color.Red), 1e-4f)
        assertEquals(0.7152f, relativeLuminance(Color.Green), 1e-4f)
        assertEquals(0.0722f, relativeLuminance(Color.Blue), 1e-4f)
    }

    @Test
    fun fr14_contrast_runs_from_one_to_twenty_one_and_does_not_care_about_order() {
        assertEquals(21f, contrastRatio(Color.Black, Color.White), 1e-4f)
        assertEquals(21f, contrastRatio(Color.White, Color.Black), 1e-4f)
        assertEquals(1f, contrastRatio(Color.Gray, Color.Gray), 1e-6f)
    }

    @Test
    fun fr14_compositing_a_translucent_colour_lands_between_the_two_and_is_opaque() {
        val result = compositeOver(Color.Black.copy(alpha = 0.5f), Color.White)
        assertEquals(1f, result.alpha)
        assertEquals(0.5f, result.red, 1e-4f)
        assertTrue(abs(relativeLuminance(result) - relativeLuminance(Color.White)) > 0.1f)
    }

    @Test
    fun fr14_compositing_at_the_extremes_returns_one_side_or_the_other() {
        assertEquals(Color.White, compositeOver(Color.Transparent, Color.White))
        assertEquals(
            Color.Black,
            compositeOver(Color.Black, Color.White),
            "a fully opaque top hides what is under it",
        )
    }

    @Test
    fun fr14_a_colour_that_already_meets_the_ratio_is_returned_untouched() {
        val pale = Color(0xFFF7F7FA)
        assertEquals(pale, ensureContrast(pale, Color.Black, 4.5f))
    }

    @Test
    fun fr14_a_colour_that_does_not_meet_the_ratio_is_moved_until_it_does() {
        val tooClose = Color(0xFF777777)
        val fixed = ensureContrast(tooClose, Color.Black, 4.5f)
        assertTrue(contrastRatio(fixed, Color.Black) >= 4.5f)
        assertTrue(
            relativeLuminance(fixed) > relativeLuminance(tooClose),
            "moving towards black would have had to pass through the content colour first",
        )
    }

    @Test
    fun fr14_the_move_is_the_smallest_one_that_satisfies_the_requirement() {
        val start = Color(0xFF777777)
        val fixed = ensureContrast(start, Color.Black, 4.5f)
        // Anything materially closer to the original fails the ratio, so the search did
        // not overshoot into a colour the design never asked for.
        val undershoot = Color(
            red = fixed.red - (fixed.red - start.red) * 0.2f,
            green = fixed.green - (fixed.green - start.green) * 0.2f,
            blue = fixed.blue - (fixed.blue - start.blue) * 0.2f,
        )
        assertTrue(contrastRatio(undershoot, Color.Black) < 4.5f)
    }

    @Test
    fun fr14_an_impossible_ratio_returns_the_best_that_colour_can_do() {
        // Nothing reaches 21 against mid grey, so the answer is the far extreme rather
        // than a failure or the unchanged input.
        val best = ensureContrast(Color(0xFF808080), Color.Gray, 21f)
        assertTrue(
            best == Color.White || best == Color.Black,
            "the extreme is the honest answer when the requirement cannot be met: $best",
        )
    }

    @Test
    fun fr14_ensure_contrast_returns_an_opaque_colour_so_it_can_be_trusted_as_a_fallback() {
        assertEquals(1f, ensureContrast(Color(0x80777777), Color.Black, 4.5f).alpha)
    }
}
