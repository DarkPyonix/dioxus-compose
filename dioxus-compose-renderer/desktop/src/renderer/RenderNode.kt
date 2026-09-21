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
import dioxus.compose.design.ContainerRole
import dioxus.compose.design.LocalDesignTheme
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.foundation.HostButton
import dioxus.compose.foundation.HostContainerColumn
import dioxus.compose.foundation.HostDialog
import dioxus.compose.foundation.HostCanvas
import dioxus.compose.foundation.HostDatePicker
import dioxus.compose.foundation.HostDropdown
import dioxus.compose.foundation.HostIcon
import dioxus.compose.foundation.HostImage
import dioxus.compose.foundation.HostTimePicker
import dioxus.compose.foundation.HostLazyColumn
import dioxus.compose.foundation.HostLazyRow
import dioxus.compose.foundation.HostMenu
import dioxus.compose.foundation.HostTabs
import dioxus.compose.foundation.HostTooltip
import dioxus.compose.foundation.HostTopAppBar
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
 * Counts recompositions of interpreted nodes, so a test can assert that changing one node
 * does not recompose its siblings. Set only by tests.
 */
internal object RenderNodeObserver {
    var onCompose: ((Int) -> Unit)? = null
}

/** Test tag every interpreted node carries, so tests can address nodes by Host node id. */
fun nodeTestTag(nodeId: Int): String = "dioxus-node-$nodeId"

/**
 * Interprets one node of the table into real Compose widgets.
 *
 * Only the node's own state is read here, so a `SetProp` on a sibling cannot invalidate this
 * composable.
 *
 * `parentModifier` carries what only the parent layout can express, which today is the
 * `Weight` modifier: weight is parent data of a `Column` or `Row` scope and cannot be
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
    // Observation point for the recomposition test; null in production.
    RenderNodeObserver.onCompose?.let { observer -> SideEffect { observer(nodeId) } }
    val theme = LocalDesignTheme.current
    val chain = parentModifier
        .then(node.modifiers.toComposeModifier(nodeId, dispatcher, theme))
        .testTag(nodeTestTag(nodeId))
    // The TextField wires its own key handling, because it has an editor to intercept and a
    // composition to protect; everything else routes keys here.
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
        // Drawing commands rather than children: the size comes from the modifier chain
        // and the commands are read from the node's one property.
        WidgetKind.Canvas -> HostCanvas(node, modifier, theme)
        // ScrollColumn is a column that scrolls without the Host windowing it, so every
        // child is materialised. Use LazyColumn when the list is long.
        //
        // LinearProgressIndicator is declared by an extension package rather than the core
        // schema, and is drawn like any built-in widget.
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

        // The containers, the overlays and the tab strip carry no appearance of their own:
        // each one names the kind of container it is and the design system decides what
        // that looks like.
        WidgetKind.Card ->
            HostContainerColumn(ContainerRole.Card, node, modifier, table, dispatcher, theme)

        WidgetKind.Surface ->
            HostContainerColumn(ContainerRole.Surface, node, modifier, table, dispatcher, theme)

        WidgetKind.TopAppBar -> HostTopAppBar(node, modifier, table, dispatcher, theme)
        WidgetKind.Dialog -> HostDialog(node, modifier, table, dispatcher, theme)
        WidgetKind.Menu -> HostMenu(node, modifier, table, dispatcher, theme)
        WidgetKind.Tabs -> HostTabs(node, modifier, table, dispatcher, theme)
        WidgetKind.Tooltip -> HostTooltip(node, modifier, table, dispatcher, theme)
        WidgetKind.LazyRow -> HostLazyRow(node, modifier, table, dispatcher)

        // A picture is one registered id. The bytes were read when the Host registered
        // them, so what a frame carries is the id and a lookup.
        WidgetKind.Image -> HostImage(node, modifier, table.assets, dispatcher)
        WidgetKind.Icon -> HostIcon(node, modifier, table.assets, dispatcher, theme)

        // The pickers carry a value, a range and a change handler. Which way of picking the
        // user gets, a calendar grid, a wheel, a dial or a flyout, is the design system's
        // decision, and there is no property that could ask for one of them.
        WidgetKind.DatePicker -> HostDatePicker(node, modifier, dispatcher, theme)
        WidgetKind.TimePicker -> HostTimePicker(node, modifier, dispatcher, theme)
        WidgetKind.Dropdown -> HostDropdown(node, modifier, table, dispatcher, theme)
    }
}

/**
 * The axis a widget stacks its children along, which is the axis a child's `Weight` is a
 * share of: a weight under a Column is a share of the height, a weight under a Row a share
 * of the width.
 */
internal enum class StackingAxis { Vertical, Horizontal, None }

/**
 * How the node that hosts a child lays its children out, so a child can tell whether a
 * `Weight` of its own has already decided one of its sizes.
 *
 * Only the layouts that really apply a weight are named here. Anything else, a Box or a
 * widget that draws its children without a weight scope, leaves a weighted child both of
 * its axes free.
 */
internal fun NodeTable.stackingAxis(nodeId: Int): StackingAxis = when (node(nodeId)?.widget) {
    WidgetKind.Column,
    WidgetKind.ScrollColumn,
    WidgetKind.Card,
    WidgetKind.Surface,
    -> StackingAxis.Vertical

    WidgetKind.Row,
    WidgetKind.TopAppBar,
    -> StackingAxis.Horizontal

    else -> StackingAxis.None
}

/**
 * Children of a Column: weight is applied here because `Modifier.weight` exists only inside
 * `ColumnScope`. Every layout that stacks its children vertically draws them through this,
 * so a weighted child is given its share wherever it is hosted and not only under a Column.
 */
@Composable
internal fun ColumnScope.Children(node: Node, table: NodeTable, dispatcher: EventDispatcher) {
    // `key` keeps each child's state attached to its Host node id across Move mutations.
    node.children.forEach { childId -> key(childId) { WeightedChild(childId, table, dispatcher) } }
}

@Composable
private fun ColumnScope.WeightedChild(childId: Int, table: NodeTable, dispatcher: EventDispatcher) {
    // Its own composable, so reading the child's modifier list subscribes this scope alone
    // and a `SetModifier` on one child cannot invalidate its siblings.
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
