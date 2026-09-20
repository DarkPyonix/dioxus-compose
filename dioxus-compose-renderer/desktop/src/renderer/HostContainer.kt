package dioxus.compose.foundation

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.dp
import dioxus.compose.design.ContainerRole
import dioxus.compose.design.ContainerStyle
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.protocol.Modifier as ProtocolModifier
import dioxus.compose.protocol.SpaceRole
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.ui.node.Node
import dioxus.compose.ui.node.NodeTable
import dioxus.compose.ui.node.RenderNode
import dioxus.compose.ui.resolvedShape
import dioxus.compose.ui.weightOf

/**
 * The corner this container is drawn with: the one the Host asked for if it asked, and the
 * design system's corner for this kind of container otherwise.
 */
internal fun Node.containerShape(style: ContainerStyle, theme: ResolvedTheme): Shape =
    if (modifiers.any { it is ProtocolModifier.Shape || it is ProtocolModifier.ShapeRole }) {
        modifiers.resolvedShape(theme)
    } else {
        style.shape
    }

/** True when the Host set a resting height of its own, which then replaces the default. */
internal fun Node.setsOwnElevation(): Boolean =
    modifiers.any { it is ProtocolModifier.Elevation }

/**
 * Background, corner, stroke, resting height and inner padding for a container role.
 *
 * The node's own modifiers have already been applied to [modifier], so anything the Host
 * set wins: its `Background` paints over this one, and its `Elevation` replaces the resting
 * height instead of adding to it.
 */
@Composable
internal fun Modifier.containerDecoration(
    node: Node,
    style: ContainerStyle,
    theme: ResolvedTheme,
): Modifier {
    val shape = node.containerShape(style, theme)
    val raised = if (node.setsOwnElevation()) {
        this
    } else {
        theme.rules.elevation(this, style.elevation, shape, theme)
    }
    return raised
        .clip(shape)
        .background(style.container, shape)
        .then(
            if (style.borderWidth.value > 0f) {
                Modifier.border(style.borderWidth, style.borderColor, shape)
            } else {
                Modifier
            },
        )
        .padding(horizontal = style.horizontalPadding, vertical = style.verticalPadding)
}

/**
 * A grouped container. Its children are its content, stacked top to bottom.
 *
 * Nothing about how it looks is decided here: [ContainerRole.Card] and
 * [ContainerRole.Surface] differ only in what the design system says each one means.
 */
@Composable
internal fun HostContainerColumn(
    role: ContainerRole,
    node: Node,
    modifier: Modifier,
    table: NodeTable,
    dispatcher: EventDispatcher,
    theme: ResolvedTheme,
) {
    val style = theme.rules.container(role, theme)
    Column(modifier.containerDecoration(node, style, theme)) {
        node.children.forEach { childId -> key(childId) { RenderNode(childId, table, dispatcher) } }
    }
}

/**
 * The bar across the top of a screen: its children laid out left to right, vertically
 * centred, with whatever rule the design system draws under it.
 *
 * A child with a `Weight` modifier takes its share of the bar, which is how a title pushes
 * the actions to the far end.
 */
@Composable
internal fun HostTopAppBar(
    node: Node,
    modifier: Modifier,
    table: NodeTable,
    dispatcher: EventDispatcher,
    theme: ResolvedTheme,
) {
    val style = theme.rules.container(ContainerRole.TopAppBar, theme)
    Column(modifier) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .containerDecoration(node, style, theme),
            horizontalArrangement = Arrangement.spacedBy(theme.space(SpaceRole.Sm)),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            node.children.forEach { childId -> key(childId) { BarChild(childId, table, dispatcher) } }
        }
        val separator = style.separator
        if (separator != null) {
            Box(Modifier.fillMaxWidth().height(HAIRLINE).background(separator))
        }
    }
}

@Composable
private fun RowScope.BarChild(childId: Int, table: NodeTable, dispatcher: EventDispatcher) {
    val weight = table.node(childId)?.modifiers?.weightOf()
    RenderNode(
        childId,
        table,
        dispatcher,
        if (weight == null) Modifier else Modifier.weight(weight),
    )
}

/** The thinnest line that still draws on every density. */
private val HAIRLINE = 1.dp
