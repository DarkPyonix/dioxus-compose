package dioxus.compose.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.Modifier as ProtocolModifier
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.runtime.EventDispatcher

/**
 * Rebuilds a Compose `Modifier` chain from the Host's modifier value list.
 *
 * List order is chain order, so `[Padding(16), FillMaxWidth]` and the reverse differ exactly
 * as they do in hand-written Compose.
 *
 * Roles are resolved here against the active design system's token table:
 * the Host sent a role, the Renderer decides what it measures.
 */
internal fun List<ProtocolModifier>.toComposeModifier(
    nodeId: Int,
    dispatcher: EventDispatcher,
    theme: ResolvedTheme,
): Modifier {
    // The last Shape or ShapeRole in the list is what clips, what the border
    // follows and what the background fills, whatever their order in the chain.
    val shape = resolvedShape(theme)
    return fold(Modifier as Modifier) { chain, value ->
        when (value) {
            is ProtocolModifier.Empty -> chain
            is ProtocolModifier.Padding -> chain.padding(value.value.dp)
            is ProtocolModifier.FillMaxWidth -> chain.fillMaxWidth()
            is ProtocolModifier.FillMaxHeight -> chain.fillMaxHeight()
            is ProtocolModifier.Width -> chain.width(value.value.dp)
            is ProtocolModifier.Height -> chain.height(value.value.dp)
            is ProtocolModifier.Size -> chain.size(value.width.dp, value.height.dp)
            is ProtocolModifier.Background -> chain.background(theme.color(value.paint), shape)
            is ProtocolModifier.Clickable -> chain.hostClickable(nodeId, value.handlerId, dispatcher)

            is ProtocolModifier.PaddingEach ->
                chain.padding(value.start.dp, value.top.dp, value.end.dp, value.bottom.dp)
            is ProtocolModifier.PaddingRole -> chain.padding(theme.space(value.role))
            is ProtocolModifier.Shape -> chain.clip(shape)
            is ProtocolModifier.ShapeRole -> chain.clip(shape)
            is ProtocolModifier.Border ->
                chain.border(value.width.dp, theme.color(value.paint), shape)
            is ProtocolModifier.Elevation ->
                theme.rules.elevation(chain, value.value.dp, shape, theme)

            // Weight is parent data: it is applied by the Column or Row that owns this node,
            // not here. See `weightOf` and `Children` in RenderNode.kt.
            is ProtocolModifier.Weight -> chain
        }
    }
}

/** The shape this node's clip, border and background all use. */
internal fun List<ProtocolModifier>.resolvedShape(theme: ResolvedTheme): Shape {
    for (index in indices.reversed()) {
        when (val value = this[index]) {
            is ProtocolModifier.Shape -> return androidx.compose.foundation.shape.RoundedCornerShape(
                topStart = value.topStart.dp,
                topEnd = value.topEnd.dp,
                bottomEnd = value.bottomEnd.dp,
                bottomStart = value.bottomStart.dp,
            )

            is ProtocolModifier.ShapeRole -> return theme.shape(value.role)
            else -> Unit
        }
    }
    return RectangleShape
}

/** The weight this node asked its parent layout for, or null. */
internal fun List<ProtocolModifier>.weightOf(): Float? =
    lastOrNull { it is ProtocolModifier.Weight }
        ?.let { (it as ProtocolModifier.Weight).value }
        ?.takeIf { it > 0f }

/**
 * Pointer handling for `Modifier.Clickable`.
 *
 * The Host handler runs synchronously inside the gesture, and its result decides whether the
 * pointer change is consumed. A Host that does not consume leaves the gesture
 * available to whatever is underneath.
 */
private fun Modifier.hostClickable(
    nodeId: Int,
    handlerId: Long,
    dispatcher: EventDispatcher,
): Modifier = pointerInput(nodeId, handlerId, dispatcher) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = true)
        val up = waitForUpOrCancellation() ?: return@awaitEachGesture
        if (dispatcher.dispatch(HostEvent.Clicked(nodeId, handlerId))) {
            down.consume()
            up.consume()
        }
    }
}
