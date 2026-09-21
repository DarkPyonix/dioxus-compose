package dioxus.compose.foundation

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.ui.node.Node

/**
 * A ProgressIndicator drawn by the active design system's rules.
 *
 * `determinate` says whether the value means anything, and `circular` picks the form.
 * Everything else, the thickness, the colours, whether the ends are round or square and
 * how fast an indeterminate sweep travels, comes from
 * [dioxus.compose.design.ComponentRules.controls]. How fast something moves is motion,
 * and motion is the design system's decision rather than a Host property.
 */
@Composable
internal fun HostProgressIndicator(node: Node, modifier: Modifier, theme: ResolvedTheme) {
    val style = theme.rules.controls(theme).progress
    val determinate = node.flag(PropertyKind.Determinate, default = true)
    val circular = node.flag(PropertyKind.Circular, default = false)
    val value = (node.number(PropertyKind.Value) ?: 0f).coerceIn(0f, 1f)

    val sweep by rememberInfiniteTransition(label = "progress").animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(style.periodMillis, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "sweep",
    )
    // An indeterminate indicator is the only one that animates. A determinate one reads
    // the value it was given and stays there.
    val phase = if (determinate) 0f else sweep
    val cap = if (style.rounded) StrokeCap.Round else StrokeCap.Butt

    if (circular) {
        Box(
            modifier.size(style.diameter).drawBehind {
                val stroke = Stroke(width = style.thickness.toPx(), cap = cap)
                val inset = style.thickness.toPx() / 2f
                val box = Size(size.width - inset * 2f, size.height - inset * 2f)
                drawArc(
                    color = style.track,
                    startAngle = 0f,
                    sweepAngle = FULL_TURN,
                    useCenter = false,
                    topLeft = Offset(inset, inset),
                    size = box,
                    style = stroke,
                )
                drawArc(
                    color = style.indicator,
                    // A determinate ring starts at twelve o'clock; an indeterminate one
                    // chases its own tail from wherever the sweep has reached.
                    startAngle = if (determinate) QUARTER_TURN_BACK else phase * FULL_TURN,
                    sweepAngle = if (determinate) value * FULL_TURN else INDETERMINATE_ARC,
                    useCenter = false,
                    topLeft = Offset(inset, inset),
                    size = box,
                    style = stroke,
                )
            },
        )
        return
    }

    Box(
        modifier
            .fillMaxWidth()
            .height(style.thickness)
            .drawBehind {
                val radius = if (style.rounded) {
                    CornerRadius(size.height / 2f, size.height / 2f)
                } else {
                    CornerRadius.Zero
                }
                drawRoundRect(color = style.track, cornerRadius = radius)
                val width = if (determinate) size.width * value else size.width * INDETERMINATE_BAR
                val left = if (determinate) {
                    0f
                } else {
                    // The bar travels off one end and back on at the other.
                    (size.width + width) * phase - width
                }
                drawRoundRect(
                    color = style.indicator,
                    topLeft = Offset(left, 0f),
                    size = Size(width.coerceAtMost(size.width), size.height),
                    cornerRadius = radius,
                )
            },
    )
}

private const val FULL_TURN = 360f
private const val QUARTER_TURN_BACK = -90f

/** How much of the ring an indeterminate indicator fills while it turns. */
private const val INDETERMINATE_ARC = 90f

/** How much of the bar an indeterminate indicator fills while it travels. */
private const val INDETERMINATE_BAR = 0.3f
