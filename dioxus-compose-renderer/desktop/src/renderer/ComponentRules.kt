package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.thisisthepy.dioxus.compose.protocol.ButtonVariant
import org.thisisthepy.dioxus.compose.protocol.ColorRole
import org.thisisthepy.dioxus.compose.protocol.ShapeRole
import org.thisisthepy.dioxus.compose.protocol.SpaceRole
import org.thisisthepy.dioxus.compose.protocol.TypeRole

/**
 * Material 3 rules (FR-14.6 items 5 to 7).
 *
 * Reference: m3.material.io, "Elevation", "Buttons" and "Motion easing and duration",
 * 2024 baseline, the same revision the generated token table cites.
 *
 * Elevation is tonal plus a shadow, buttons are fully rounded with a state layer that grows
 * on press, and motion uses the emphasised easing.
 */
internal object Material3Rules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // Material 3 raises the surface tone with height as well as casting a shadow. The
        // tint is drawn first so a node that also sets its own Background still wins.
        val tint = lerp(
            theme.color(ColorRole.Surface),
            theme.color(ColorRole.Primary),
            (elevation.value / TONE_FULL_DP).coerceIn(0f, MAX_TONE),
        )
        return modifier.shadow(elevation, shape).background(tint, shape)
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val outline = theme.color(ColorRole.Outline)
        val base = ButtonStyle(
            container = Color.Transparent,
            pressedContainer = Color.Transparent,
            content = theme.color(ColorRole.Primary),
            // The state layer, not opacity, is Material's press feedback, so the label
            // keeps its full opacity.
            pressedContentAlpha = 1f,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            pressedBorderColor = Color.Transparent,
            topHighlight = null,
            // Material 3 buttons are pills.
            shape = theme.shape(ShapeRole.Full),
            horizontalPadding = theme.space(SpaceRole.Lg),
            verticalPadding = theme.space(SpaceRole.Sm) + 2.dp,
            minHeight = 40.dp,
            typeRole = TypeRole.Label,
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            ButtonVariant.Filled -> base.copy(
                container = theme.color(ColorRole.Primary),
                pressedContainer = stateLayer(theme.color(ColorRole.Primary), theme.color(ColorRole.OnPrimary)),
                content = theme.color(ColorRole.OnPrimary),
                // A pressed filled button rises: Material's feedback is tonal and spatial.
                pressedElevation = 2.dp,
            )

            ButtonVariant.Tonal -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                pressedContainer = stateLayer(
                    theme.color(ColorRole.SurfaceVariant),
                    theme.color(ColorRole.OnSurfaceVariant),
                ),
                content = theme.color(ColorRole.OnSurfaceVariant),
                pressedElevation = 1.dp,
            )

            ButtonVariant.Outlined -> base.copy(
                pressedContainer = stateLayer(Color.Transparent, theme.color(ColorRole.Primary)),
                borderWidth = 1.dp,
                borderColor = outline,
                pressedBorderColor = outline,
            )

            ButtonVariant.Text -> base.copy(
                pressedContainer = stateLayer(Color.Transparent, theme.color(ColorRole.Primary)),
            )
        }
    }

    /** Material's pressed state layer: the content colour at 12% over the container. */
    private fun stateLayer(container: Color, content: Color): Color =
        lerp(container, content.copy(alpha = 1f), STATE_LAYER_ALPHA)
            .copy(alpha = if (container.alpha == 0f) STATE_LAYER_ALPHA else container.alpha)

    override val motion: Motion = Motion(
        pressMillis = 100,
        releaseMillis = 200,
        easing = FastOutSlowInEasing,
    )

    private const val STATE_LAYER_ALPHA = 0.12f
    private const val TONE_FULL_DP = 24f
    private const val MAX_TONE = 0.14f
}

/**
 * Cupertino rules (FR-14.6 items 5 to 7).
 *
 * Reference: Apple Human Interface Guidelines, "Materials", "Buttons" and "Motion", 2024.
 *
 * Shadows are wide and faint rather than layered, the emphasised button is flat with no
 * shadow at all (FR-14.2), and the press feedback is a dim, not a ripple.
 */
internal object CupertinoRules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // One soft shadow spread over roughly twice the requested height, at a low alpha.
        return modifier.shadow(
            elevation = elevation * SPREAD,
            shape = shape,
            clip = false,
            ambientColor = Color.Black.copy(alpha = AMBIENT_ALPHA),
            spotColor = Color.Black.copy(alpha = SPOT_ALPHA),
        )
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val base = ButtonStyle(
            container = Color.Transparent,
            pressedContainer = Color.Transparent,
            content = theme.color(ColorRole.Primary),
            // HIG presses dim the whole control instead of layering a colour on it.
            pressedContentAlpha = PRESSED_ALPHA,
            borderWidth = 0.dp,
            borderColor = Color.Transparent,
            pressedBorderColor = Color.Transparent,
            topHighlight = null,
            // The continuous curvature of a HIG capsule is approximated by the Medium
            // radius; see the note in the Renderer README about squircle support.
            shape = theme.shape(ShapeRole.Medium),
            horizontalPadding = theme.space(SpaceRole.Md),
            verticalPadding = theme.space(SpaceRole.Sm),
            minHeight = 34.dp,
            typeRole = TypeRole.BodyStrong,
            // HIG buttons never cast a shadow, pressed or not (FR-14.2).
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            ButtonVariant.Filled -> base.copy(
                container = theme.color(ColorRole.Primary),
                pressedContainer = theme.color(ColorRole.Primary).copy(alpha = PRESSED_ALPHA),
                content = theme.color(ColorRole.OnPrimary),
            )

            ButtonVariant.Tonal -> base.copy(
                container = theme.color(ColorRole.SurfaceVariant),
                pressedContainer = theme.color(ColorRole.OutlineVariant),
                content = theme.color(ColorRole.Primary),
            )

            ButtonVariant.Outlined -> base.copy(
                pressedContainer = theme.color(ColorRole.SurfaceVariant),
                borderWidth = 1.dp,
                borderColor = theme.color(ColorRole.Outline),
                pressedBorderColor = theme.color(ColorRole.Outline),
            )

            // A plain button: content colour only, no container even when pressed.
            ButtonVariant.Text -> base
        }
    }

    override val motion: Motion = Motion(
        pressMillis = 80,
        releaseMillis = 180,
        easing = LinearOutSlowInEasing,
    )

    private const val PRESSED_ALPHA = 0.6f
    private const val AMBIENT_ALPHA = 0.08f
    private const val SPOT_ALPHA = 0.12f
    private const val SPREAD = 2f
}

/**
 * WinUI / Fluent 2 rules (FR-14.6 items 5 to 7).
 *
 * Reference: fluent2.microsoft.design, "Elevation", "Button" and "Motion duration", 2024.
 *
 * Elevation is a layer shadow with a hairline stroke, buttons are 4 dp rounded with a
 * lighter top edge, and the press is an immediate tone change with no travel.
 */
internal object FluentRules : ComponentRules {
    override fun elevation(modifier: Modifier, elevation: Dp, shape: Shape, theme: ResolvedTheme): Modifier {
        if (elevation.value <= 0f) return modifier
        // Fluent pairs every layer shadow with a thin stroke so the layer edge stays crisp.
        return modifier
            .shadow(elevation, shape, clip = false)
            .border(1.dp, theme.color(ColorRole.OutlineVariant), shape)
    }

    override fun button(variant: ButtonVariant, theme: ResolvedTheme): ButtonStyle {
        val outline = theme.color(ColorRole.Outline)
        val base = ButtonStyle(
            container = theme.color(ColorRole.Surface),
            pressedContainer = theme.color(ColorRole.SurfaceVariant),
            content = theme.color(ColorRole.OnSurface),
            pressedContentAlpha = 1f,
            borderWidth = 1.dp,
            borderColor = outline,
            pressedBorderColor = theme.color(ColorRole.OutlineVariant),
            // The lighter top edge every Fluent control carries.
            topHighlight = Color.White.copy(alpha = if (theme.dark) 0.08f else 0.6f),
            // Fluent corners are 4 dp, which is ShapeRole.Medium in the Fluent table.
            shape = theme.shape(ShapeRole.Medium),
            horizontalPadding = theme.space(SpaceRole.Lg),
            verticalPadding = theme.space(SpaceRole.Sm),
            minHeight = 32.dp,
            typeRole = TypeRole.BodyStrong,
            restElevation = 0.dp,
            pressedElevation = 0.dp,
        )
        return when (variant) {
            // Accent button.
            ButtonVariant.Filled -> base.copy(
                container = theme.color(ColorRole.Primary),
                pressedContainer = lerp(theme.color(ColorRole.Primary), Color.Black, PRESS_SHADE),
                content = theme.color(ColorRole.OnPrimary),
                borderColor = theme.color(ColorRole.Primary),
                pressedBorderColor = Color.Transparent,
                topHighlight = Color.White.copy(alpha = 0.18f),
            )

            // Standard button.
            ButtonVariant.Tonal -> base

            // Standard button with a stronger stroke.
            ButtonVariant.Outlined -> base.copy(
                container = Color.Transparent,
                borderColor = outline,
                topHighlight = null,
            )

            // Subtle button: no container until it is pressed.
            ButtonVariant.Text -> base.copy(
                container = Color.Transparent,
                borderWidth = 0.dp,
                borderColor = Color.Transparent,
                pressedBorderColor = Color.Transparent,
                topHighlight = null,
            )
        }
    }

    override val motion: Motion = Motion(
        // Fluent's "ultra fast" duration: the press reads as instant.
        pressMillis = 50,
        releaseMillis = 100,
        easing = LinearEasing,
    )

    private const val PRESS_SHADE = 0.12f
}
