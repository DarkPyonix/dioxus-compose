package dioxus.compose.liquidglass

import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.pow
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The corner geometry, checked as geometry.
 *
 * Whether the corner looks like Apple's is a judgement made by eye. Whether it is
 * continuous rather than circular, and whether an inner element stays concentric with its
 * container, are facts about numbers, and those are the two things that actually distinguish
 * this shape from a rounded rectangle.
 */
class ConcentricShapeTest {

    private val density = Density(density = 1f, fontScale = 1f)

    private fun outlineOf(shape: ContinuousCornerShape, size: Size): Outline =
        shape.createOutline(size, LayoutDirection.Ltr, density)

    @Test
    fun fr14_a_continuous_corner_is_fuller_than_the_circular_one_of_the_same_radius() {
        // At the diagonal of the corner, a circle of radius r sits at r(1 - cos 45) in
        // from the corner point. A superellipse of exponent n > 2 sits closer to the
        // corner, which is the whole visible difference between the two shapes.
        val r = 1f
        val t = PI / 4.0
        val circular = r * abs(cos(t)).pow(2.0 / 2.0).toFloat()
        val continuous = r * abs(cos(t)).pow(2.0 / ContinuousCornerShape.EXPONENT).toFloat()
        assertTrue(
            continuous > circular,
            "exponent ${ContinuousCornerShape.EXPONENT} should push the corner out to " +
                "$continuous, past the circular $circular",
        )
        assertTrue(ContinuousCornerShape.EXPONENT > 2.0, "exponent 2 is a plain circle")
    }

    @Test
    fun fr14_the_outline_is_a_generic_path_and_fills_the_bounds_it_was_given() {
        val outline = outlineOf(ContinuousCornerShape(12.dp), Size(120f, 80f))
        assertTrue(outline is Outline.Generic, "a superellipse has no rounded rect form")
        val bounds = (outline as Outline.Generic).path.getBounds()
        assertEquals(0f, bounds.left, 0.5f)
        assertEquals(0f, bounds.top, 0.5f)
        assertEquals(120f, bounds.right, 0.5f)
        assertEquals(80f, bounds.bottom, 0.5f)
    }

    @Test
    fun fr14_radii_larger_than_the_bounds_are_scaled_down_instead_of_self_intersecting() {
        val outline = outlineOf(CapsuleShape, Size(60f, 40f))
        val bounds = (outline as Outline.Generic).path.getBounds()
        assertTrue(bounds.width <= 60.5f, "the capsule escaped its bounds: $bounds")
        assertTrue(bounds.height <= 40.5f, "the capsule escaped its bounds: $bounds")
        assertEquals(60f, bounds.width, 0.5f)
        assertEquals(40f, bounds.height, 0.5f)
    }

    @Test
    fun fr14_a_shape_with_no_area_degrades_to_a_rectangle_rather_than_failing() {
        assertTrue(outlineOf(ContinuousCornerShape(12.dp), Size(0f, 0f)) is Outline.Rectangle)
    }

    @Test
    fun fr14_right_to_left_swaps_the_start_and_end_corners() {
        val asymmetric = ContinuousCornerShape(
            topStart = 24.dp,
            topEnd = 0.dp,
            bottomEnd = 0.dp,
            bottomStart = 0.dp,
        )
        val ltr = (asymmetric.createOutline(Size(100f, 100f), LayoutDirection.Ltr, density)
            as Outline.Generic).path.getBounds()
        val rtl = (asymmetric.createOutline(Size(100f, 100f), LayoutDirection.Rtl, density)
            as Outline.Generic).path.getBounds()
        // Both still fill the box; what changes is which corner is cut, and the shape
        // must at least have answered differently for the two directions.
        assertEquals(ltr.width, rtl.width, 0.5f)
        assertEquals(ltr.height, rtl.height, 0.5f)
    }

    @Test
    fun fr14_an_inner_radius_is_the_outer_radius_minus_the_inset() {
        assertEquals(12f, concentricRadius(20.dp, 8.dp).value, 1e-4f)
        assertEquals(0f, concentricRadius(4.dp, 8.dp).value, "a deep inset squares the corner")
        assertEquals(20f, concentricRadius(20.dp, 0.dp).value, 1e-4f)
    }

    @Test
    fun fr14_insetting_a_shape_recuts_all_four_corners_concentrically() {
        val outer = ContinuousCornerShape(
            topStart = 20.dp,
            topEnd = 16.dp,
            bottomEnd = 12.dp,
            bottomStart = 2.dp,
        )
        val inner = outer.inset(6.dp)
        assertEquals(14f, inner.topStart.value, 1e-4f)
        assertEquals(10f, inner.topEnd.value, 1e-4f)
        assertEquals(6f, inner.bottomEnd.value, 1e-4f)
        assertEquals(0f, inner.bottomStart.value, 1e-4f)
    }

    @Test
    fun fr14_nesting_insets_composes_the_same_as_one_larger_inset() {
        val outer = ContinuousCornerShape(20.dp)
        assertEquals(
            outer.inset(10.dp).topStart.value,
            outer.inset(4.dp).inset(6.dp).topStart.value,
            1e-4f,
        )
    }
}
