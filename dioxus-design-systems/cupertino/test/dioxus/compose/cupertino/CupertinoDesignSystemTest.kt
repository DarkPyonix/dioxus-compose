package dioxus.compose.cupertino

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.unit.dp
import dioxus.compose.ButtonVariant
import dioxus.compose.ColorRole
import dioxus.compose.DesignSystemId
import dioxus.compose.ShapeRole
import dioxus.compose.SpaceRole
import dioxus.compose.SurfaceMaterial
import dioxus.compose.TypeRole
import dioxus.compose.liquidglass.ContinuousCornerShape
import dioxus.compose.liquidglass.LiquidGlass
import dioxus.compose.liquidglass.contrastRatio
import dioxus.compose.liquidglass.relativeLuminance
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class CupertinoDesignSystemTest {

    private val light = CupertinoDesignSystem.Light
    private val dark = CupertinoDesignSystem.Dark

    @Test
    fun fr14_every_role_answers_in_both_schemes() {
        for (system in listOf(light, dark)) {
            assertEquals(DesignSystemId.Cupertino, system.id)

            for (role in ColorRole.entries) {
                val color = system.color(role)
                assertEquals(
                    1f, color.alpha,
                    "$role in ${system.isDark} returned a translucent colour; a role has " +
                        "to answer with something drawable on its own",
                )
            }
            for (role in TypeRole.entries) {
                val style = system.type(role)
                assertTrue(style.fontSize.value > 0f, "$role has no size")
                assertTrue(style.lineHeight.value >= style.fontSize.value, "$role leads tight")
                assertTrue(style.fontWeight != null, "$role has no weight")
            }
            for (role in ShapeRole.entries) {
                system.shape(role)
            }
            for (role in SpaceRole.entries) {
                assertTrue(system.space(role).value >= 0f, "$role is negative")
            }
            for (variant in ButtonVariant.entries) {
                system.button(variant)
            }
        }
        assertFalse(light.isDark)
        assertTrue(dark.isDark)
    }

    @Test
    fun fr14_light_and_dark_are_different_palettes_rather_than_one_inverted() {
        // Several Apple system colours move in ways a lightness transform would not
        // produce, so the two schemes have to disagree on more than polarity.
        for (role in ColorRole.entries) {
            assertNotEquals(
                light.color(role), dark.color(role),
                "$role is the same colour in both schemes",
            )
        }
        // The page is near white in one and near black in the other.
        assertTrue(relativeLuminance(light.color(ColorRole.Background)) > 0.9f)
        assertTrue(relativeLuminance(dark.color(ColorRole.Background)) < 0.05f)
        // Apple's accent brightens in the dark rather than darkening, which is the case a
        // naive inversion gets backwards.
        assertTrue(
            relativeLuminance(dark.color(ColorRole.Primary)) >
                relativeLuminance(light.color(ColorRole.Primary)),
        )
    }

    @Test
    fun fr14_text_carrying_surfaces_meet_body_text_contrast() {
        val pairs = listOf(
            ColorRole.Surface to ColorRole.OnSurface,
            ColorRole.Background to ColorRole.OnBackground,
            ColorRole.SurfaceVariant to ColorRole.OnSurfaceVariant,
        )
        for (system in listOf(light, dark)) {
            for ((surface, content) in pairs) {
                val ratio = contrastRatio(system.color(surface), system.color(content))
                assertTrue(
                    ratio >= 4.5f,
                    "$content on $surface (dark=${system.isDark}) is $ratio to 1, below " +
                        "the 4.5 to 1 that body text needs",
                )
            }
        }
    }

    @Test
    fun fr14_accent_containers_meet_the_threshold_for_interface_components() {
        // Apple's own systemBlue with a white label is about 4 to 1, and systemRed is
        // lower still. Holding these to 4.5 to 1 would mean not shipping Apple's colours,
        // which is the one thing this design system cannot do. They are held instead to
        // the 3 to 1 that applies to interface components and to the large, heavy text a
        // filled control actually carries, and an application that needs 4.5 to 1
        // throughout should pass a content colour of its own rather than have this file
        // quietly repaint the platform.
        val pairs = listOf(
            ColorRole.Primary to ColorRole.OnPrimary,
            ColorRole.Secondary to ColorRole.OnSecondary,
            ColorRole.Error to ColorRole.OnError,
        )
        for (system in listOf(light, dark)) {
            for ((container, content) in pairs) {
                val ratio = contrastRatio(system.color(container), system.color(content))
                assertTrue(
                    ratio >= 3f,
                    "$content on $container (dark=${system.isDark}) is $ratio to 1",
                )
            }
        }
    }

    @Test
    fun fr14_corners_are_continuous_rather_than_circular() {
        // A circular corner would be the single clearest sign that this had become a
        // Material theme with an Apple palette on it.
        for (system in listOf(light, dark)) {
            for (role in ShapeRole.entries) {
                val shape = system.shape(role)
                if (role == ShapeRole.None) {
                    assertEquals(RectangleShape, shape, "a None corner is square")
                } else {
                    assertTrue(
                        shape is ContinuousCornerShape,
                        "$role is ${shape::class.simpleName}, not a continuous corner",
                    )
                }
            }
        }
    }

    @Test
    fun fr14_no_button_variant_ripples() {
        // A ripple is Material's press, not Apple's. Every variant dims instead, and the
        // dim has to be visible: a pressed state equal to the resting one is the same bug
        // as having no pressed state at all.
        for (system in listOf(light, dark)) {
            for (variant in ButtonVariant.entries) {
                val style = system.button(variant)
                assertFalse(style.ripple, "$variant (dark=${system.isDark}) ripples")

                val changed = style.pressedContainer != style.container ||
                    style.pressedContent != style.content ||
                    style.pressedBorder != style.border
                assertTrue(
                    changed,
                    "$variant (dark=${system.isDark}) looks identical while held",
                )
                assertEquals(
                    ShapeRole.Full, style.shape,
                    "Apple's buttons are capsules",
                )
            }
        }
    }

    @Test
    fun fr14_a_press_moves_a_filled_container_away_from_the_page() {
        // Darkening in both schemes would make a dark-scheme press nearly invisible
        // against a black page, so the direction has to follow the scheme.
        val lightFilled = light.button(ButtonVariant.Filled)
        assertTrue(
            relativeLuminance(lightFilled.pressedContainer) <
                relativeLuminance(lightFilled.container),
        )
        val darkFilled = dark.button(ButtonVariant.Filled)
        assertTrue(
            relativeLuminance(darkFilled.pressedContainer) >
                relativeLuminance(darkFilled.container),
        )
    }

    @Test
    fun fr14_elevation_casts_a_shadow_without_tinting_the_surface() {
        // Material encodes height as hue. Apple does not, and doing it here would look
        // like Material with Apple's blue mixed in.
        for (system in listOf(light, dark)) {
            val base = system.color(ColorRole.Surface)
            val raised = system.elevation(8.dp, base)
            assertEquals(base, raised.surface, "the surface was tinted by being raised")
            assertTrue(raised.shadowElevation > 8.dp, "an Apple shadow spreads wider than its height")
            assertTrue(raised.shadowColor.alpha > 0f)
            assertNull(raised.strokeTop, "a top stroke is Fluent's device, not Apple's")

            assertEquals(base, system.elevation(0.dp, base).surface)
            assertEquals(0.dp, system.elevation(0.dp, base).shadowElevation)
        }
    }

    @Test
    fun fr14_surfaces_are_glass_and_ink_is_not() {
        for (system in listOf(light, dark)) {
            val surface = system.material(ColorRole.Surface)
            assertTrue(
                surface is SurfaceMaterial.Glass,
                "the Surface role has to answer with the material, not a flat fill",
            )
            assertTrue(surface.tintAlpha < 1f)
            assertTrue(surface.blurRadius > 0.dp)

            // The page is the bottom of the stack: glass with nothing behind it is a flat
            // fill drawn the expensive way.
            assertTrue(system.material(ColorRole.Background) is SurfaceMaterial.Opaque)
            assertTrue(system.material(ColorRole.OnSurface) is SurfaceMaterial.Opaque)
            assertTrue(system.material(ColorRole.Primary) is SurfaceMaterial.Opaque)
        }
    }

    @Test
    fun fr14_the_glass_fallback_stays_readable_in_both_schemes() {
        // This is the promise that has to survive a reader turning transparency off, and
        // the only contrast claim a translucent surface can honestly make.
        for (system in listOf(light, dark)) {
            for (role in listOf(ColorRole.Surface, ColorRole.SurfaceVariant)) {
                val glass = system.material(role) as SurfaceMaterial.Glass
                val content = system.color(
                    if (role == ColorRole.Surface) ColorRole.OnSurface
                    else ColorRole.OnSurfaceVariant,
                )
                val ratio = contrastRatio(glass.fallback, content)
                assertTrue(
                    ratio >= LiquidGlass.MIN_CONTRAST_BODY,
                    "$role fallback ${glass.fallback} against $content (dark=" +
                        "${system.isDark}) is $ratio to 1",
                )
                assertEquals(1f, glass.fallback.alpha)
            }
        }
    }

    @Test
    fun fr14_type_scale_rises_and_keeps_apple_optical_tracking() {
        val ladder = listOf(
            TypeRole.Caption, TypeRole.Label, TypeRole.Body,
            TypeRole.Subtitle, TypeRole.Title, TypeRole.Headline, TypeRole.Display,
        )
        for (i in 1 until ladder.size) {
            val smaller = light.type(ladder[i - 1]).fontSize.value
            val larger = light.type(ladder[i]).fontSize.value
            assertTrue(
                larger > smaller,
                "${ladder[i]} at $larger is not above ${ladder[i - 1]} at $smaller",
            )
        }
        // Emphasis inside a paragraph must not reflow it, so the strong body differs only
        // in weight.
        val body = light.type(TypeRole.Body)
        val strong = light.type(TypeRole.BodyStrong)
        assertEquals(body.fontSize, strong.fontSize)
        assertEquals(body.lineHeight, strong.lineHeight)
        assertTrue(strong.fontWeight!!.weight > body.fontWeight!!.weight)

        // San Francisco tightens as it grows: tracking is negative at reading sizes and
        // positive at display sizes.
        assertTrue(light.type(TypeRole.Body).letterSpacing.value < 0f)
        assertTrue(light.type(TypeRole.Display).letterSpacing.value > 0f)
    }

    @Test
    fun fr14_both_schemes_are_shared_instances_and_compare_equal() {
        // The design system sits in a static composition local, so an instance that is
        // never equal to itself would invalidate the whole tree on every read.
        assertEquals(light, CupertinoDesignSystem.of(dark = false))
        assertEquals(dark, CupertinoDesignSystem.of(dark = true))
        assertNotEquals(light, dark)
        assertEquals(light.hashCode(), CupertinoDesignSystem.of(dark = false).hashCode())
    }

    @Test
    fun fr14_motion_recovers_more_slowly_than_it_depresses() {
        for (system in listOf(light, dark)) {
            assertTrue(system.motion.pressMillis > 0)
            assertTrue(
                system.motion.releaseMillis > system.motion.pressMillis,
                "a control that recovers as fast as it depresses feels brittle",
            )
        }
    }

    @Test
    fun fr14_colors_are_the_published_apple_system_values() {
        // Spot checks against Apple's documented sRGB values, so a well-meaning tweak
        // towards a prettier blue fails rather than ships.
        assertEquals(Color(0xFF007AFF), light.color(ColorRole.Primary))
        assertEquals(Color(0xFF0A84FF), dark.color(ColorRole.Primary))
        assertEquals(Color(0xFFFF3B30), light.color(ColorRole.Error))
        assertEquals(Color(0xFFFF453A), dark.color(ColorRole.Error))
        assertEquals(Color(0xFF1C1C1E), dark.color(ColorRole.Surface))
        assertEquals(Color(0xFFF2F2F7), light.color(ColorRole.Surface))
    }
}
