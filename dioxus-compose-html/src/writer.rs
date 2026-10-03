//! Applies Dioxus mutations to a blitz-dom document.
//!
//! The same stack machine dioxus-native-dom 0.7.10 runs over blitz-dom 0.2.4, with one
//! difference that is the reason this is not a dependency on that crate: listeners are
//! recorded per node and per event name, so that a pointer hit-tested onto a node can be
//! routed to the Dioxus element whose handler should run. dioxus-native-dom only counts
//! listeners per event kind and leaves routing to blitz-dom's own event driver.

use std::collections::HashMap;

use blitz_dom::{BaseDocument, DocumentMutator, LocalName, Namespace, QualName};
use dioxus_core::{
    AttributeValue, ElementId, Template, TemplateAttribute, TemplateNode, WriteMutations,
};

use crate::NodeId;

const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";

/// What the writer keeps between batches of mutations.
#[derive(Default)]
pub(crate) struct WriterState {
    /// Dioxus's stack of nodes being assembled.
    stack: Vec<NodeId>,
    /// Dioxus element id to document node.
    element_to_node: Vec<Option<NodeId>>,
    /// Document node to the Dioxus element id it was last assigned.
    node_to_element: HashMap<NodeId, ElementId>,
    /// Each template is built once, detached, and cloned for every use.
    templates: HashMap<Template, Vec<NodeId>>,
    /// Event names with a listener, per node.
    listeners: HashMap<NodeId, Vec<&'static str>>,
    /// The text the renderer last committed to a field, per node, kept until the app sets
    /// the field's `value` again. The renderer owns a field's text while the user edits it,
    /// so this, not the `value` attribute, is what the field holds now.
    committed: HashMap<NodeId, String>,
}

impl WriterState {
    /// State for a document whose Dioxus root (element 0) is `mount`.
    pub(crate) fn new(mount: NodeId) -> Self {
        let mut state = Self {
            stack: vec![mount],
            element_to_node: vec![Some(mount)],
            ..Self::default()
        };
        state.node_to_element.insert(mount, ElementId(0));
        state
    }

    pub(crate) fn node_of(&self, element: ElementId) -> Option<NodeId> {
        self.element_to_node.get(element.0).copied().flatten()
    }

    /// The Dioxus element of `node`, if Dioxus still maps that element to this node.
    pub(crate) fn element_of(&self, node: NodeId) -> Option<ElementId> {
        let element = *self.node_to_element.get(&node)?;
        (self.node_of(element) == Some(node)).then_some(element)
    }

    /// The event names `node` has a listener for.
    pub(crate) fn listeners(&self, node: NodeId) -> &[&'static str] {
        self.listeners.get(&node).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Records what the renderer committed to the field `node`.
    pub(crate) fn commit(&mut self, node: NodeId, value: &str) {
        self.committed.insert(node, value.to_string());
    }

    pub(crate) fn committed_values(&self) -> &HashMap<NodeId, String> {
        &self.committed
    }

    fn map(&mut self, node: NodeId, element: ElementId) {
        if self.element_to_node.len() <= element.0 {
            self.element_to_node.resize(element.0 + 1, None);
        }
        self.element_to_node[element.0] = Some(node);
        self.node_to_element.insert(node, element);
    }

    fn take_stack(&mut self, m: usize) -> Vec<NodeId> {
        let keep = self.stack.len().saturating_sub(m);
        self.stack.split_off(keep)
    }

    fn forget(&mut self, node: NodeId) {
        self.listeners.remove(&node);
        self.node_to_element.remove(&node);
        self.committed.remove(&node);
    }
}

pub(crate) struct DomWriter<'a> {
    mutator: DocumentMutator<'a>,
    state: &'a mut WriterState,
}

impl<'a> DomWriter<'a> {
    /// Takes nodes that Dioxus is moving out of wherever they sit now.
    ///
    /// blitz-dom 0.2.4 inserts a node at its new place and then removes every occurrence of
    /// it from its old parent's child list. When the old parent is the new parent, which is
    /// what a keyed reorder is, that removes the node it has just inserted and the node
    /// drops out of the tree. Detaching first makes every move an insertion of a node with
    /// no parent.
    fn detach_moved(&mut self, nodes: &[NodeId]) {
        for &node in nodes {
            let attached = self
                .mutator
                .doc
                .get_node(node)
                .is_some_and(|n| n.parent.is_some());
            if attached {
                self.mutator.remove_node(node);
            }
        }
    }

    pub(crate) fn new(doc: &'a mut BaseDocument, state: &'a mut WriterState) -> Self {
        Self {
            mutator: doc.mutate(),
            state,
        }
    }

    fn node(&self, element: ElementId) -> Option<NodeId> {
        self.state.node_of(element)
    }

    fn push_mapped(&mut self, node: NodeId, element: ElementId) {
        self.forget_subtree(node);
        self.state.map(node, element);
        self.state.stack.push(node);
    }

    /// Drops any listener record left on these node ids. blitz-dom reuses the ids of nodes
    /// it has dropped, and a new node must not inherit a dead node's handlers.
    fn forget_subtree(&mut self, node: NodeId) {
        self.state.forget(node);
        for child in self.mutator.child_ids(node) {
            self.forget_subtree(child);
        }
    }

    /// The node at `path` below the top of the stack.
    fn node_at_path(&self, path: &[u8]) -> Option<NodeId> {
        let top = *self.state.stack.last()?;
        Some(self.mutator.node_at_path(top, path))
    }
}

pub(crate) fn element_name(tag: &str, namespace: Option<&str>) -> QualName {
    QualName::new(
        None,
        Namespace::from(namespace.unwrap_or(HTML_NAMESPACE)),
        LocalName::from(tag),
    )
}

fn attribute_name(name: &str, namespace: Option<&str>) -> QualName {
    QualName::new(
        None,
        Namespace::from(namespace.unwrap_or("")),
        LocalName::from(name),
    )
}

fn set_attribute_value(
    mutator: &mut DocumentMutator<'_>,
    node: NodeId,
    name: &str,
    namespace: Option<&str>,
    value: Option<&str>,
) {
    // Dioxus writes single CSS properties (`width: "10px"` in rsx) as attributes in the
    // "style" namespace.
    if namespace == Some("style") {
        match value {
            Some(value) => mutator.set_style_property(node, name, value),
            None => mutator.remove_style_property(node, name),
        }
        return;
    }
    let qual_name = attribute_name(name, namespace);
    match value {
        None => mutator.clear_attribute(node, qual_name),
        Some(value) if name == "dangerous_inner_html" => mutator.set_inner_html(node, value),
        Some(value) => mutator.set_attribute(node, qual_name, value),
    }
}

/// Makes a checkbox or radio button's checkedness follow its `checked` attribute.
///
/// blitz-dom keeps the checkedness of a laid-out checkbox apart from the attribute, seeded
/// from it once when the box is built. Setting the attribute updates it only for a node in
/// the document and only for a value that reads as a boolean, and removing the attribute
/// does not update it at all, so an app that unticks a box would see it stay ticked. The
/// app's `checked` is what the box shows, so the state is written here on every change.
fn set_checkedness(mutator: &mut DocumentMutator<'_>, node: NodeId, checked: bool) {
    if let Some(state) = mutator
        .doc
        .get_node_mut(node)
        .and_then(|node| node.element_data_mut())
        .and_then(|element| element.checkbox_input_checked_mut())
    {
        *state = checked;
    }
}

/// Boolean HTML attributes are present or absent; `false` means absent.
fn is_boolean_off(name: &str, value: &AttributeValue) -> bool {
    matches!(
        name,
        "checked" | "disabled" | "readonly" | "hidden" | "selected" | "multiple"
    ) && match value {
        AttributeValue::Bool(on) => !on,
        AttributeValue::Text(text) => text == "false",
        _ => false,
    }
}

fn build_template_node(mutator: &mut DocumentMutator<'_>, node: &TemplateNode) -> NodeId {
    match node {
        TemplateNode::Element {
            tag,
            namespace,
            attrs,
            children,
        } => {
            let id = mutator.create_element(element_name(tag, *namespace), Vec::new());
            for attr in attrs.iter() {
                if let TemplateAttribute::Static {
                    name,
                    value,
                    namespace,
                } = attr
                {
                    if is_boolean_off(name, &AttributeValue::Text((*value).to_string())) {
                        continue;
                    }
                    set_attribute_value(mutator, id, name, *namespace, Some(*value));
                }
            }
            let child_ids: Vec<NodeId> = children
                .iter()
                .map(|child| build_template_node(mutator, child))
                .collect();
            mutator.append_children(id, &child_ids);
            id
        }
        TemplateNode::Text { text } => mutator.create_text_node(text),
        TemplateNode::Dynamic { .. } => mutator.create_comment_node(),
    }
}

impl WriteMutations for DomWriter<'_> {
    fn append_children(&mut self, id: ElementId, m: usize) {
        let children = self.state.take_stack(m);
        self.detach_moved(&children);
        if let Some(parent) = self.node(id) {
            self.mutator.append_children(parent, &children);
        }
    }

    fn assign_node_id(&mut self, path: &'static [u8], id: ElementId) {
        // A node Dioxus mapped to this id before and that nothing holds any more is gone
        // for good.
        if let Some(previous) = self.node(id) {
            let detached = self
                .mutator
                .doc
                .get_node(previous)
                .is_some_and(|node| node.parent.is_none());
            if detached && !self.state.stack.contains(&previous) {
                self.state.forget(previous);
                self.mutator.remove_node_if_unparented(previous);
            }
        }
        if let Some(node) = self.node_at_path(path) {
            self.state.map(node, id);
        }
    }

    fn create_placeholder(&mut self, id: ElementId) {
        let node = self.mutator.create_comment_node();
        self.push_mapped(node, id);
    }

    fn create_text_node(&mut self, value: &str, id: ElementId) {
        let node = self.mutator.create_text_node(value);
        self.push_mapped(node, id);
    }

    fn load_template(&mut self, template: Template, index: usize, id: ElementId) {
        let mutator = &mut self.mutator;
        let roots = self.state.templates.entry(template).or_insert_with(|| {
            template
                .roots
                .iter()
                .map(|root| build_template_node(mutator, root))
                .collect()
        });
        let Some(&prototype) = roots.get(index) else {
            return;
        };
        let clone = self.mutator.deep_clone_node(prototype);
        self.push_mapped(clone, id);
    }

    fn replace_node_with(&mut self, id: ElementId, m: usize) {
        let replacements = self.state.take_stack(m);
        if let Some(anchor) = self.node(id) {
            self.mutator.replace_node_with(anchor, &replacements);
            self.state.forget(anchor);
        }
    }

    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize) {
        // Take the new nodes first: the path is relative to the node below them.
        let replacements = self.state.take_stack(m);
        if let Some(anchor) = self.node_at_path(path) {
            self.mutator.replace_node_with(anchor, &replacements);
        }
    }

    fn insert_nodes_after(&mut self, id: ElementId, m: usize) {
        let nodes = self.state.take_stack(m);
        self.detach_moved(&nodes);
        if let Some(anchor) = self.node(id) {
            self.mutator.insert_nodes_after(anchor, &nodes);
        }
    }

    fn insert_nodes_before(&mut self, id: ElementId, m: usize) {
        let nodes = self.state.take_stack(m);
        self.detach_moved(&nodes);
        if let Some(anchor) = self.node(id) {
            self.mutator.insert_nodes_before(anchor, &nodes);
        }
    }

    fn set_attribute(
        &mut self,
        name: &'static str,
        namespace: Option<&'static str>,
        value: &AttributeValue,
        id: ElementId,
    ) {
        let Some(node) = self.node(id) else {
            return;
        };
        if namespace.is_none() && name == "value" {
            // The app set the field's value: that is what it holds now, whatever was
            // typed before.
            self.state.committed.remove(&node);
        }
        if is_boolean_off(name, value) {
            set_attribute_value(&mut self.mutator, node, name, namespace, None);
            if namespace.is_none() && name == "checked" {
                set_checkedness(&mut self.mutator, node, false);
            }
            return;
        }
        let text = match value {
            AttributeValue::None => None,
            AttributeValue::Text(text) => Some(text.clone()),
            AttributeValue::Float(number) => Some(number.to_string()),
            AttributeValue::Int(number) => Some(number.to_string()),
            AttributeValue::Bool(flag) => Some(flag.to_string()),
            // Listeners arrive through create_event_listener; an opaque Rust value has no
            // text form for the document.
            AttributeValue::Listener(_) | AttributeValue::Any(_) => return,
        };
        set_attribute_value(&mut self.mutator, node, name, namespace, text.as_deref());
        if namespace.is_none() && name == "checked" {
            // Present means checked, whatever the value; `None` removed it.
            set_checkedness(&mut self.mutator, node, text.is_some());
        }
    }

    fn set_node_text(&mut self, value: &str, id: ElementId) {
        if let Some(node) = self.node(id) {
            self.mutator.set_node_text(node, value);
        }
    }

    fn create_event_listener(&mut self, name: &'static str, id: ElementId) {
        let Some(node) = self.node(id) else {
            return;
        };
        self.state.node_to_element.insert(node, id);
        let names = self.state.listeners.entry(node).or_default();
        if !names.contains(&name) {
            names.push(name);
        }
    }

    fn remove_event_listener(&mut self, name: &'static str, id: ElementId) {
        let Some(node) = self.node(id) else {
            return;
        };
        if let Some(names) = self.state.listeners.get_mut(&node) {
            names.retain(|existing| *existing != name);
        }
    }

    fn remove_node(&mut self, id: ElementId) {
        if let Some(node) = self.node(id) {
            self.mutator.remove_node(node);
            self.state.forget(node);
        }
    }

    fn push_root(&mut self, id: ElementId) {
        if let Some(node) = self.node(id) {
            self.state.stack.push(node);
        }
    }
}
