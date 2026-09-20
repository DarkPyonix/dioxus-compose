package dioxus.compose.ui.node

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.key
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.TypeRole
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.design.LocalDesignTheme
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.foundation.HostButton
import dioxus.compose.foundation.HostLazyColumn
import dioxus.compose.foundation.HostTextField
import dioxus.compose.foundation.hostKeyEvents
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.ui.boxAlignment
import dioxus.compose.ui.horizontalAlignment
import dioxus.compose.ui.horizontalArrangement
import dioxus.compose.ui.maxLines
import dioxus.compose.ui.overflow
import dioxus.compose.ui.textStyle
import dioxus.compose.ui.toComposeModifier
import dioxus.compose.ui.verticalAlignment
import dioxus.compose.ui.verticalArrangement
import dioxus.compose.ui.weightOf

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
 *
 * `parentModifier` carries what only the parent layout can express, which today is the
 * `Weight` of FR-13.4: weight is parent data of a `Column` or `Row` scope and cannot be
 * produced by a modifier chain built outside that scope.
 */
@Composable
fun RenderNode(
    nodeId: Int,
    table: NodeTable,
    dispatcher: EventDispatcher,
    parentModifier: Modifier = Modifier,
) {
    val node = table.node(nodeId) ?: return
    // Observation point for the FR-4 recomposition check; null in production.
    RenderNodeObserver.onCompose?.let { observer -> SideEffect { observer(nodeId) } }
    val theme = LocalDesignTheme.current
    val chain = parentModifier
        .then(node.modifiers.toComposeModifier(nodeId, dispatcher, theme))
        .testTag(nodeTestTag(nodeId))
    // The TextField wires its own key handling, because it has an editor to intercept and a
    // composition to protect (SPEC FR-12); everything else routes keys here.
    val keyDownHandler = node.handler(PropertyKind.OnKeyDown)
    val modifier = if (keyDownHandler == null || node.widget == WidgetKind.TextField) {
        chain
    } else {
        chain.hostKeyEvents(nodeId, keyDownHandler, dispatcher)
    }
    when (node.widget) {
        WidgetKind.Column -> Column(
            modifier = modifier,
            verticalArrangement = node.verticalArrangement(theme),
            horizontalAlignment = node.horizontalAlignment(),
        ) { Children(node, table, dispatcher) }

        WidgetKind.Row -> Row(
            modifier = modifier,
            horizontalArrangement = node.horizontalArrangement(theme),
            verticalAlignment = node.verticalAlignment(),
        ) { Children(node, table, dispatcher) }

        WidgetKind.Box -> Box(modifier, contentAlignment = node.boxAlignment()) {
            Children(node, table, dispatcher)
        }

        WidgetKind.Text -> BasicText(
            text = node.text(PropertyKind.Text),
            modifier = modifier,
            style = node.textStyle(theme),
            maxLines = node.maxLines(),
            overflow = node.overflow(),
        )

        WidgetKind.Spacer -> Spacer(modifier)
        WidgetKind.Button -> HostButton(node, modifier, dispatcher, theme)
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
        WidgetKind.ScrollColumn -> Column(
            modifier = modifier.verticalScroll(rememberScrollState()),
            verticalArrangement = node.verticalArrangement(theme),
            horizontalAlignment = node.horizontalAlignment(),
        ) { Children(node, table, dispatcher) }
    }
}

/**
 * Children of a Column: weight is applied here because `Modifier.weight` exists only inside
 * `ColumnScope` (SPEC FR-13.4).
 */
@Composable
private fun ColumnScope.Children(node: Node, table: NodeTable, dispatcher: EventDispatcher) {
    // `key` keeps each child's state attached to its Host node id across Move mutations.
    node.children.forEach { childId -> key(childId) { WeightedChild(childId, table, dispatcher) } }
}

@Composable
private fun ColumnScope.WeightedChild(childId: Int, table: NodeTable, dispatcher: EventDispatcher) {
    // Its own composable, so reading the child's modifier list subscribes this scope alone
    // and a `SetModifier` on one child cannot invalidate its siblings (SPEC FR-4).
    val weight = table.node(childId)?.modifiers?.weightOf()
    RenderNode(
        childId,
        table,
        dispatcher,
        if (weight == null) Modifier else Modifier.weight(weight),
    )
}

@Composable
private fun RowScope.Children(node: Node, table: NodeTable, dispatcher: EventDispatcher) {
    node.children.forEach { childId -> key(childId) { WeightedChild(childId, table, dispatcher) } }
}

@Composable
private fun RowScope.WeightedChild(childId: Int, table: NodeTable, dispatcher: EventDispatcher) {
    val weight = table.node(childId)?.modifiers?.weightOf()
    RenderNode(
        childId,
        table,
        dispatcher,
        if (weight == null) Modifier else Modifier.weight(weight),
    )
}

/** A Box has no weight axis, so its children are drawn as they are. */
@Composable
private fun BoxScope.Children(node: Node, table: NodeTable, dispatcher: EventDispatcher) {
    node.children.forEach { childId -> key(childId) { RenderNode(childId, table, dispatcher) } }
}

/** The label of a Button follows the design system's button type role unless overridden. */
internal fun Node.buttonTextStyle(theme: ResolvedTheme, role: TypeRole) = textStyle(theme, role)
