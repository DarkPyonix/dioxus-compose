package dioxus.compose.foundation

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.distinctUntilChanged
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.ui.node.Node
import dioxus.compose.ui.node.NodeTable
import dioxus.compose.ui.node.RenderNode

/**
 * How many items are asked for beyond the visible range on each side (SPEC FR-8).
 *
 * The buffer lives here because the scroll position does (D5): the Renderer is the side that
 * knows what is on screen and therefore how far ahead to read. The Host answers a request
 * exactly, so `start` is the global index of the first child it sends back.
 */
const val LAZY_COLUMN_BUFFER: Int = 4

/** Height a not-yet-materialised slot stands at until a real item has been measured. */
private val ESTIMATED_ITEM_HEIGHT: Dp = 24.dp

/** The window of items to ask the Host for, given what is on screen (SPEC FR-8). */
internal fun requestedRange(
    firstVisible: Int,
    visibleCount: Int,
    itemCount: Int,
    buffer: Int = LAZY_COLUMN_BUFFER,
): Pair<Int, Int> {
    if (itemCount <= 0 || visibleCount <= 0) return 0 to 0
    val start = (firstVisible - buffer).coerceIn(0, itemCount)
    val end = (firstVisible + visibleCount + buffer).coerceIn(start, itemCount)
    return start to (end - start)
}

/**
 * A windowed list (SPEC FR-8).
 *
 * This is a real Compose `LazyColumn` over all `item_count` items, so the scrollbar and the
 * scroll distance match the whole list even though only a window of it exists as nodes.
 * Global index `i` draws the Host's child `i - windowStart`; outside the window the slot is
 * an empty `Spacer`, which is what fills the instant before the Host answers a scroll.
 *
 * Item identity comes from `item_key`, the string the Host puts on each `Box` wrapper, so an
 * item keeps its Compose state when the same range is materialised again.
 */
@Composable
internal fun HostLazyColumn(
    node: Node,
    modifier: Modifier,
    table: NodeTable,
    dispatcher: EventDispatcher,
    state: LazyListState = rememberLazyListState(),
) {
    val children = node.children
    val declaredCount =
        (node.property(PropertyKind.ItemCount) as? PropertyValue.Integer)?.value?.toInt()
    val itemCount = declaredCount ?: children.size
    val handlerId = node.handler(PropertyKind.OnRangeRequested)
    // The global index of `children[0]`, which is whatever was last asked for (D5).
    var windowStart by remember(node.id) { mutableIntStateOf(0) }
    // A slot outside the window has to stand as tall as a real item. A zero-height gap would
    // never fill the viewport, so Compose would measure all `item_count` slots and the list
    // would stop being windowed at all.
    var gapHeight by remember(node.id) { mutableStateOf(ESTIMATED_ITEM_HEIGHT) }

    LazyColumn(modifier = modifier, state = state) {
        items(
            count = itemCount,
            key = { index -> itemKey(table, children, index - windowStart, index) },
        ) { index ->
            val childId = children.getOrNull(index - windowStart)
            if (childId == null) {
                Spacer(Modifier.fillMaxWidth().height(gapHeight))
            } else {
                RenderNode(childId, table, dispatcher)
            }
        }
    }

    val density = LocalDensity.current
    LaunchedEffect(state, density) {
        // Once a real item has been measured, the gaps match it and scrolling stays honest.
        snapshotFlow {
            state.layoutInfo.visibleItemsInfo.firstOrNull { item -> item.size > 0 }?.size ?: 0
        }
            .distinctUntilChanged()
            .collect { size -> if (size > 0) gapHeight = with(density) { size.toDp() } }
    }

    if (handlerId != null) {
        LaunchedEffect(state, handlerId, itemCount, dispatcher) {
            snapshotFlow {
                requestedRange(
                    firstVisible = state.firstVisibleItemIndex,
                    visibleCount = state.layoutInfo.visibleItemsInfo.size,
                    itemCount = itemCount,
                )
            }
                // A scroll that stays inside the window costs no boundary call (section 5.1).
                .distinctUntilChanged()
                .collect { (start, count) ->
                    if (count <= 0) return@collect
                    // One transaction, so no frame sees the new children at the old offset.
                    Snapshot.withMutableSnapshot {
                        windowStart = start
                        dispatcher.dispatch(
                            HostEvent.RangeRequested(node.id, handlerId, start, count),
                        )
                    }
                }
        }
    }
}

/** The Compose item key for a global index: the Host's `item_key` when it has one. */
private fun itemKey(table: NodeTable, children: List<Int>, childIndex: Int, index: Int): Any {
    val childId = children.getOrNull(childIndex) ?: return "dioxus-gap-$index"
    val key = table.node(childId)?.text(PropertyKind.ItemKey).orEmpty()
    return key.ifEmpty { "dioxus-item-$index" }
}
