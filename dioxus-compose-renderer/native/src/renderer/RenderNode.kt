package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import org.thisisthepy.dioxus.compose.protocol.HostEvent
import org.thisisthepy.dioxus.compose.protocol.PropertyKind
import org.thisisthepy.dioxus.compose.protocol.WidgetKind

/**
 * Counts recompositions of interpreted nodes (SPEC FR-4). Set only by tests.
 */
internal object RenderNodeObserver {
    var onCompose: ((Int) -> Unit)? = null
}

/** Test tag every interpreted node carries, so tests can address nodes by Host node id. */
fun nodeTestTag(nodeId: Int): String = "dioxus-node-$nodeId"

/**
 * Interprets one node of the table into real Compose widgets (SPEC FR-2).
 *
 * Only the node's own state is read here, so a `SetProp` on a sibling cannot invalidate this
 * composable (SPEC FR-4).
 */
@Composable
fun RenderNode(nodeId: Int, table: NodeTable, dispatcher: EventDispatcher) {
    val node = table.node(nodeId) ?: return
    // Observation point for the FR-4 recomposition check; null in production.
    RenderNodeObserver.onCompose?.let { observer -> SideEffect { observer(nodeId) } }
    val chain = node.modifiers.toComposeModifier(nodeId, dispatcher).testTag(nodeTestTag(nodeId))
    // The TextField wires its own key handling, because it has an editor to intercept and a
    // composition to protect (SPEC FR-12); everything else routes keys here.
    val keyDownHandler = node.handler(PropertyKind.OnKeyDown)
    val modifier = if (keyDownHandler == null || node.widget == WidgetKind.TextField) {
        chain
    } else {
        chain.hostKeyEvents(nodeId, keyDownHandler, dispatcher)
    }
    when (node.widget) {
        WidgetKind.Column -> Column(modifier) { Children(node, table, dispatcher) }
        WidgetKind.Row -> Row(modifier) { Children(node, table, dispatcher) }
        WidgetKind.Box -> Box(modifier) { Children(node, table, dispatcher) }
        WidgetKind.Text -> BasicText(text = node.text(PropertyKind.Text), modifier = modifier)
        WidgetKind.Spacer -> Spacer(modifier)
        WidgetKind.Button -> HostButton(node, modifier, dispatcher)
        WidgetKind.TextField -> HostTextField(node, modifier, dispatcher)
        WidgetKind.LazyColumn -> HostLazyColumn(node, modifier, table, dispatcher)
        // FR-13: a column that scrolls without the Host windowing it, so every child is
        // materialised. Use LazyColumn when the list is long.
        // FR-11: declared by an extension package rather than the core schema, and drawn
        // like any built-in widget.
        WidgetKind.LinearProgressIndicator -> {
            val progress = node.number(PropertyKind.Progress)
            if (progress == null) {
                LinearProgressIndicator(modifier = modifier)
            } else {
                LinearProgressIndicator(progress = { progress }, modifier = modifier)
            }
        }
        WidgetKind.ScrollColumn -> Column(modifier.verticalScroll(rememberScrollState())) {
            Children(node, table, dispatcher)
        }
    }
}

@Composable
private fun Children(node: Node, table: NodeTable, dispatcher: EventDispatcher) {
    // `key` keeps each child's state attached to its Host node id across Move mutations.
    node.children.forEach { childId ->
        androidx.compose.runtime.key(childId) { RenderNode(childId, table, dispatcher) }
    }
}

@Composable
private fun HostButton(node: Node, modifier: Modifier, dispatcher: EventDispatcher) {
    val enabled = node.flag(PropertyKind.Enabled, default = true)
    val handlerId = node.handler(PropertyKind.OnClick)
    val clickable = if (handlerId == null) {
        modifier
    } else {
        // Compose's `clickable` owns the gesture and its consumption; the Host result is not
        // used here because a click that reached a Button is never offered to anything below.
        modifier.clickable(enabled = enabled, role = Role.Button) {
            dispatcher.dispatch(HostEvent.Clicked(node.id, handlerId))
        }
    }
    Box(clickable) { BasicText(node.text(PropertyKind.Text)) }
}
