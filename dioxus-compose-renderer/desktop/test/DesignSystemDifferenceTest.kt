package dioxus.compose.test

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.runComposeUiTest
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.pow
import kotlin.test.Test
import kotlin.test.assertTrue
import dioxus.compose.design.HostPlatform
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.design.resolveTheme
import dioxus.compose.protocol.ColorRole
import dioxus.compose.protocol.ColorScheme
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.Theme
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.ui.node.nodeTestTag

private fun resolved(system: DesignSystem, dark: Boolean): ResolvedTheme = resolveTheme(
    Theme(system, system, if (dark) ColorScheme.Dark else ColorScheme.Light, false),
    HostPlatform.MacOs,
    systemDark = false,
)

/** Both schemes of one system, which is how a colour answer has to be checked. */
private fun bothSchemes(system: DesignSystem): List<ResolvedTheme> =
    listOf(resolved(system, dark = false), resolved(system, dark = true))

/** WCAG relative luminance, 0 for black and 1 for white. */
private fun luminance(color: Color): Float {
    fun channel(value: Float): Float =
        if (value <= 0.03928f) value / 12.92f else ((value + 0.055f) / 1.055f).pow(2.4f)
    return 0.2126f * channel(color.red) + 0.7152f * channel(color.green) + 0.0722f * channel(color.blue)
}

/**
 * How far apart two colours are on their widest channel, in the 0 to 255 the values were
 * written as.
 *
 * A ratio would say two dark greys are alike when a reader can see the edge between them
 * perfectly well; the widest channel is closer to what the eye picks up on small shapes
 * like a thumb against a page.
 */
private fun apart(first: Color, second: Color): Float = max(
    max(abs(first.red - second.red), abs(first.green - second.green)),
    abs(first.blue - second.blue),
) * 255f

/** The corner this shape would round a square of [size] to, in dp. */
private fun cornerRadius(shape: Shape, size: Dp): Float {
    val rounded = shape as? RoundedCornerShape ?: return 0f
    return rounded.topStart.toPx(Size(size.value, size.value), Density(1f))
}

/**
 * The places where one design system's answer has to stay its own, and where an answer
 * that is merely plausible turns out to be invisible on screen.
 *
 * Three of these systems were written from a description rather than from a desktop, and
 * a wrong colour here does not fail anything else in the build: a white thumb on a white
 * page is drawn full size, in the right place, and cannot be seen.
 */
class DesignSystemDifferenceTest {

    /**
     * A checkbox is drawn as a box, except in the one system that draws a circle.
     *
     * Cupertino is named rather than skipped. A round checkbox is Apple's own answer and
     * the difference from its radio button is the mark inside, not the outline. Everywhere
     * else a corner that grows past a third of the control stops reading as a box at all,
     * and a reader is left with two circles that differ only in what is drawn inside them.
     */
    @Test
    fun fr14_1_a_checkbox_is_a_box_except_where_the_system_draws_a_circle() {
        DesignSystem.entries.forEach { system ->
            val theme = resolved(system, dark = false)
            val checkbox = theme.rules.controls(theme).checkbox
            val radius = cornerRadius(checkbox.shape, checkbox.size)
            if (system == DesignSystem.Cupertino) {
                assertTrue(
                    radius >= checkbox.size.value / 2f - 0.5f,
                    "Cupertino's checkbox is a circle: ${checkbox.size} rounded at $radius",
                )
            } else {
                assertTrue(
                    radius <= checkbox.size.value / 3f,
                    "$system rounds a ${checkbox.size} checkbox by $radius, which is round " +
                        "enough to be mistaken for its own radio button",
                )
            }
        }
    }

    /**
     * GNOME, KDE and Deepin all draw a light switch handle, whichever way the switch is
     * thrown and whichever scheme is in effect.
     *
     * None of the three recolours the handle when the switch turns on: what changes is the
     * track behind it. A handle taken from the ink colour of whatever is under it comes out
     * dark grey when the switch is off and near black on KDE's blue when it is on, which is
     * a control nobody on those desktops would recognise.
     */
    @Test
    fun fr14_1_the_linux_switches_keep_a_light_handle() {
        listOf(DesignSystem.Gnome, DesignSystem.Breeze, DesignSystem.Deepin).forEach { system ->
            bothSchemes(system).forEach { theme ->
                val switch = theme.rules.controls(theme).switch
                listOf("on" to switch.mark, "off" to switch.markUnchecked).forEach { (state, handle) ->
                    assertTrue(
                        luminance(handle) >= LIGHT_HANDLE_LUMINANCE,
                        "$system draws a dark handle on a switch that is $state " +
                            "(dark scheme: ${theme.dark})",
                    )
                }
            }
        }
    }

    /**
     * A slider's thumb can be seen against the page it sits on, in every system and both
     * schemes.
     *
     * A thumb filled with the reading surface and given no ring is the failure this whole
     * set of tables exists to catch: it is drawn, at the right size, in the right place,
     * and there is nothing on screen. A system may earn the contrast with the fill or with
     * a ring around it, and which of the two is its own decision.
     */
    @Test
    fun fr14_1_a_slider_thumb_can_be_seen_against_the_page() {
        DesignSystem.entries.forEach { system ->
            bothSchemes(system).forEach { theme ->
                val slider = theme.rules.controls(theme).slider
                val page = theme.color(ColorRole.Surface)
                val byFill = apart(slider.thumb, page)
                val byRing = if (slider.thumbBorderWidth.value > 0f) {
                    apart(slider.thumbBorder, page)
                } else {
                    0f
                }
                assertTrue(
                    max(byFill, byRing) >= VISIBLE_APART,
                    "$system (dark scheme: ${theme.dark}) draws a slider thumb $byFill from " +
                        "the page it sits on, with a ring $byRing from it",
                )
            }
        }
    }

    /**
     * A control that asks for no line around it is drawn without one.
     *
     * Compose reads a zero width border as a hairline and draws a one pixel line anyway,
     * so the two systems that say a switch has no outline were both getting a dark ring
     * around the track. Cupertino is the case here because its switch is the plainest of
     * the six: a pale track, a white knob and nothing else, so any line in the picture is
     * a line nobody asked for.
     */
    @OptIn(ExperimentalTestApi::class)
    @Test
    fun fr14_1_a_control_that_asks_for_no_line_is_drawn_without_one() {
        val system = DesignSystem.Cupertino
        val theme = resolved(system, dark = false)
        assertTrue(
            theme.rules.controls(theme).switch.borderWidth.value == 0f,
            "this test only means anything while $system asks for no line",
        )
        runComposeUiTest {
            setContent {
                DioxusContent(
                    rememberDioxusHost(
                        FakeHostConnection(
                            listOf(
                                Mutation.SetTheme(
                                    Theme(system, system, ColorScheme.Light, false),
                                ),
                                Mutation.Create(SWITCH, WidgetKind.Switch),
                                Mutation.SetProp(
                                    SWITCH,
                                    PropertyKind.Checked,
                                    PropertyValue.Bool(false),
                                ),
                            ),
                        ),
                    ),
                )
            }
            waitForIdle()
            val pixels = onNodeWithTag(nodeTestTag(SWITCH)).captureToImage().toPixelMap()
            var darkest = 1f
            for (y in 0 until pixels.height) {
                for (x in 0 until pixels.width) {
                    darkest = minOf(darkest, luminance(pixels[x, y]))
                }
            }
            assertTrue(
                darkest >= UNLINED_LUMINANCE,
                "$system's switch has a line around it: the darkest pixel in it is " +
                    "$darkest, and its palest colour is the track",
            )
        }
    }
}

/** The node the border test draws. */
private const val SWITCH = 1

/**
 * How light a switch handle has to be to read as the white knob those desktops draw.
 *
 * Far enough above the middle that a mid grey fails, low enough that a warm off white
 * passes.
 */
private const val LIGHT_HANDLE_LUMINANCE = 0.55f

/** How far apart two colours have to be before the edge between them is worth drawing. */
private const val VISIBLE_APART = 24f

/**
 * How dark a pixel of an unlined control is allowed to get.
 *
 * Above the darkest colour such a control is made of, and far below the near black a
 * one pixel line comes out as.
 */
private const val UNLINED_LUMINANCE = 0.5f
