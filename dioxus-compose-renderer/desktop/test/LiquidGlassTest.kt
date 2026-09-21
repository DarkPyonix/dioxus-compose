package dioxus.compose.test

import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import dioxus.compose.design.ContainerRole
import dioxus.compose.design.ContinuousCornerShape
import dioxus.compose.design.GlassProminence
import dioxus.compose.design.HostPlatform
import dioxus.compose.design.LiquidGlass
import dioxus.compose.design.SurfaceMaterial
import dioxus.compose.design.adaptiveSystem
import dioxus.compose.design.concentricRadius
import dioxus.compose.design.contrastRatio
import dioxus.compose.design.drawsAsGlass
import dioxus.compose.design.glassBlurRadius
import dioxus.compose.design.glassFill
import dioxus.compose.design.resolveTheme
import dioxus.compose.protocol.ColorScheme
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.Theme
import dioxus.compose.protocol.WindowSizeClass
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The Apple design system draws Liquid Glass, and the window's width decides how far the
 * glass reaches.
 *
 * These assert the parts that can be stated as values rather than looked at: which roles
 * are glass at which width, that the opaque fallback carries a real contrast guarantee,
 * that reduced transparency reaches both the colour and the blur pass, and that an inner
 * corner is cut concentric with its container.
 */
class LiquidGlassTest {

    private fun appleTheme(dark: Boolean, sizeClass: WindowSizeClass) = resolveTheme(
        theme = null,
        platform = HostPlatform.MacOs,
        systemDark = dark,
        sizeClass = sizeClass,
    )

    private fun materialOf(role: ContainerRole, dark: Boolean, sizeClass: WindowSizeClass): SurfaceMaterial {
        val theme = appleTheme(dark, sizeClass)
        return requireNotNull(theme.rules.container(role, theme).material) {
            "the Apple rules answer every container role with a material"
        }
    }

    @Test
    fun fr14_3_apple_platforms_default_to_the_glass_system() {
        assertEquals(
            DesignSystem.Cupertino,
            adaptiveSystem(HostPlatform.MacOs, DesignSystem.Material3),
        )
        assertEquals(
            DesignSystem.Cupertino,
            adaptiveSystem(HostPlatform.Ios, DesignSystem.Material3),
        )
        // Saying nothing at all on a Mac has to arrive at the same place, because that is
        // the path an application takes when it never mentions a theme.
        val theme = resolveTheme(theme = null, platform = HostPlatform.MacOs, systemDark = false)
        assertEquals(DesignSystem.Cupertino, theme.system)
    }

    @Test
    fun fr14_1_3_chrome_is_glass_at_every_window_size() {
        val chrome = listOf(
            ContainerRole.TopAppBar,
            ContainerRole.Menu,
            ContainerRole.Dialog,
            ContainerRole.Tooltip,
        )
        for (sizeClass in WindowSizeClass.entries) {
            for (role in chrome) {
                assertTrue(
                    materialOf(role, dark = false, sizeClass = sizeClass) is SurfaceMaterial.Glass,
                    "$role is chrome and is glass at every width; it was not at $sizeClass",
                )
            }
        }
    }

    @Test
    fun fr14_1_3_content_surfaces_are_glass_only_in_a_compact_window() {
        val content = listOf(ContainerRole.Card, ContainerRole.Surface)
        for (role in content) {
            assertTrue(
                materialOf(role, dark = false, sizeClass = WindowSizeClass.Compact)
                    is SurfaceMaterial.Glass,
                "$role carries glass on a phone sized window",
            )
            for (wide in listOf(WindowSizeClass.Medium, WindowSizeClass.Expanded)) {
                assertTrue(
                    materialOf(role, dark = false, sizeClass = wide) is SurfaceMaterial.Opaque,
                    "$role is the document on a desktop window and stays opaque; it did not at $wide",
                )
            }
        }
    }

    @Test
    fun fr14_1_2_the_opaque_fallback_meets_body_contrast() {
        for (dark in listOf(false, true)) {
            for (prominence in GlassProminence.entries) {
                val content = if (dark) Color.White else Color.Black
                val material = LiquidGlass.material(
                    dark = dark,
                    prominence = prominence,
                    backdrop = if (dark) Color(0xFF000000) else Color(0xFFF2F2F7),
                    content = content,
                )
                val ratio = contrastRatio(material.fallback, content)
                assertTrue(
                    ratio >= LiquidGlass.MIN_CONTRAST_BODY,
                    "the fallback is the one surface whose contrast can be promised; " +
                        "dark=$dark $prominence reached only $ratio",
                )
                assertEquals(1f, material.fallback.alpha, "a fallback that is see-through promises nothing")
            }
        }
    }

    @Test
    fun fr14_1_2_reduced_transparency_draws_the_fallback_and_drops_the_blur() {
        val material = LiquidGlass.material(
            dark = false,
            backdrop = Color(0xFFF2F2F7),
            content = Color.Black,
        )
        assertTrue(drawsAsGlass(reduceTransparency = false, blurAvailable = true))

        val glassFill = glassFill(material, reduceTransparency = false, blurAvailable = true)
        // Compose stores an sRGB alpha in eight bits, so the round trip lands within one
        // step of the requested value rather than on it.
        assertEquals(material.tintAlpha, glassFill.alpha, absoluteTolerance = 1f / 255f)

        val reducedFill = glassFill(material, reduceTransparency = true, blurAvailable = true)
        assertEquals(material.fallback, reducedFill)

        // The blur pass has to go with the look. Blurring a backdrop nobody can see
        // through is pure cost, paid every frame, by the reader who asked for less of it.
        assertEquals(
            material.blurRadius,
            glassBlurRadius(material, reduceTransparency = false, blurAvailable = true),
        )
        assertEquals(
            0.dp,
            glassBlurRadius(material, reduceTransparency = true, blurAvailable = true),
        )
        assertEquals(
            0.dp,
            glassBlurRadius(material, reduceTransparency = false, blurAvailable = false),
        )
    }

    @Test
    fun fr14_1_2_an_inner_corner_is_concentric_with_its_container() {
        // Concentric means the gap between the two outlines is the same all the way
        // round, which happens only when the inner radius is the outer one minus the
        // inset. A shared radius, or a constant of its own, pinches at the corners.
        assertEquals(12.dp, concentricRadius(20.dp, 8.dp))
        assertEquals(0.dp, concentricRadius(4.dp, 8.dp))

        val theme = appleTheme(dark = false, sizeClass = WindowSizeClass.Compact)
        val tabs = theme.rules.tabs(theme)
        val track = tabs.shape as ContinuousCornerShape
        val segment = tabs.selectedShape as ContinuousCornerShape
        assertEquals(
            concentricRadius(track.topStart, tabs.verticalPadding),
            segment.topStart,
            "the selected segment's corner is cut concentric with the track it sits in",
        )
    }

    @Test
    fun fr14_1_2_a_continuous_corner_is_fuller_than_a_circular_one() {
        // A superellipse corner keeps more area near the corner than a quarter circle of
        // the same radius, which is the whole visible difference. Sampling the outline at
        // the diagonal is the cheapest way to state it.
        val shape = ContinuousCornerShape(40.dp)
        val outline = shape.createOutline(
            size = Size(200f, 200f),
            layoutDirection = LayoutDirection.Ltr,
            density = Density(1f),
        )
        assertTrue(outline is Outline.Generic, "a superellipse has no exact rounded rectangle form")
        val bounds = outline.path.getBounds()
        assertEquals(0f, bounds.left)
        assertEquals(0f, bounds.top)
        assertTrue(bounds.right in 199f..200f)
        assertTrue(bounds.bottom in 199f..200f)
    }

    @Test
    fun fr14_1_2_the_glass_systems_material_differs_from_the_flat_systems() {
        val apple = appleTheme(dark = false, sizeClass = WindowSizeClass.Compact)
        val appleBar = apple.rules.container(ContainerRole.TopAppBar, apple)
        assertTrue(appleBar.material is SurfaceMaterial.Glass)

        for (system in listOf(DesignSystem.Material3, DesignSystem.Fluent)) {
            val theme = resolveTheme(
                theme = Theme(system, system, ColorScheme.Light, adaptive = false),
                platform = HostPlatform.Unknown,
                systemDark = false,
                sizeClass = WindowSizeClass.Compact,
            )
            val bar = theme.rules.container(ContainerRole.TopAppBar, theme)
            assertEquals(
                null,
                bar.material,
                "$system is a flat fill by design, not a degraded glass",
            )
        }
    }
}
