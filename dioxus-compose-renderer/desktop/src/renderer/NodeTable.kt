package dioxus.compose.ui.node

import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.Theme
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.protocol.Modifier as ProtocolModifier

/**
 * Text the Host pushed into a TextField with `SetText`.
 *
 * The field is uncontrolled: the Renderer owns the edit value and the composition state, and
 * this is the only way the Host changes it. If an IME composition is in progress the change
 * waits until the composition commits, because replacing text mid-composition destroys the
 * syllable being assembled.
 */
data class HostText(
    val text: String,
    val selectionStart: Int,
    val selectionEnd: Int,
    /** Increments on every `SetText`, so re-sending the same text still applies. */
    val revision: Long,
)

/**
 * One interpreted node. Each field is its own snapshot state, so a `SetProp` on one node
 * invalidates only the composables that read that node.
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

    /** Handler ids arrive as integer property values. */
    fun handler(kind: PropertyKind): Long? = (props[kind] as? PropertyValue.Integer)?.value

    fun number(kind: PropertyKind): Float? = (props[kind] as? PropertyValue.Float)?.value
}

/** A protocol violation that must become a `ProtocolError` event, never a crash. */
data class TableError(val code: Int, val message: String) {
    companion object {
        const val UNKNOWN_NODE = 1
        const val DUPLICATE_NODE = 2
        const val UNSUPPORTED_PROPERTY = 3
        const val INVALID_INDEX = 4
    }
}

/**
 * The interpreted node tree.
 *
 * A whole batch is applied inside one `Snapshot.withMutableSnapshot` transaction by
 * [DioxusHost], so no intermediate tree state is ever drawn.
 */
class NodeTable {
    private val nodes = mutableStateMapOf<Int, Node>()
    private val rootChildren = mutableStateListOf<Int>()
    private val errors = mutableListOf<TableError>()
    private var revision = 0L

    /** Null until the Host sends its first `SetTheme` record. */
    var theme: Theme? by mutableStateOf(null)
        private set

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
            is Mutation.AppendText -> appendText(mutation)
            // One record changes the whole tree's appearance. `DioxusContent`
            // resolves it into tokens and rules, and Compose invalidates the readers.
            is Mutation.SetTheme -> theme = mutation.theme
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
        // There is no mutation that attaches a root, so roots are inferred here.
        rootChildren.add(mutation.nodeId)
    }

    private fun setProp(mutation: Mutation.SetProp) {
        val node = nodes[mutation.nodeId] ?: return fail(
            TableError.UNKNOWN_NODE,
            "SetProp for unknown node ${mutation.nodeId}",
        )
        if (!supportsProperty(node.widget, mutation.property)) {
            // An out-of-schema property is reported and skipped, never applied.
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
        // Node id 0 is the "no node" sentinel. The Host uses it for a Dioxus
        // placeholder: an empty `for` body still has a position in the parent, but nothing
        // to draw. It occupies no slot here, and the Host's later Insert for the real
        // children carries the position the placeholder stood at, so indices still line up.
        if (nodeId == ROOT_ID) return
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
        // The placeholder sentinel was never materialised, so removing it is a no-op.
        if (nodeId == ROOT_ID) return
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

    /**
     * Appends to a Text node's content. Streaming sends only the new tail, so the
     * batch does not grow with the text already on screen.
     */
    private fun appendText(mutation: Mutation.AppendText) {
        val node = nodes[mutation.nodeId] ?: return fail(
            TableError.UNKNOWN_NODE,
            "AppendText for unknown node ${mutation.nodeId}",
        )
        if (!supportsProperty(node.widget, PropertyKind.Text)) {
            fail(TableError.UNSUPPORTED_PROPERTY, "AppendText on ${node.widget}")
            return
        }
        revision += 1
        node.props[PropertyKind.Text] = PropertyValue.Text(node.text(PropertyKind.Text) + mutation.text)
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
                PropertyKind.OnKeyDown,
                PropertyKind.OnRangeRequested,
                -> true

                PropertyKind.Text ->
                    widget == WidgetKind.Text ||
                        widget == WidgetKind.Button ||
                        widget == WidgetKind.TextField

                // Note: SpacerProps has width and height in the Rust schema, but there are
                // no matching PropertyKind variants, so a Spacer can only be sized with
                // modifiers.
                PropertyKind.Placeholder -> widget == WidgetKind.TextField
                PropertyKind.Multiline -> widget == WidgetKind.TextField
                PropertyKind.Enabled -> widget != WidgetKind.Spacer

                // Windowing properties belong to the lazy container alone.
                PropertyKind.ItemCount -> widget == WidgetKind.LazyColumn
                PropertyKind.ItemKey -> true

                // Design primitives, resolved against the design system's token table when
                // the node is drawn.
                PropertyKind.TypeRole,
                PropertyKind.FontSize,
                PropertyKind.FontWeight,
                PropertyKind.LineHeight,
                PropertyKind.LetterSpacing,
                PropertyKind.Color,
                PropertyKind.TextAlign,
                PropertyKind.MaxLines,
                PropertyKind.Overflow,
                -> widget == WidgetKind.Text ||
                    widget == WidgetKind.Button ||
                    widget == WidgetKind.TextField

                PropertyKind.Arrangement,
                PropertyKind.Spacing,
                PropertyKind.SpaceRole,
                PropertyKind.Alignment,
                -> widget == WidgetKind.Column ||
                    widget == WidgetKind.Row ||
                    widget == WidgetKind.Box ||
                    widget == WidgetKind.LazyColumn ||
                    widget == WidgetKind.ScrollColumn

                PropertyKind.Variant -> widget == WidgetKind.Button

                // A property declared by an extension package belongs to the widget
                // that package declared it for.
                PropertyKind.Progress -> widget == WidgetKind.LinearProgressIndicator
            }
    }
}
