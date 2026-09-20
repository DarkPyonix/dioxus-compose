package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import org.thisisthepy.dioxus.compose.protocol.HostEvent
import org.thisisthepy.dioxus.compose.protocol.Paint
import org.thisisthepy.dioxus.compose.protocol.Modifier as ProtocolModifier

/**
 * Rebuilds a Compose `Modifier` chain from the Host's modifier value list (SPEC FR-10).
 *
 * List order is chain order, so `[Padding(16), FillMaxWidth]` and the reverse differ exactly
 * as they do in hand-written Compose.
 */
internal fun List<ProtocolModifier>.toComposeModifier(
    nodeId: Int,
    dispatcher: EventDispatcher,
): Modifier = fold(Modifier as Modifier) { chain, value ->
    when (value) {
        is ProtocolModifier.Empty -> chain
        is ProtocolModifier.Padding -> chain.padding(value.value.dp)
        is ProtocolModifier.FillMaxWidth -> chain.fillMaxWidth()
        is ProtocolModifier.FillMaxHeight -> chain.fillMaxHeight()
        is ProtocolModifier.Width -> chain.width(value.value.dp)
        is ProtocolModifier.Height -> chain.height(value.value.dp)
        is ProtocolModifier.Size -> chain.size(value.width.dp, value.height.dp)
        is ProtocolModifier.Background -> value.paint.color()?.let { chain.background(it) } ?: chain
        is ProtocolModifier.Clickable -> chain.hostClickable(nodeId, value.handlerId, dispatcher)

        // FR-13 primitives that carry a value the Renderer can honour without a design
        // system behind it.
        is ProtocolModifier.PaddingEach ->
            chain.padding(value.start.dp, value.top.dp, value.end.dp, value.bottom.dp)
        is ProtocolModifier.Shape -> chain.clip(
            RoundedCornerShape(
                topStart = value.topStart.dp,
                topEnd = value.topEnd.dp,
                bottomEnd = value.bottomEnd.dp,
                bottomStart = value.bottomStart.dp,
            ),
        )
        is ProtocolModifier.Border -> value.paint.color()
            ?.let { chain.border(value.width.dp, it) } ?: chain
        is ProtocolModifier.Elevation -> chain.shadow(value.value.dp)

        // TODO(FR-14): these name a role in a design system's token table, and the tables
        // are the Renderer's next piece of work. Ignoring them leaves the widget unstyled
        // rather than wrongly styled.
        is ProtocolModifier.PaddingRole -> chain
        is ProtocolModifier.ShapeRole -> chain

        // TODO(FR-13): weight belongs to the parent's layout scope, not to a modifier
        // chain built outside it, so it needs plumbing through Column and Row.
        is ProtocolModifier.Weight -> chain
    }
}

/**
 * A literal colour becomes a Compose colour. A role has to be resolved against a design
 * system's token table, which does not exist yet, so it paints nothing for now.
 */
private fun Paint.color(): Color? = when (this) {
    is Paint.Literal -> Color(argb)
    is Paint.Role -> null
}

/**
 * Pointer handling for `Modifier.Clickable`.
 *
 * The Host handler runs synchronously inside the gesture, and its result decides whether the
 * pointer change is consumed (SPEC FR-12). A Host that does not consume leaves the gesture
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
