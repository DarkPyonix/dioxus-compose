package dioxus.compose.liquidglass

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import dioxus.compose.SurfaceMaterial
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class GlassMaterialTest {

    @Test
    fun fr14_glass_fallback_meets_body_text_contrast() {
        // Every combination the material is asked for has to produce a fallback that a
        // reader can read, including the ones where the requested tint lands close to the
        // content colour and has to be moved.
        for (dark in listOf(false, true)) {
            for (prominence in GlassProminence.entries) {
                val content = if (dark) Color.White else Color.Black
                val backdrop = if (dark) Color(0xFF1C1C1E) else Color(0xFFF2F2F7)
                val glass = LiquidGlass.material(
                    dark = dark,
                    prominence = prominence,
                    backdrop = backdrop,
                    content = content,
                )
                val ratio = contrastRatio(glass.fallback, content)
                assertTrue(
                    ratio >= LiquidGlass.MIN_CONTRAST_BODY,
                    "dark=$dark $prominence fallback ${glass.fallback} against $content " +
                        "is $ratio to 1, below the ${LiquidGlass.MIN_CONTRAST_BODY} to 1 " +
                        "that body text needs",
                )
                assertEquals(1f, glass.fallback.alpha, "the fallback has to be opaque")
            }
        }
    }

    @Test
    fun fr14_glass_fallback_is_moved_only_when_the_tint_alone_would_fail() {
        // A backdrop the tint already reads well against should come back untouched: the
        // fallback is meant to be what the glass looks like, corrected only where it must
        // be.
        val glass = LiquidGlass.material(
            dark = false,
            backdrop = Color.White,
            content = Color.Black,
        )
        val uncorrected = compositeOver(
            glass.tint.copy(alpha = glass.tintAlpha),
            Color.White,
        )
        assertEquals(uncorrected, glass.fallback)

        // A backdrop that would leave the surface nearly the colour of its own text has to
        // be corrected away from it.
        val hostile = LiquidGlass.material(
            dark = false,
            prominence = GlassProminence.Clear,
            backdrop = Color(0xFF222222),
            content = Color.Black,
        )
        assertTrue(
            contrastRatio(hostile.fallback, Color.Black) >= LiquidGlass.MIN_CONTRAST_BODY,
            "a dark backdrop under a barely tinted material has to be corrected, not " +
                "passed through at ${hostile.fallback}",
        )
    }

    @Test
    fun fr14_edge_is_lighter_at_the_top_than_at_the_bottom() {
        // The whole difference between a lit rim and a drawn border is that the two sides
        // are not the same. If a change ever made them equal the surface would flatten,
        // and nothing else in the module would notice.
        for (dark in listOf(false, true)) {
            val glass = LiquidGlass.material(
                dark = dark,
                backdrop = Color.Gray,
                content = if (dark) Color.White else Color.Black,
            )
            assertTrue(
                relativeLuminance(glass.highlight) > relativeLuminance(glass.shade),
                "dark=$dark highlight ${glass.highlight} is not lighter than shade " +
                    "${glass.shade}",
            )
            assertTrue(glass.highlight.alpha > 0f && glass.shade.alpha > 0f)
        }
    }

    @Test
    fun fr14_stacked_layers_gain_tint_rather_than_transparency() {
        val base = LiquidGlass.material(
            dark = false,
            backdrop = Color.White,
            content = Color.Black,
        )
        val first = base.atDepth(1)
        val second = base.atDepth(2)

        assertEquals(base.tintAlpha, base.atDepth(0).tintAlpha, "depth 0 is the base layer")
        assertTrue(first.tintAlpha > base.tintAlpha)
        assertTrue(second.tintAlpha > first.tintAlpha)
        // A stack deep enough to reach opacity would stop being glass, so the rise stops
        // short of it.
        assertTrue(base.atDepth(20).tintAlpha < 1f)
    }

    @Test
    fun fr14_prominence_trades_blur_and_tint_against_what_shows_through() {
        val regular = LiquidGlass.material(
            dark = false, prominence = GlassProminence.Regular,
            backdrop = Color.White, content = Color.Black,
        )
        val clear = LiquidGlass.material(
            dark = false, prominence = GlassProminence.Clear,
            backdrop = Color.White, content = Color.Black,
        )
        assertTrue(regular.tintAlpha > clear.tintAlpha)
        assertTrue(regular.blurRadius > clear.blurRadius)
        assertTrue(clear.blurRadius > 0.dp)
    }

    @Test
    fun fr14_material_is_glass_and_not_an_opaque_value_in_disguise() {
        val glass: SurfaceMaterial = LiquidGlass.material(
            dark = false,
            backdrop = Color.White,
            content = Color.Black,
        )
        assertTrue(glass is SurfaceMaterial.Glass)
        assertTrue(glass.tintAlpha < 1f, "a fully opaque tint would not be a material")
    }
}
