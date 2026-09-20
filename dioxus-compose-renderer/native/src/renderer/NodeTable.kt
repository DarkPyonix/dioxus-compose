package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import org.thisisthepy.dioxus.compose.protocol.Mutation
import org.thisisthepy.dioxus.compose.protocol.PropertyKind
import org.thisisthepy.dioxus.compose.protocol.PropertyValue
import org.thisisthepy.dioxus.compose.protocol.WidgetKind
import org.thisisthepy.dioxus.compose.protocol.Modifier as ProtocolModifier

/** Text the Host pushed into a TextField with `SetText` (SPEC FR-5). */
data class HostText(
    val text: String,
    val selectionStart: Int,
    val selectionEnd: Int,
    /** Increments on every `SetText`, so re-sending the same text still applies. */
    val revision: Long,
)

/**
 * One interpreted node. Each field is its own snapshot state, so a `SetProp` on one node
 * invalidates only the composables that read that node (SPEC FR-4).
 */
class Node internal constructor(val id: Int, val widget: WidgetKind) {
    internal val props = mutableStateMapOf<PropertyKind, PropertyValue>()
    internal val modifiers = mutableStateListOf<ProtocolModifier>()
    internal val children = mutableStateListOf<Int>()
    internal var parentId: Int = NodeTable.ROOT_ID
    internal var hostText by mutableStateOf<HostText?>(null)

    fun property(kind: PropertyKind): PropertyValue? = props[kind]

    fun text(kind: PropertyKind, default: String = ""): String =
        (props[kind] as? PropertyValue.Text)?.value ?: default

    fun flag(kind: PropertyKind, default: Boolean): Boolean =
        (props[kind] as? PropertyValue.Bool)?.value ?: default

    /** Handler ids arrive as integer property values (SPEC FR-3). */
    fun handler(kind: PropertyKind): Long? = (props[kind] as? PropertyValue.Integer)?.value
}

/** A protocol violation that must become a `ProtocolError` event, never a crash (NFR-7). */
data class TableError(val code: Int, val message: String) {
    companion object {
        const val UNKNOWN_NODE = 1
        const val DUPLICATE_NODE = 2
        const val UNSUPPORTED_PROPERTY = 3
        const val INVALID_INDEX = 4
    }
}

/**
 * The interpreted node tree (SPEC FR-1).
 *
 * A whole batch is applied inside one `Snapshot.withMutableSnapshot` transaction by
 * [DioxusHost], so no intermediate tree state is ever drawn (SPEC PR-2).
 */
class NodeTable {
    private val nodes = mutableStateMapOf<Int, Node>()
    private val rootChildren = mutableStateListOf<Int>()
    private val errors = mutableListOf<TableError>()
    private var revision = 0L

    /** Top-level nodes, in creation order until the Host parents them. */
    val roots: List<Int> get() = rootChildren

    fun node(id: Int): Node? = nodes[id]

    /** Errors collected while applying a batch; drained after the transaction commits. */
    internal fun drainErrors(): List<TableError> {
        if (errors.isEmpty()) return emptyList()
        val drained = errors.toList()
        errors.clear()
        return drained
    }

    fun apply(mutation: Mutation) {
        when (mutation) {
            is Mutation.Create -> create(mutation)
            is Mutation.SetProp -> setProp(mutation)
            is Mutation.SetModifier -> setModifier(mutation)
            is Mutation.Insert -> insert(mutation.parentId, mutation.nodeId, mutation.index)
            is Mutation.Move -> insert(mutation.parentId, mutation.nodeId, mutation.index)
            is Mutation.Remove -> remove(mutation.nodeId)
            is Mutation.SetText -> setText(mutation)
        }
    }

    private fun create(mutation: Mutation.Create) {
        if (nodes.containsKey(mutation.nodeId)) {
            fail(TableError.DUPLICATE_NODE, "node ${mutation.nodeId} already exists")
            return
        }
        nodes[mutation.nodeId] = Node(mutation.nodeId, mutation.widget)
        // A node the Host never inserts stays a root. The Host has no mutation that attaches
        // the application root, so the first created node is what the Renderer draws.
        // SPEC-GAP: FR-1 defines no explicit root attachment; roots are inferred here.
        rootChildren.add(mutation.nodeId)
    }

    private fun setProp(mutation: Mutation.SetProp) {
        val node = nodes[mutation.nodeId] ?: return fail(
            TableError.UNKNOWN_NODE,
            "SetProp for unknown node ${mutation.nodeId}",
        )
        if (!supportsProperty(node.widget, mutation.property)) {
            // FR-2: an out-of-schema property is reported and skipped, never applied.
            fail(
                TableError.UNSUPPORTED_PROPERTY,
                "${node.widget} does not support ${mutation.property}",
            )
            return
        }
        if (mutation.value is PropertyValue.None) {
            node.props.remove(mutation.property)
        } else {
            node.props[mutation.property] = mutation.value
        }
    }

    private fun setModifier(mutation: Mutation.SetModifier) {
        val node = nodes[mutation.nodeId] ?: return fail(
            TableError.UNKNOWN_NODE,
            "SetModifier for unknown node ${mutation.nodeId}",
        )
        if (mutation.index < 0 || mutation.index > MAX_MODIFIERS) {
            fail(TableError.INVALID_INDEX, "modifier index ${mutation.index} out of range")
            return
        }
        while (node.modifiers.size <= mutation.index) {
            node.modifiers.add(ProtocolModifier.Empty)
        }
        node.modifiers[mutation.index] = mutation.modifier
    }

    private fun insert(parentId: Int, nodeId: Int, index: Int) {
        if (!nodes.containsKey(nodeId)) {
            fail(TableError.UNKNOWN_NODE, "Insert of unknown node $nodeId")
            return
        }
        if (parentId != ROOT_ID && !nodes.containsKey(parentId)) {
            fail(TableError.UNKNOWN_NODE, "Insert into unknown parent $parentId")
            return
        }
        detach(nodeId)
        val siblings = childrenOf(parentId) ?: return
        // `index` is unsigned on the wire; u32::MAX means "append".
        val position = if (index.toLong() and 0xFFFF_FFFFL > siblings.size.toLong()) {
            siblings.size
        } else {
            index
        }
        siblings.add(position, nodeId)
        nodes[nodeId]?.parentId = parentId
    }

    private fun remove(nodeId: Int) {
        if (!nodes.containsKey(nodeId)) {
            fail(TableError.UNKNOWN_NODE, "Remove of unknown node $nodeId")
            return
        }
        detach(nodeId)
        removeSubtree(nodeId)
    }

    private fun removeSubtree(nodeId: Int) {
        val node = nodes.remove(nodeId) ?: return
        node.children.toList().forEach(::removeSubtree)
    }

    private fun setText(mutation: Mutation.SetText) {
        val node = nodes[mutation.nodeId] ?: return fail(
            TableError.UNKNOWN_NODE,
            "SetText for unknown node ${mutation.nodeId}",
        )
        if (node.widget != WidgetKind.TextField) {
            fail(TableError.UNSUPPORTED_PROPERTY, "SetText on ${node.widget}")
            return
        }
        revision += 1
        node.hostText = HostText(
            text = mutation.text,
            selectionStart = mutation.selectionStart,
            selectionEnd = mutation.selectionEnd,
            revision = revision,
        )
    }

    private fun detach(nodeId: Int) {
        val parent = nodes[nodeId]?.parentId ?: ROOT_ID
        childrenOf(parent)?.remove(nodeId)
        rootChildren.remove(nodeId)
        nodes[nodeId]?.parentId = ROOT_ID
    }

    private fun childrenOf(parentId: Int): MutableList<Int>? =
        if (parentId == ROOT_ID) rootChildren else nodes[parentId]?.children

    private fun fail(code: Int, message: String) {
        errors += TableError(code, message)
    }

    companion object {
        /** Node id 0 is the Host's "no node" sentinel, so it names the virtual root. */
        const val ROOT_ID: Int = 0
        private const val MAX_MODIFIERS = 64

        internal fun supportsProperty(widget: WidgetKind, property: PropertyKind): Boolean =
            when (property) {
                // Event properties carry handler ids and are valid on any widget.
                PropertyKind.OnClick,
                PropertyKind.OnValueChange,
                PropertyKind.OnSubmit,
                PropertyKind.OnFocusLost,
                -> true

                PropertyKind.Text ->
                    widget == WidgetKind.Text ||
                        widget == WidgetKind.Button ||
                        widget == WidgetKind.TextField

                // SPEC-GAP: SpacerProps has width and height in the Rust schema, but there
                // are no matching PropertyKind variants, so a Spacer can only be sized with
                // modifiers. Either the schema gains the properties or SPEC FR-2 should say
                // that Spacer is modifier-sized.
                PropertyKind.Placeholder -> widget == WidgetKind.TextField
                PropertyKind.Multiline -> widget == WidgetKind.TextField
                PropertyKind.Enabled -> widget != WidgetKind.Spacer
            }
    }
}
