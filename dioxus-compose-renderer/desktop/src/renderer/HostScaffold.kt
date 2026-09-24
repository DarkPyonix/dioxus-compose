package dioxus.compose.foundation

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dioxus.compose.design.NavigationPresentation
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.SlotRole
import dioxus.compose.protocol.SpaceRole
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.protocol.WindowSizeClass
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.runtime.LocalWindowSizeClass
import dioxus.compose.ui.node.Node
import dioxus.compose.ui.node.NodeTable
import dioxus.compose.ui.node.RenderNode

/**
 * How a screen's frame is arranged at one window width.
 *
 * A pure description so it can be tested without a window. What the frame is made of
 * comes from the slots an application filled; what it becomes is this.
 */
internal enum class ScaffoldFrame {
    /** Bars across the top and the bottom, the page between them. A phone. */
    Stacked,

    /** The destinations run down the leading edge beside the page. A tablet or a desktop. */
    SideBySide,
}

/**
 * Which frame a window of this width gets.
 *
 * Read off the navigation presentation rather than decided again here. The design system
 * has already answered "bar, rail or drawer" for this width, and a frame that disagreed
 * with it would put a rail down the side and still leave room for a bar at the bottom.
 */
internal fun scaffoldFrame(presentation: NavigationPresentation): ScaffoldFrame =
    when (presentation) {
        NavigationPresentation.Bar -> ScaffoldFrame.Stacked
        else -> ScaffoldFrame.SideBySide
    }

/**
 * Where the one action a screen is about goes.
 *
 * It floats over the page where there is room below for a thumb to reach it, and moves
 * into the top bar where the window is wide and the pointer is already up there. Compact
 * is the phone case and the only one that floats.
 */
internal fun floatingActionFloats(sizeClass: WindowSizeClass): Boolean =
    sizeClass == WindowSizeClass.Compact

/**
 * The screen's frame.
 *
 * The application filled slots and said nothing about what they become. This is where
 * that is answered, from the design system and the width the window was measured at, the
 * same two things `Navigation` already answers "bar, rail or drawer" from.
 *
 * The page is laid out clear of whatever the other slots took, which is the part an
 * application must not compute for itself: it would be wrong the moment the frame chose
 * differently.
 */
@Composable
internal fun HostScaffold(
    node: Node,
    modifier: Modifier,
    table: NodeTable,
    dispatcher: EventDispatcher,
    theme: ResolvedTheme,
) {
    val sizeClass = LocalWindowSizeClass.current
    val frame = scaffoldFrame(theme.rules.navigation(sizeClass, theme).presentation)

    val slots = node.children.mapNotNull { id ->
        val child = table.node(id) ?: return@mapNotNull null
        if (child.widget != WidgetKind.ScaffoldSlot) return@mapNotNull null
        slotOf(child)?.let { it to child }
    }.toMap()

    val topBar = slots[SlotRole.TopBar]
    val bottomBar = slots[SlotRole.BottomBar]
    val floatingAction = slots[SlotRole.FloatingAction]
    val content = slots[SlotRole.Content]

    @Composable
    fun slot(child: Node?, slotModifier: Modifier = Modifier) {
        if (child == null) return
        child.children.forEach { grandchild ->
            RenderNode(grandchild, table, dispatcher, slotModifier)
        }
    }

    Column(modifier.fillMaxSize()) {
        // The top bar spans the window at every width, above the destinations as well as
        // above the page. A bar that stopped at the rail would read as a panel heading
        // rather than as the window's own bar, and on the one platform that hands its
        // caption to the application it is the window's caption.
        Row(Modifier.fillMaxWidth()) { slot(topBar) }

        Row(Modifier.fillMaxWidth().weight(1f)) {
            if (frame == ScaffoldFrame.SideBySide) {
                slot(bottomBar)
            }
            Box(Modifier.weight(1f).fillMaxSize()) {
                slot(content, Modifier.fillMaxSize())
                if (floatingAction != null && floatingActionFloats(sizeClass)) {
                    Box(
                        Modifier
                            .align(Alignment.BottomEnd)
                            .padding(theme.space(SpaceRole.Lg)),
                    ) { slot(floatingAction) }
                }
            }
            // Wide windows put it at the trailing end of the top bar's row instead, which
            // is where a pointer already is. Drawn here rather than inside the bar so the
            // bar stays whatever the application put in it.
            if (floatingAction != null && !floatingActionFloats(sizeClass)) {
                Box(Modifier.align(Alignment.Top).padding(theme.space(SpaceRole.Sm))) {
                    slot(floatingAction)
                }
            }
        }

        if (frame == ScaffoldFrame.Stacked) {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.Center,
            ) { slot(bottomBar) }
        }
    }
}

/** Which slot a `ScaffoldSlot` node says it is. */
internal fun slotOf(node: Node): SlotRole? =
    (node.property(PropertyKind.Slot) as? PropertyValue.Integer)
        ?.value
        ?.toInt()
        ?.let { tag -> SlotRole.entries.getOrNull(tag - 1) }
