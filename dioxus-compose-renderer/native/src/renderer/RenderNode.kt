package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
    val modifier = node.modifiers.toComposeModifier(nodeId, dispatcher).testTag(nodeTestTag(nodeId))
    when (node.widget) {
        WidgetKind.Column -> Column(modifier) { Children(node, table, dispatcher) }
        WidgetKind.Row -> Row(modifier) { Children(node, table, dispatcher) }
        WidgetKind.Box -> Box(modifier) { Children(node, table, dispatcher) }
        WidgetKind.Text -> BasicText(text = node.text(PropertyKind.Text), modifier = modifier)
        WidgetKind.Spacer -> Spacer(modifier)
        WidgetKind.Button -> HostButton(node, modifier, dispatcher)
        WidgetKind.TextField -> HostTextField(node, modifier, dispatcher)
        // TODO(FR-8): windowing, so the Host materialises only the visible range. Rendering
        // the materialised children keeps the tree correct meanwhile; it is not yet lazy.
        WidgetKind.LazyColumn -> Column(modifier) { Children(node, table, dispatcher) }
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
