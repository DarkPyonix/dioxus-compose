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
import androidx.compose.foundation.layout.heightIn
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
import dioxus.compose.design.glassSurface
import dioxus.compose.protocol.Modifier as ProtocolModifier
import dioxus.compose.protocol.SpaceRole
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.runtime.LocalWindowCaption
import dioxus.compose.ui.node.Node
import dioxus.compose.ui.node.NodeTable
import dioxus.compose.ui.node.Children
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
 * True when the Host filled this container itself, which then replaces the design
 * system's fill rather than sitting behind it.
 *
 * A background in the Modifier chain is drawn before the container style's fill, so the
 * style's fill covers it and the only part that survives is whatever falls outside the
 * style's shape. An application that asked for a panel in the reading ink got a ring of
 * that ink around a panel in the usual colour, with its text set in the surface colour
 * and therefore invisible.
 */
internal fun Node.setsOwnBackground(): Boolean =
    modifiers.any { it is ProtocolModifier.Background }

/**
 * Background, corner, stroke, resting height and inner padding for a container role.
 *
 * The node's own modifiers have already been applied to [modifier], so anything the Host
 * set wins: its `Background` replaces this one and its `Elevation` replaces the resting
 * height instead of adding to it. Everything it did not set still comes from the design
 * system, so a recoloured panel keeps that system's corner, stroke and inner padding.
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
    // A Host that named its own Background has already had it applied, so painting the
    // design system's fill here would cover it. The corner, stroke and inner padding still
    // come from the system: only the colour is the application's.
    if (node.setsOwnBackground()) return raised
    val material = style.material
    val filled = if (material == null) {
        raised.clip(shape).background(style.container, shape)
    } else {
        // A glass surface paints its own fill and its own lit edge, because the two are
        // one effect: the tint is what lets the backdrop through and the edge is what
        // gives the sheet thickness. Clipping still happens first so a child cannot spill
        // past the corner.
        raised.clip(shape).glassSurface(material, shape)
    }
    return filled
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
        Children(node, table, dispatcher)
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
    val caption = LocalWindowCaption.current
    Column(modifier) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                // The bar is the window's caption where the tree opens with one: at
                // least as tall as the strip the window buttons sit in, and starting
                // clear of them. Its content shares that row rather than stacking under
                // it, because a desktop toolbar sits on the same line as the window
                // buttons and a bar that began below them would be twice as tall for
                // nothing.
                .heightIn(min = caption.height)
                .containerDecoration(node, style, theme)
                .padding(
                    start = if (caption.buttonsAtStart) caption.buttonsWidth else 0.dp,
                    end = if (caption.buttonsAtStart) 0.dp else caption.buttonsWidth,
                ),
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
