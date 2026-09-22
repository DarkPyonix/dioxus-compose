package dioxus.compose.foundation

import androidx.compose.foundation.background
import androidx.compose.ui.graphics.Brush
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
import androidx.compose.runtime.DisposableEffect
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
import dioxus.compose.design.NavigationExtent
import dioxus.compose.design.NavigationIndicator
import dioxus.compose.design.NavigationPresentation
import dioxus.compose.design.NavigationStyle
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.IconRole
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.runtime.LocalSystemBars
import dioxus.compose.runtime.LocalWindowSizeClass
import dioxus.compose.ui.node.Node
import dioxus.compose.ui.paintProp
import dioxus.compose.ui.node.NodeTable
import dioxus.compose.ui.node.RenderNode
import dioxus.compose.ui.node.nodeTestTag
import dioxus.compose.ui.role
import dioxus.compose.ui.textStyle

/** How thick the bar Fluent draws along the leading edge of the selected row is. */
private val LEADING_BAR_THICKNESS = 3.dp

/**
 * How long that bar is.
 *
 * A fixed length rather than a fraction of the row: the rows sit in a scrolling column,
 * so the height they are offered is unbounded and a fraction of it comes out as nothing.
 */
private val LEADING_BAR_EXTENT = 20.dp

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

    // Where the platform has a strip of its own worth more than the one drawn here, it
    // gets the destinations and this side draws only the screen. The question is asked of
    // whatever is installed, and on the platforms where nothing is, the answer is no and
    // the code below is unchanged.
    //
    // Only the bar is offered. A rail and a drawer are laid out beside the screen and take
    // their width out of it, so handing them to a chrome that sits outside the Compose
    // surface would leave the screen the full window wide with the strip on top of it.
    //
    // And only a navigation that is a root of the tree. The platform's chrome belongs to
    // the window, and there is one of it: a navigation nested inside some part of the
    // screen would take the window's bar away from whatever owns it, and two of them would
    // take turns. A nested one keeps the bar drawn here, where it can sit inside the part
    // of the screen it actually belongs to.
    //
    // And only where Apple's design language was asked for. The chrome the shell stands up
    // is Apple's, drawn by Apple; putting it under an application that asked for Material 3
    // or Fluent would answer a question nobody asked. An application on this platform that
    // said nothing gets Apple's anyway, because the default theme follows the platform.
    val apple = theme.system == DesignSystem.Cupertino || theme.system == DesignSystem.LiquidGlass
    val offered = style.presentation == NavigationPresentation.Bar &&
        node.id in table.roots &&
        apple
    val shell = platformNavigationShell?.takeIf { it.drawsStrip && offered }

    // A message is drawn over the whole window, so it has to be told what the bar along
    // the bottom is using or it would cover the destinations.
    val barHeight = when {
        shell != null -> shell.stripHeight.dp
        // Including the strip it grows into, because a message placed above the bar has
        // to clear what is on the screen rather than what the design system nominally
        // asked for.
        style.presentation == NavigationPresentation.Bar ->
            style.barHeight + LocalSystemBars.current.bottom
        else -> 0.dp
    }
    DisposableEffect(table, barHeight) {
        table.insets.bottom = barHeight
        onDispose { table.insets.bottom = 0.dp }
    }

    if (shell != null) {
        val handed = destinations.mapNotNull { childId ->
            table.node(childId)?.let { destination ->
                ShellDestination(
                    nodeId = childId,
                    label = destination.text(PropertyKind.Text),
                    icon = destination.role(PropertyKind.Icon, IconRole.entries.toTypedArray()),
                    enabled = destination.flag(PropertyKind.Enabled, default = true),
                )
            }
        }
        // Two effects rather than one. Handing the destinations over happens again every
        // time they or the selection change, and tapping a destination changes the
        // selection, so putting the teardown in the same effect would take the strip down
        // and put it back up on every tap.
        DisposableEffect(shell, handed, selected) {
            shell.present(handed, selected) { index ->
                handed.getOrNull(index)?.let { choose(index, it.nodeId) }
            }
            onDispose {}
        }
        DisposableEffect(shell) {
            onDispose { shell.dismiss() }
        }
        // The strip along the bottom is not padded away: the screen runs under it, which
        // is what gives a bar made of glass something to refract. A title bar is padded
        // away, because it is opaque enough at the top that content under it is lost.
        Box(modifier.padding(top = shell.titleHeight.dp)) {
            Screen(content, table, dispatcher)
        }
        return
    }

    when (style.presentation) {
        NavigationPresentation.Bar -> Column(modifier.navigationBackdrop(style)) {
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
                    .background(style.container)
                    // The strip the system's gesture bar sits in belongs to this bar: its
                    // own colour runs to the bottom edge of the window and the
                    // destinations sit above the gesture bar rather than under it. A bar
                    // that stopped short would leave a band of the system's own
                    // background below it that no other application on the device has.
                    .height(style.barHeight + LocalSystemBars.current.bottom)
                    .padding(bottom = LocalSystemBars.current.bottom),
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

        NavigationPresentation.Rail, NavigationPresentation.Drawer -> Row(
            modifier.navigationBackdrop(style),
        ) {
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
    val search = role == IconRole.Search &&
        presentation == NavigationPresentation.Drawer &&
        style.searchContainer != null
    // The design system decides what "selected" looks like, unless the node names a
    // colour itself. A unified sample is what needs the exception: its reference bar is
    // white icons on black with no accent anywhere, and asking the active system instead
    // puts its own accent on the selected one.
    //
    // The named colour is used for both states. A destination that says what colour it is
    // is saying it about itself, not about half of itself, and a sample wanting the two
    // states apart says so by giving each destination its own colour.
    val named = node.paintProp(PropertyKind.Color)?.let { theme.color(it) }
    val tint = named ?: if (selected && !search) style.selectedContent else style.content
    val showLabel = label.isNotEmpty() &&
        (presentation != NavigationPresentation.Rail || style.labelInRail)
    val pill = selected && !search && style.indicatorKind == NavigationIndicator.Pill
    val bar = selected && !search && style.indicatorKind == NavigationIndicator.LeadingEdgeBar

    Box(
        modifier
            .testTag(nodeTestTag(node.id))
            .then(
                if (search) {
                    Modifier
                        .clip(style.indicatorShape)
                        .background(style.searchContainer)
                } else {
                    Modifier
                },
            )
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
                indicatorSize(presentation, style.indicatorExtent)
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

private fun Modifier.navigationBackdrop(style: NavigationStyle): Modifier {
    val start = style.pageGradientStart ?: return this
    val end = style.pageGradientEnd ?: return this
    return background(Brush.verticalGradient(listOf(start, end)))
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

/**
 * How big the mark behind the selected destination is.
 *
 * A system that covers the whole destination is matched to it rather than given a size,
 * so the fill reaches the label the design system coloured to read on it. The systems
 * that mark the icon alone keep a fixed mark: a drawer's spans the row, a bar's or a
 * rail's sits behind the icon with the label under it.
 */
@Composable
private fun BoxScope.indicatorSize(
    presentation: NavigationPresentation,
    extent: NavigationExtent,
): Modifier = when {
    extent == NavigationExtent.Destination -> Modifier.matchParentSize()
    presentation == NavigationPresentation.Drawer -> Modifier.fillMaxWidth().height(40.dp)
    else -> Modifier.size(56.dp, 34.dp)
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
                .width(LEADING_BAR_EXTENT)
                .height(LEADING_BAR_THICKNESS)
                .clip(style.indicatorShape)
                .background(style.indicator),
        )
    } else {
        Box(
            Modifier
                .align(Alignment.CenterStart)
                .width(LEADING_BAR_THICKNESS)
                .height(LEADING_BAR_EXTENT)
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
