package dioxus.compose.liquidglass

import androidx.compose.ui.graphics.Color
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ContrastTest {

    @Test
    fun fr14_contrast_ratio_matches_the_wcag_anchors() {
        // The two ends of the scale are fixed by the formula, so they are the cheapest
        // check that the luminance transfer function is right rather than approximately
        // right.
        assertEquals(21f, contrastRatio(Color.Black, Color.White), 0.01f)
        assertEquals(1f, contrastRatio(Color.Red, Color.Red), 0.001f)
        // Mid grey against white, a value published widely enough to be a real anchor.
        assertEquals(3.95f, contrastRatio(Color(0xFF767676), Color.White), 0.05f)
    }

    @Test
    fun fr14_contrast_ratio_is_symmetric() {
        val a = Color(0xFF123456)
        val b = Color(0xFFABCDEF)
        assertEquals(contrastRatio(a, b), contrastRatio(b, a), 0.0001f)
    }

    @Test
    fun fr14_composite_over_resolves_a_translucent_colour_to_an_opaque_one() {
        val half = Color.Black.copy(alpha = 0.5f)
        val result = compositeOver(half, Color.White)
        assertEquals(1f, result.alpha)
        assertEquals(0.5f, result.red, 0.001f)

        // Fully opaque top wins outright; fully transparent top leaves the bottom alone.
        assertEquals(Color.Red, compositeOver(Color.Red, Color.White))
        assertEquals(Color.White, compositeOver(Color.Transparent, Color.White))
    }

    @Test
    fun fr14_ensure_contrast_leaves_a_colour_that_already_passes_alone() {
        val unchanged = ensureContrast(Color.White, Color.Black, 4.5f)
        assertEquals(Color.White, unchanged)
    }

    @Test
    fun fr14_ensure_contrast_moves_away_from_the_content_colour() {
        // Two near-identical greys: the result has to separate them, and has to do it by
        // moving further from the content colour rather than crossing it.
        val surface = Color(0xFF6E6E6E)
        val content = Color(0xFF747474)
        val fixed = ensureContrast(surface, content, 4.5f)

        assertTrue(contrastRatio(fixed, content) >= 4.5f)
        assertTrue(
            relativeLuminance(fixed) < relativeLuminance(surface),
            "a surface darker than its content should have darkened, not lightened",
        )

        val lighter = ensureContrast(Color(0xFF7A7A7A), Color(0xFF747474), 4.5f)
        assertTrue(relativeLuminance(lighter) > relativeLuminance(Color(0xFF7A7A7A)))
    }

    @Test
    fun fr14_ensure_contrast_moves_no_further_than_it_has_to() {
        val surface = Color(0xFF6E6E6E)
        val content = Color.White
        val fixed = ensureContrast(surface, content, 4.5f)
        // Landing on pure black would satisfy the ratio too, and would be the wrong
        // answer: the point is the smallest correction that works.
        assertTrue(contrastRatio(fixed, content) >= 4.5f)
        assertTrue(
            contrastRatio(fixed, content) < 6f,
            "overshot to ${contrastRatio(fixed, content)} to 1 for a 4.5 requirement",
        )
    }

    @Test
    fun fr14_ensure_contrast_returns_the_best_available_when_the_ratio_is_unreachable() {
        // Nothing reaches 21 to 1 against mid grey. The function has to answer with the
        // extreme rather than loop or return the input untouched.
        val fixed = ensureContrast(Color(0xFF808080), Color(0xFF808080), 21f)
        assertTrue(contrastRatio(fixed, Color(0xFF808080)) > 1f)
        assertEquals(1f, fixed.alpha)
    }
}
