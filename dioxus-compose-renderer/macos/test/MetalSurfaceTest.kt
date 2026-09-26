@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

package dioxus.compose.ui.platform

import kotlinx.cinterop.useContents
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * The layer this window draws into, and the two things about it that a window's edge
 * depends on.
 *
 * These are assertions about what Core Animation actually holds, not about what the code
 * meant to ask for, because both of the settings below are ones a layer has by default in
 * the wrong state and both are invisible from inside the process when they are wrong.
 *
 * What no test here can show is the screen. Whether the drawing and the window's frame
 * arrive together is a fact about what the display server composited, and it is checked by
 * painting the window's background a colour nothing else on the screen is, dragging an edge
 * at a known speed, and counting how many pixels of that colour a capture of the screen
 * holds. Measured that way on 2026-09-25: with a layer that presented on its own schedule
 * the strip averaged 3 pixels at 640 px/s and reached 350 at 28,845 px/s, which in each case
 * is the speed of the hand times one refresh of the screen; with the layer below it was zero
 * at every speed.
 */
class MetalSurfaceTest {

    @Test
    fun nfr9_the_layer_presents_with_the_transaction() {
        val surface = MetalSurface()
        try {
            assertTrue(
                surface.layer.presentsWithTransaction,
                "a layer that presents on its own schedule puts what was drawn on the " +
                    "screen after the window's frame has already moved, which a hand " +
                    "dragging an edge sees as the drawing coming away from the pointer",
            )
        } finally {
            surface.close()
        }
    }

    @Test
    fun nfr9_the_layer_is_sized_in_pixels_rather_than_points() {
        val surface = MetalSurface()
        try {
            surface.resize(widthInPoints = 520.0, heightInPoints = 360.0, scale = 2.0)
            assertEquals(2.0, surface.layer.contentsScale)
            surface.layer.drawableSize.useContents {
                assertEquals(1040.0, width, "a drawable measured in points is half a window")
                assertEquals(720.0, height)
            }
        } finally {
            surface.close()
        }
    }

    @Test
    fun the_layer_lets_skia_draw_into_its_texture() {
        val surface = MetalSurface()
        try {
            assertFalse(
                surface.layer.framebufferOnly,
                "a framebuffer-only texture cannot be read back, and Skia reads one back",
            )
        } finally {
            surface.close()
        }
    }
}
