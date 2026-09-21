package dioxus.compose.foundation

import androidx.compose.foundation.background
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dioxus.compose.design.NavigationIndicator
import dioxus.compose.design.NavigationPresentation
import dioxus.compose.design.NavigationStyle
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.IconRole
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.runtime.LocalWindowSizeClass
import dioxus.compose.ui.node.Node
import dioxus.compose.ui.node.NodeTable
import dioxus.compose.ui.node.RenderNode
import dioxus.compose.ui.node.nodeTestTag
import dioxus.compose.ui.role
import dioxus.compose.ui.textStyle

/** How thick the bar Fluent draws along the leading edge of the selected row is. */
private val LEADING_BAR_THICKNESS = 3.dp

/** How long that bar is, as a fraction of the row it marks. */
private const val LEADING_BAR_EXTENT = 0.6f

/** How faint a destination that cannot be chosen is drawn. */
private const val DISABLED_ALPHA = 0.38f

/**
 * A set of destinations and the screen they lead to.
 *
 * The children that are `NavigationItem` are the destinations and everything else is the
 * content. Splitting by kind rather than by a count the Host sends means a destination that
 * only exists under some condition cannot put the two lists out of step.
 *
 * Which of the three presentations this is comes from the design system, which is asked
 * about the size class the window is in. That is the whole of the responsive claim: the
 * Host sent one declaration and never learned how wide the window was.
 *
 * The selection is this side's state. `SelectedIndex` seeds it and moves it when the change
 * came from the Host; choosing a destination moves it here and then reports it as that
 * destination's own click, so switching screens costs one event and no rebuilt strip.
 */
@Composable
internal fun HostNavigation(
    node: Node,
    modifier: Modifier,
    table: NodeTable,
    dispatcher: EventDispatcher,
    theme: ResolvedTheme,
) {
    val style = theme.rules.navigation(LocalWindowSizeClass.current, theme)
    val destinations = node.children.filter { table.node(it)?.widget == WidgetKind.NavigationItem }
    val content = node.children.filter { table.node(it)?.widget != WidgetKind.NavigationItem }
    val fromHost = (node.property(PropertyKind.SelectedIndex) as? PropertyValue.Integer)
        ?.value
        ?.toInt()
        ?: 0
    var selected by remember(node.id) { mutableIntStateOf(fromHost) }
    LaunchedEffect(node.id, fromHost) { selected = fromHost }

    val choose: (Int, Int) -> Unit = { index, childId ->
        selected = index
        val handlerId = table.node(childId)?.handler(PropertyKind.OnClick)
        if (handlerId != null) {
            dispatcher.dispatch(HostEvent.Clicked(childId, handlerId))
        }
    }

    when (style.presentation) {
        NavigationPresentation.Bar -> Column(modifier) {
            Box(Modifier.fillMaxWidth().weight(1f)) {
                Screen(content, table, dispatcher)
            }
            style.separator?.let { line ->
                Box(Modifier.fillMaxWidth().height(1.dp).background(line))
            }
            Row(
                Modifier
                    .testTag(navigationStripTestTag(node.id))
                    .fillMaxWidth()
                    .height(style.barHeight)
                    .background(style.container),
                horizontalArrangement = Arrangement.SpaceEvenly,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                destinations.forEachIndexed { index, childId ->
                    key(childId) {
                        table.node(childId)?.let { destination ->
                            Destination(
                                node = destination,
                                selected = index == selected,
                                style = style,
                                presentation = style.presentation,
                                theme = theme,
                                modifier = Modifier.weight(1f),
                                onSelect = { choose(index, childId) },
                            )
                        }
                    }
                }
            }
        }

        NavigationPresentation.Rail, NavigationPresentation.Drawer -> Row(modifier) {
            val width = if (style.presentation == NavigationPresentation.Rail) {
                style.railWidth
            } else {
                style.drawerWidth
            }
            Column(
                Modifier
                    .testTag(navigationStripTestTag(node.id))
                    .width(width)
                    .fillMaxHeight()
                    .background(style.container)
                    .verticalScroll(rememberScrollState())
                    .padding(vertical = style.itemPadding),
                verticalArrangement = Arrangement.spacedBy(style.itemSpacing),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                destinations.forEachIndexed { index, childId ->
                    key(childId) {
                        table.node(childId)?.let { destination ->
                            Destination(
                                node = destination,
                                selected = index == selected,
                                style = style,
                                presentation = style.presentation,
                                theme = theme,
                                modifier = Modifier.fillMaxWidth(),
                                onSelect = { choose(index, childId) },
                            )
                        }
                    }
                }
            }
            style.separator?.let { line ->
                Box(Modifier.width(1.dp).fillMaxHeight().background(line))
            }
            Box(Modifier.fillMaxHeight().weight(1f)) {
                Screen(content, table, dispatcher)
            }
        }
    }
}

/** The part of the tree that is not a destination: the screen the selection leads to. */
@Composable
private fun Screen(content: List<Int>, table: NodeTable, dispatcher: EventDispatcher) {
    content.forEach { childId -> key(childId) { RenderNode(childId, table, dispatcher) } }
}

/**
 * One destination.
 *
 * A bar and a rail stack the label under the icon; a drawer sets it beside, and is the only
 * one wide enough to do so. The mark on the selected one is the design system's: a filled
 * pill, a bar along the leading edge, or nothing but the colour of the label.
 */
@Composable
internal fun Destination(
    node: Node,
    selected: Boolean,
    style: NavigationStyle,
    presentation: NavigationPresentation,
    theme: ResolvedTheme,
    modifier: Modifier,
    onSelect: () -> Unit,
) {
    val label = node.text(PropertyKind.Text)
    val role = node.role(PropertyKind.Icon, IconRole.entries.toTypedArray())
    val enabled = node.flag(PropertyKind.Enabled, default = true)
    val tint = if (selected) style.selectedContent else style.content
    val showLabel = label.isNotEmpty() &&
        (presentation != NavigationPresentation.Rail || style.labelInRail)
    val pill = selected && style.indicatorKind == NavigationIndicator.Pill
    val bar = selected && style.indicatorKind == NavigationIndicator.LeadingEdgeBar

    Box(
        modifier
            .testTag(nodeTestTag(node.id))
            // `selectable` rather than `clickable`: one of a set is chosen, and saying so
            // is what puts "selected" in the accessibility tree instead of leaving a
            // screen reader to announce every destination identically.
            .selectable(selected = selected, enabled = enabled, role = Role.Tab, onClick = onSelect)
            .alpha(if (enabled) 1f else DISABLED_ALPHA),
        contentAlignment = Alignment.Center,
    ) {
        // The mark is drawn behind the content rather than around it, so selecting a
        // destination never changes how much room it takes and the strip does not shift.
        if (pill) {
            Box(
                Modifier
                    .indicatorSize(presentation)
                    .clip(style.indicatorShape)
                    .background(style.indicator),
            )
        }
        if (bar) {
            LeadingBar(presentation, style)
        }
        if (presentation == NavigationPresentation.Drawer) {
            Row(
                Modifier.fillMaxWidth().padding(
                    horizontal = style.itemPadding,
                    vertical = style.itemPadding,
                ),
                horizontalArrangement = Arrangement.spacedBy(style.itemSpacing),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                DestinationIcon(role, tint, theme)
                DestinationLabel(node, label, showLabel, tint, style, theme)
            }
        } else {
            Column(
                Modifier.padding(horizontal = style.itemPadding, vertical = style.itemPadding),
                verticalArrangement = Arrangement.spacedBy(style.itemSpacing),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                DestinationIcon(role, tint, theme)
                DestinationLabel(node, label, showLabel, tint, style, theme)
            }
        }
    }
}

@Composable
private fun DestinationIcon(role: IconRole?, tint: Color, theme: ResolvedTheme) {
    if (role != null) {
        RoleIcon(role, tint, theme, Modifier)
    }
}

@Composable
private fun DestinationLabel(
    node: Node,
    label: String,
    show: Boolean,
    tint: Color,
    style: NavigationStyle,
    theme: ResolvedTheme,
) {
    if (!show) return
    BasicText(
        text = label,
        style = node.textStyle(theme, style.typeRole).copy(color = tint),
    )
}

/** A drawer's pill spans the row; a bar's or a rail's sits behind the icon and label. */
private fun Modifier.indicatorSize(presentation: NavigationPresentation): Modifier =
    if (presentation == NavigationPresentation.Drawer) {
        this.fillMaxWidth().height(40.dp)
    } else {
        this.size(56.dp, 34.dp)
    }

/** Fluent's mark: a short bar along the edge the row starts at. */
@Composable
private fun BoxScope.LeadingBar(
    presentation: NavigationPresentation,
    style: NavigationStyle,
) {
    if (presentation == NavigationPresentation.Bar) {
        Box(
            Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth(LEADING_BAR_EXTENT)
                .height(LEADING_BAR_THICKNESS)
                .clip(style.indicatorShape)
                .background(style.indicator),
        )
    } else {
        Box(
            Modifier
                .align(Alignment.CenterStart)
                .width(LEADING_BAR_THICKNESS)
                .fillMaxHeight(LEADING_BAR_EXTENT)
                .clip(style.indicatorShape)
                .background(style.indicator),
        )
    }
}

/** A destination outside a `Navigation` still draws, unselected and unmarked. */
@Composable
internal fun HostNavigationItem(
    node: Node,
    modifier: Modifier,
    theme: ResolvedTheme,
) {
    val style = theme.rules.navigation(LocalWindowSizeClass.current, theme)
    Destination(
        node = node,
        selected = false,
        style = style.copy(indicator = Color.Transparent),
        presentation = NavigationPresentation.Drawer,
        theme = theme,
        modifier = modifier,
        onSelect = {},
    )
}

/** Test tag of the strip of destinations, which is the half that changes shape. */
fun navigationStripTestTag(nodeId: Int): String = "${nodeTestTag(nodeId)}-strip"
