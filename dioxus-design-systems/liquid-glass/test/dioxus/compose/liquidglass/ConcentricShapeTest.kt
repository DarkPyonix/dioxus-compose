package dioxus.compose.liquidglass

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ConcentricShapeTest {

    private val density = Density(2f)
    private val size = Size(200f, 120f)

    private fun outlinePath(
        shape: ContinuousCornerShape,
        layoutDirection: LayoutDirection = LayoutDirection.Ltr,
        size: Size = this.size,
    ): Path {
        val outline = shape.createOutline(size, layoutDirection, density)
        assertTrue(
            outline is Outline.Generic,
            "a continuous corner has no rounded-rectangle form, so the outline has to be " +
                "a path",
        )
        return outline.path
    }

    @Test
    fun fr14_continuous_corner_stays_within_its_bounds() {
        val bounds = outlinePath(ContinuousCornerShape(16.dp)).getBounds()
        assertEquals(0f, bounds.left, 0.5f)
        assertEquals(0f, bounds.top, 0.5f)
        assertEquals(size.width, bounds.right, 0.5f)
        assertEquals(size.height, bounds.bottom, 0.5f)
    }

    @Test
    fun fr14_continuous_corner_is_fuller_than_a_circular_one() {
        // This is the reason the shape exists. A superellipse corner of the same nominal
        // radius keeps material that a circular arc cuts away, which is what removes the
        // pinched look at the corners. If the exponent were ever dropped back to 2 the
        // difference would vanish and nothing else here would notice.
        val radius = 24.dp
        val continuous = outlinePath(ContinuousCornerShape(radius))
        val circular = Path().apply {
            val outline = RoundedCornerShape(radius)
                .createOutline(size, LayoutDirection.Ltr, density)
            addOutline(outline)
        }

        val extra = Path().apply { op(continuous, circular, PathOperation.Difference) }
        assertTrue(
            !extra.isEmpty,
            "the continuous corner covers no area the circular one does not, so it is a " +
                "circle by another name",
        )
    }

    @Test
    fun fr14_radii_are_scaled_together_when_they_do_not_fit() {
        // Two radii larger than the side they share would make the outline cross itself.
        // Scaling both keeps the shape proportional; the outline must still fill its box.
        val oversized = ContinuousCornerShape(500.dp)
        val bounds = outlinePath(oversized).getBounds()
        assertEquals(size.width, bounds.width, 0.5f)
        assertEquals(size.height, bounds.height, 0.5f)
    }

    @Test
    fun fr14_capsule_is_the_largest_radius_the_bounds_allow() {
        val bounds = outlinePath(CapsuleShape).getBounds()
        assertEquals(size.width, bounds.width, 0.5f)
        assertEquals(size.height, bounds.height, 0.5f)
    }

    @Test
    fun fr14_right_to_left_mirrors_start_and_end_radii() {
        // The radii are named by writing order, so the same shape has to put the large
        // corner on the other side when the layout direction flips.
        val lopsided = ContinuousCornerShape(
            topStart = 40.dp, topEnd = 0.dp, bottomEnd = 0.dp, bottomStart = 0.dp,
        )
        val ltr = outlinePath(lopsided, LayoutDirection.Ltr)
        val rtl = outlinePath(lopsided, LayoutDirection.Rtl)
        val difference = Path().apply { op(ltr, rtl, PathOperation.Difference) }
        assertTrue(!difference.isEmpty, "the outline did not mirror")
    }

    @Test
    fun fr14_zero_radius_is_a_rectangle_and_not_a_special_case() {
        val bounds = outlinePath(ContinuousCornerShape(0.dp)).getBounds()
        assertEquals(size.width, bounds.width, 0.01f)
        assertEquals(size.height, bounds.height, 0.01f)
    }

    @Test
    fun fr14_concentric_inner_radius_is_the_outer_radius_less_the_inset() {
        // Two rounded rectangles are concentric only when the gap between them is the same
        // through the corner as along the edge, and that holds exactly when the inner
        // radius is the outer one minus the inset.
        assertEquals(12.dp, concentricRadius(20.dp, 8.dp))
        assertEquals(0.dp, concentricRadius(20.dp, 20.dp))
        // An element inset further than the radius has a square corner rather than a
        // negative one.
        assertEquals(0.dp, concentricRadius(8.dp, 20.dp))
    }

    @Test
    fun fr14_inset_recuts_every_corner() {
        val outer = ContinuousCornerShape(
            topStart = 20.dp, topEnd = 16.dp, bottomEnd = 12.dp, bottomStart = 4.dp,
        )
        val inner = outer.inset(6.dp)
        assertEquals(ContinuousCornerShape(14.dp, 10.dp, 6.dp, 0.dp), inner)
    }
}
