package dioxus.compose.cupertino

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import dioxus.compose.ButtonVariant
import dioxus.compose.ColorRole
import dioxus.compose.DesignSystem
import dioxus.compose.DesignSystemId
import dioxus.compose.ShapeRole
import dioxus.compose.SpaceRole
import dioxus.compose.TypeRole
import dioxus.compose.breeze.BreezeDesignSystem
import dioxus.compose.deepin.DeepinDesignSystem
import dioxus.compose.fluent.FluentDesignSystem
import dioxus.compose.gnome.GnomeDesignSystem
import dioxus.compose.material3.Material3DesignSystem
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue

/**
 * All six systems, asked the same questions side by side.
 *
 * The other cross system tests cover a pair and a trio: Material against Fluent, and the
 * three Linux systems against each other. Neither of them ever asks Cupertino anything,
 * so until this existed a design system could have collapsed onto another one's values in
 * five of the fifteen pairings and every test in the project would still have passed.
 *
 * What is asserted here is that no two systems answer a whole family of roles the same
 * way. It is deliberately not "no two systems ever agree on any single role", because
 * they legitimately do and should: a square corner is a square corner, white is the only
 * sensible label on half of these accent colours, and Material and Deepin both happen to
 * round a medium container at twelve. Those coincidences are not the failure mode. The
 * failure mode is a system that is another system with the numbers nudged, and a system
 * that answers an entire ladder identically to its neighbour is exactly that.
 */
class AllDesignSystemsDifferTest {

    private fun systems(dark: Boolean): List<DesignSystem> = listOf(
        if (dark) Material3DesignSystem.Dark else Material3DesignSystem.Light,
        if (dark) CupertinoDesignSystem.Dark else CupertinoDesignSystem.Light,
        if (dark) FluentDesignSystem.Dark else FluentDesignSystem.Light,
        GnomeDesignSystem.of(dark),
        BreezeDesignSystem.of(dark),
        DeepinDesignSystem.of(dark),
    )

    private fun eachPair(dark: Boolean, body: (DesignSystem, DesignSystem) -> Unit) {
        val all = systems(dark)
        for (i in all.indices) {
            for (j in i + 1 until all.size) body(all[i], all[j])
        }
    }

    private fun bothSchemes(body: (DesignSystem, DesignSystem) -> Unit) {
        for (dark in listOf(false, true)) eachPair(dark, body)
    }

    @Test
    fun fr14_there_are_six_systems_and_each_reports_its_own_identity() {
        for (dark in listOf(false, true)) {
            val ids = systems(dark).map { it.id }
            assertEquals(DesignSystemId.entries.size, ids.size)
            assertEquals(ids.size, ids.toSet().size, "two systems report the same id: $ids")
            assertEquals(DesignSystemId.entries.toSet(), ids.toSet())
        }
    }

    @Test
    fun fr14_every_system_resolves_every_role_in_both_schemes() {
        for (dark in listOf(false, true)) {
            for (system in systems(dark)) {
                assertEquals(dark, system.isDark, "${system.id} disagrees about its scheme")
                for (role in ColorRole.entries) {
                    assertNotEquals(
                        Color.Unspecified,
                        system.color(role),
                        "${system.id} has no answer for $role",
                    )
                }
                for (role in TypeRole.entries) {
                    assertTrue(
                        system.type(role).fontSize.value > 0f,
                        "${system.id} sets $role at no size",
                    )
                }
                for (role in ShapeRole.entries) system.shape(role)
                for (role in SpaceRole.entries) {
                    assertTrue(system.space(role).value >= 0f, "${system.id} pads $role negatively")
                }
                for (variant in ButtonVariant.entries) system.button(variant)
                assertTrue(system.motion.pressMillis > 0, "${system.id} presses instantly")
            }
        }
    }

    @Test
    fun fr14_no_two_systems_share_an_accent() {
        // The accent is the single most identity bearing colour a system has: it is the
        // one a reader names when asked what the screen looks like. Unlike the label and
        // outline roles, there is no excuse for two systems landing on the same one.
        bothSchemes { a, b ->
            assertNotEquals(
                a.color(ColorRole.Primary),
                b.color(ColorRole.Primary),
                "${a.id} and ${b.id} use the same accent",
            )
        }
    }

    @Test
    fun fr13_no_two_systems_share_a_colour_table() {
        bothSchemes { a, b ->
            assertNotEquals(
                ColorRole.entries.map(a::color),
                ColorRole.entries.map(b::color),
                "${a.id} and ${b.id} are one palette under two names",
            )
        }
    }

    @Test
    fun fr13_no_two_systems_share_a_type_ladder() {
        bothSchemes { a, b ->
            assertNotEquals(
                TypeRole.entries.map(a::type),
                TypeRole.entries.map(b::type),
                "${a.id} and ${b.id} set every step of the ladder identically",
            )
        }
    }

    @Test
    fun fr13_no_two_systems_share_a_corner_ladder() {
        bothSchemes { a, b ->
            assertNotEquals(
                ShapeRole.entries.map(a::shape),
                ShapeRole.entries.map(b::shape),
                "${a.id} and ${b.id} round every step identically",
            )
        }
    }

    @Test
    fun fr13_no_two_systems_share_a_spacing_ladder() {
        bothSchemes { a, b ->
            assertNotEquals(
                SpaceRole.entries.map(a::space),
                SpaceRole.entries.map(b::space),
                "${a.id} and ${b.id} have the same density at every step",
            )
        }
    }

    @Test
    fun fr14_no_two_systems_paint_the_same_set_of_buttons() {
        bothSchemes { a, b ->
            assertNotEquals(
                ButtonVariant.entries.map(a::button),
                ButtonVariant.entries.map(b::button),
                "${a.id} and ${b.id} draw all four button variants the same way",
            )
        }
    }

    @Test
    fun fr14_no_two_systems_raise_a_surface_the_same_way() {
        val base = Color(0xFF808080)
        bothSchemes { a, b ->
            assertNotEquals(
                listOf(2, 8, 24).map { a.elevation(it.dp, base) },
                listOf(2, 8, 24).map { b.elevation(it.dp, base) },
                "${a.id} and ${b.id} treat height identically, so depth reads the same",
            )
        }
    }

    @Test
    fun fr14_only_cupertino_answers_with_a_material_rather_than_a_fill() {
        // Glass is Cupertino's design language, not a richer option the others declined.
        // A Material 3 card that came back as glass would be wrong rather than fancier,
        // which is why this is asserted in both directions.
        for (dark in listOf(false, true)) {
            for (system in systems(dark)) {
                val glassRoles = ColorRole.entries.filter {
                    system.material(it) is dioxus.compose.SurfaceMaterial.Glass
                }
                if (system.id == DesignSystemId.Cupertino) {
                    assertTrue(
                        glassRoles.isNotEmpty(),
                        "Cupertino stopped answering with Liquid Glass anywhere",
                    )
                } else {
                    assertTrue(
                        glassRoles.isEmpty(),
                        "${system.id} answers $glassRoles with glass, which is not its language",
                    )
                }
            }
        }
    }

    @Test
    fun fr14_material_is_the_only_system_that_ripples() {
        // Ripple is a Material behaviour. Every other system here dims, tints or
        // recolours a stroke instead, and a ripple appearing in one of them would be a
        // visible mistake rather than a detail.
        for (dark in listOf(false, true)) {
            for (system in systems(dark)) {
                val ripples = ButtonVariant.entries.any { system.button(it).ripple }
                assertEquals(
                    system.id == DesignSystemId.Material3,
                    ripples,
                    "${system.id} ripples: ${system.id == DesignSystemId.Material3} expected",
                )
            }
        }
    }
}
