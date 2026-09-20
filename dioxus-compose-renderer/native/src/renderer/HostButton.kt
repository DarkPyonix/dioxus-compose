package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import org.thisisthepy.dioxus.compose.protocol.HostEvent
import org.thisisthepy.dioxus.compose.protocol.PropertyKind

/**
 * A Button drawn by the active design system's component rules (SPEC FR-14.2, FR-14.6).
 *
 * The same rsx deliberately produces different pixels per system: the shape, the label
 * weight and the press feedback all come from [ComponentRules], not from this function.
 */
@Composable
internal fun HostButton(
    node: Node,
    modifier: Modifier,
    dispatcher: EventDispatcher,
    theme: ResolvedTheme,
) {
    val enabled = node.flag(PropertyKind.Enabled, default = true)
    val handlerId = node.handler(PropertyKind.OnClick)
    val style = theme.rules.button(node.variant(), theme)
    val interactions = remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val motion = theme.rules.motion
    val spec = tween<Float>(
        durationMillis = if (pressed) motion.pressMillis else motion.releaseMillis,
        easing = motion.easing,
    )
    val container by animateColorAsState(
        targetValue = if (pressed) style.pressedContainer else style.container,
        animationSpec = tween(spec.durationMillis, easing = motion.easing),
    )
    val borderColor by animateColorAsState(
        targetValue = if (pressed) style.pressedBorderColor else style.borderColor,
        animationSpec = tween(spec.durationMillis, easing = motion.easing),
    )
    val contentAlpha by animateFloatAsState(
        targetValue = if (pressed) style.pressedContentAlpha else 1f,
        animationSpec = spec,
    )
    val elevation by animateDpAsState(
        targetValue = if (pressed) style.pressedElevation else style.restElevation,
        animationSpec = tween(spec.durationMillis, easing = motion.easing),
    )

    val clickable = if (handlerId == null) {
        modifier
    } else {
        // The Host result is not used here: a click that reached a Button is never offered
        // to anything below it. Indication is null because the design system owns the
        // feedback (FR-14.6 item 6), and a ripple is only one system's answer.
        modifier.clickable(
            interactionSource = interactions,
            indication = null,
            enabled = enabled,
            role = Role.Button,
        ) { dispatcher.dispatch(HostEvent.Clicked(node.id, handlerId)) }
    }

    val decorated = theme.rules
        .elevation(clickable, elevation, style.shape, theme)
        .clip(style.shape)
        .background(container, style.shape)
        .then(
            if (style.borderWidth.value > 0f) {
                Modifier.border(style.borderWidth, borderColor, style.shape)
            } else {
                Modifier
            },
        )
        .topHighlight(style.topHighlight)
        .defaultMinSize(minHeight = style.minHeight)
        .padding(horizontal = style.horizontalPadding, vertical = style.verticalPadding)

    val label = node.textStyle(theme, style.typeRole)
        .copy(color = style.content.copy(alpha = style.content.alpha * contentAlpha))
    Box(decorated, contentAlignment = Alignment.Center) {
        BasicText(text = node.text(PropertyKind.Text), style = label)
    }
}

/** Fluent's lighter top edge. Other systems pass null and pay nothing (FR-14.2). */
private fun Modifier.topHighlight(color: Color?): Modifier {
    if (color == null) return this
    return drawWithContent {
        drawContent()
        val thickness = 1.dpPx(density)
        drawLine(
            color = color,
            start = androidx.compose.ui.geometry.Offset(0f, thickness / 2f),
            end = androidx.compose.ui.geometry.Offset(size.width, thickness / 2f),
            strokeWidth = thickness,
        )
    }
}

private fun Int.dpPx(density: Float): Float = this * density
