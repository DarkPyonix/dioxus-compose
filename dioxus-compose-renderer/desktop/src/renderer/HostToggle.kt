package dioxus.compose.foundation

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.design.ToggleRole
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.ui.node.Node

/**
 * A Checkbox, a RadioButton or a Switch, drawn by the active design system's rules.
 *
 * All three are controlled: the widget draws exactly what the Host sent and reports the
 * state the user asked for, so the value the Host holds and the control on screen cannot
 * drift apart. Nothing here decides a size, a colour or a shape. Those come from
 * [dioxus.compose.design.ComponentRules.controls], which is why the same declaration is a
 * square Material box, a round Cupertino one and a stroked Fluent one.
 */
@Composable
internal fun HostToggle(
    role: ToggleRole,
    node: Node,
    modifier: Modifier,
    dispatcher: EventDispatcher,
    theme: ResolvedTheme,
) {
    val checked = node.flag(PropertyKind.Checked, default = false)
    val enabled = node.flag(PropertyKind.Enabled, default = true)
    val style = theme.rules.controls(theme).toggle(role)
    val motion = theme.rules.motion
    val spec = tween<Float>(
        durationMillis = if (checked) motion.pressMillis else motion.releaseMillis,
        easing = motion.easing,
    )
    // The transition between the two states belongs to the Renderer, so a toggle animates
    // without a frame of it crossing the boundary.
    val progress by animateFloatAsState(if (checked) 1f else 0f, spec)
    val container by animateColorAsState(
        targetValue = if (checked) style.containerChecked else style.container,
        animationSpec = tween(spec.durationMillis, easing = motion.easing),
    )
    val alpha = if (enabled) 1f else style.disabledAlpha
    val handlerId = node.handler(PropertyKind.OnValueChange)

    val clickable = if (handlerId == null) {
        modifier
    } else {
        modifier.clickable(
            interactionSource = remember { MutableInteractionSource() },
            // The design system owns the feedback, and a ripple is only one system's
            // answer to what a press looks like.
            indication = null,
            enabled = enabled,
            role = if (role == ToggleRole.RadioButton) Role.RadioButton else Role.Switch,
        ) {
            // Off is 0.0 and on is 1.0. The Host decides whether to honour the request.
            dispatcher.dispatch(
                HostEvent.ValueChanged(node.id, handlerId, if (checked) 0.0 else 1.0),
            )
        }
    }

    when (role) {
        ToggleRole.Checkbox -> Box(
            clickable
                .size(style.size)
                .clip(style.shape)
                .background(container.copy(alpha = container.alpha * alpha), style.shape)
                .border(style.borderWidth, style.border.copy(alpha = alpha), style.shape)
                .drawBehind { drawTick(style.mark.copy(alpha = alpha), progress) },
        )

        ToggleRole.RadioButton -> Box(
            clickable
                .size(style.size)
                .clip(style.shape)
                .background(container.copy(alpha = container.alpha * alpha), style.shape)
                .border(style.borderWidth, style.border.copy(alpha = alpha), style.shape)
                .drawBehind {
                    val radius = style.thumbSize.toPx() / 2f * progress
                    if (radius > 0f) {
                        drawCircle(style.mark.copy(alpha = alpha), radius = radius)
                    }
                },
        )

        ToggleRole.Switch -> Box(
            clickable
                .size(width = style.trackWidth, height = style.trackHeight)
                .clip(style.shape)
                .background(container.copy(alpha = container.alpha * alpha), style.shape)
                .border(style.borderWidth, style.border.copy(alpha = alpha), style.shape)
                .drawBehind {
                    val diameter = style.thumbSize.toPx()
                    val margin = (size.height - diameter) / 2f
                    val travel = size.width - diameter - margin * 2f
                    val thumb = if (checked) style.mark else style.markUnchecked
                    drawCircle(
                        color = thumb.copy(alpha = alpha),
                        radius = diameter / 2f,
                        center = Offset(
                            x = margin + diameter / 2f + travel * progress,
                            y = size.height / 2f,
                        ),
                    )
                },
        )
    }
}

/**
 * The checkmark, drawn as two strokes that sweep in together.
 *
 * `progress` is how much of the mark has appeared, so a half-finished transition draws a
 * partial tick rather than blinking a whole one into place.
 */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawTick(
    color: Color,
    progress: Float,
) {
    if (progress <= 0f) return
    val width = size.width
    val height = size.height
    val path = Path().apply {
        moveTo(width * 0.22f, height * 0.52f)
        lineTo(width * 0.42f, height * 0.72f)
        lineTo(width * 0.78f, height * 0.30f)
    }
    // Fading the stroke rather than measuring the path keeps this free of the path
    // measurement API, which differs between the platforms this renderer targets.
    drawPath(
        path = path,
        color = color.copy(alpha = color.alpha * progress.coerceIn(0f, 1f)),
        style = Stroke(width = width * 0.14f, cap = StrokeCap.Round),
    )
}
