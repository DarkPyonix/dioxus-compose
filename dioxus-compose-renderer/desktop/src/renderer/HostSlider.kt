package dioxus.compose.foundation

import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.ui.intProp
import dioxus.compose.ui.node.Node
import kotlin.math.roundToInt

/**
 * A Slider drawn by the active design system's rules.
 *
 * The position a drag is passing through is held here, not in the Host: following a
 * finger would otherwise cost a boundary call per frame. The Host's `value` seeds that
 * position and moves it when the change came from somewhere else, and every position the
 * slider settles on is reported once.
 */
@Composable
internal fun HostSlider(
    node: Node,
    modifier: Modifier,
    dispatcher: EventDispatcher,
    theme: ResolvedTheme,
) {
    val enabled = node.flag(PropertyKind.Enabled, default = true)
    val style = theme.rules.controls(theme).slider
    val min = node.number(PropertyKind.Min) ?: 0f
    val max = node.number(PropertyKind.Max) ?: 1f
    val span = (max - min).takeIf { it > 0f }
    // Zero means continuous, which is also what an absent property means.
    val steps = (node.intProp(PropertyKind.Steps) ?: 0L).toInt().coerceAtLeast(0)
    val fromHost = (node.number(PropertyKind.Value) ?: min).coerceIn(min, max)

    var current by remember(node.id) { mutableFloatStateOf(fromHost) }
    LaunchedEffect(node.id, fromHost) { current = fromHost }

    val handlerId = node.handler(PropertyKind.OnValueChange)
    val report = report@{ candidate: Float ->
        val snapped = snap(candidate.coerceIn(min, max), min, max, steps)
        if (snapped == current) return@report
        current = snapped
        if (handlerId != null) {
            dispatcher.dispatch(HostEvent.ValueChanged(node.id, handlerId, snapped.toDouble()))
        }
    }

    val fraction = if (span == null) 0f else ((current - min) / span).coerceIn(0f, 1f)
    val thumbPx = style.thumbSize
    val interactive = if (!enabled) {
        modifier
    } else {
        modifier
            .pointerInput(node.id, min, max, steps) {
                detectTapGestures { offset ->
                    report(valueAt(offset.x, size.width.toFloat(), thumbPx.toPx(), min, max))
                }
            }
            .pointerInput(node.id, min, max, steps) {
                detectHorizontalDragGestures { change, _ ->
                    report(
                        valueAt(change.position.x, size.width.toFloat(), thumbPx.toPx(), min, max),
                    )
                }
            }
    }

    Box(
        interactive
            .defaultMinSize(minWidth = 120.dp)
            .fillMaxWidth()
            .height(maxOf(thumbPx, style.trackHeight))
            .drawBehind {
                val thumb = thumbPx.toPx()
                val trackTop = (size.height - style.trackHeight.toPx()) / 2f
                val trackHeight = style.trackHeight.toPx()
                val left = thumb / 2f
                val usable = size.width - thumb
                val radius = CornerRadius(trackHeight / 2f, trackHeight / 2f)
                drawRoundRect(
                    color = style.track,
                    topLeft = Offset(left, trackTop),
                    size = Size(usable, trackHeight),
                    cornerRadius = radius,
                )
                drawRoundRect(
                    color = style.activeTrack,
                    topLeft = Offset(left, trackTop),
                    size = Size(usable * fraction, trackHeight),
                    cornerRadius = radius,
                )
                // The stops, in the systems that mark them. A slider with no steps is
                // continuous and has nothing to mark.
                val tick = style.tick
                if (tick != null && steps > 0) {
                    for (index in 1..steps) {
                        val at = left + usable * (index.toFloat() / (steps + 1))
                        drawCircle(tick, radius = trackHeight / 4f, center = Offset(at, trackTop + trackHeight / 2f))
                    }
                }
                val centre = Offset(left + usable * fraction, size.height / 2f)
                drawCircle(style.thumb, radius = thumb / 2f, center = centre)
                if (style.thumbBorderWidth.value > 0f) {
                    val border = style.thumbBorderWidth.toPx()
                    drawCircle(
                        color = style.thumbBorder,
                        radius = (thumb - border) / 2f,
                        center = centre,
                        style = Stroke(width = border),
                    )
                }
            },
    )
}

/** The value a press at [x] lands on, with the thumb's own width taken off both ends. */
private fun valueAt(x: Float, width: Float, thumb: Float, min: Float, max: Float): Float {
    val usable = (width - thumb).takeIf { it > 0f } ?: return min
    return min + (max - min) * ((x - thumb / 2f) / usable).coerceIn(0f, 1f)
}

/**
 * The nearest stop, or the value itself when the slider is continuous.
 *
 * `steps` counts the stops between the two ends, so a slider with three of them has five
 * positions in all.
 */
private fun snap(value: Float, min: Float, max: Float, steps: Int): Float {
    if (steps <= 0 || max <= min) return value
    val intervals = steps + 1
    val position = (value - min) / (max - min) * intervals
    return min + (max - min) * (position.roundToInt().toFloat() / intervals)
}
