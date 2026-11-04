//! Applies Dioxus mutations to a blitz-dom document.
//!
//! The same stack machine dioxus-native-dom 0.7.10 runs over blitz-dom 0.2.4, with one
//! difference that is the reason this is not a dependency on that crate: listeners are
//! recorded per node and per event name, so that a pointer hit-tested onto a node can be
//! routed to the Dioxus element whose handler should run. dioxus-native-dom only counts
//! listeners per event kind and leaves routing to blitz-dom's own event driver.

use std::collections::{HashMap, HashSet};

use blitz_dom::{BaseDocument, DocumentMutator, LocalName, Namespace, QualName};
use dioxus_core::{
    AttributeValue, ElementId, Template, TemplateAttribute, TemplateNode, WriteMutations,
};
use style::invalidation::element::restyle_hints::RestyleHint;
use style::selector_parser::RestyleDamage;

use crate::html::NodeId;
use crate::layout::{LayoutState, local};

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
    /// Whether event listeners changed since the last pointer subscription check.
    pub(crate) listeners_dirty: bool,
    /// The text the renderer last committed to a field, per node, kept until the app sets
    /// the field's `value` again. The renderer owns a field's text while the user edits it,
    /// so this, not the `value` attribute, is what the field holds now.
    committed: HashMap<NodeId, String>,
    /// Whether any DOM mutation happened since the last layout pass.
    pub(crate) dirty: bool,
    /// Whether the DOM tree structure, text content, or non-style attributes changed since
    /// the last layout pass.
    pub(crate) tree_dirty: bool,
    /// Whether any `<style>` element or its text changed since the last layout pass.
    pub(crate) stylesheet_dirty: bool,
    /// Nodes whose subtree structure, text content, or non-style attributes changed since
    /// the last layout pass.
    pub(crate) dirty_nodes: HashSet<NodeId>,
    /// Container nodes whose direct child list changed (children inserted, removed, or
    /// replaced) without mutating their surviving children's subtrees.
    pub(crate) dirty_children_nodes: HashSet<NodeId>,
    /// Nodes whose `style` or `class` attributes changed since the last layout pass.
    pub(crate) dirty_style_nodes: HashSet<NodeId>,
    /// Nodes whose inline `style` changed only non-inherited properties, so their already-styled
    /// children do not need to be recascaded.
    pub(crate) non_inherited_style_nodes: HashSet<NodeId>,
    /// Nodes that registered a `mounted` listener and have not yet received their mount event.
    pub(crate) pending_mounts: Vec<NodeId>,
    /// The most recently added or updated element carrying an active `autofocus` attribute.
    pub(crate) pending_autofocus: Option<NodeId>,
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

    /// Every node with a listener, with the event names it listens for.
    pub(crate) fn listening(&self) -> impl Iterator<Item = (NodeId, &[&'static str])> {
        self.listeners
            .iter()
            .map(|(node, names)| (*node, names.as_slice()))
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
        if self.listeners.remove(&node).is_some() {
            self.listeners_dirty = true;
        }
        if let Some(element) = self.node_to_element.remove(&node)
            && self.element_to_node.get(element.0) == Some(&Some(node))
        {
            self.element_to_node[element.0] = None;
        }
        self.committed.remove(&node);
        self.pending_mounts.retain(|&n| n != node);
        if self.pending_autofocus == Some(node) {
            self.pending_autofocus = None;
        }
    }

    pub(crate) fn mark_tree_dirty(&mut self, node: NodeId) {
        self.dirty = true;
        self.tree_dirty = true;
        self.dirty_nodes.insert(node);
        self.non_inherited_style_nodes.remove(&node);
    }

    pub(crate) fn mark_children_dirty(&mut self, node: NodeId) {
        self.dirty = true;
        self.tree_dirty = true;
        self.dirty_children_nodes.insert(node);
    }

    pub(crate) fn mark_style_dirty(&mut self, node: NodeId) {
        self.dirty = true;
        self.dirty_style_nodes.insert(node);
        self.non_inherited_style_nodes.remove(&node);
    }

    pub(crate) fn mark_non_inherited_style_dirty(&mut self, node: NodeId) {
        self.dirty = true;
        if self.dirty_style_nodes.insert(node) && !self.dirty_nodes.contains(&node) {
            self.non_inherited_style_nodes.insert(node);
        }
    }
}

pub(crate) struct DomWriter<'a> {
    mutator: DocumentMutator<'a>,
    state: &'a mut WriterState,
    layout_state: &'a mut LayoutState,
}

impl<'a> DomWriter<'a> {
    /// Takes nodes that Dioxus is moving out of wherever they sit now.
    ///
    /// blitz-dom 0.2.4 inserts a node at its new place and then removes every occurrence of
    /// it from its old parent's child list. When the old parent is the new parent, which is
    /// what a keyed reorder is, that removes the node it has just inserted and the node
    /// drops out of the tree. Detaching first makes every move an insertion of a node with
    /// no parent. Returns `true` if any of the nodes were already attached in the document.
    fn detach_moved(&mut self, nodes: &[NodeId]) -> bool {
        let mut any_attached = false;
        for &node in nodes {
            let attached = self
                .mutator
                .doc
                .get_node(node)
                .is_some_and(|n| n.parent.is_some());
            if attached {
                any_attached = true;
                self.mutator.remove_node(node);
            }
        }
        any_attached
    }

    pub(crate) fn new(
        doc: &'a mut BaseDocument,
        state: &'a mut WriterState,
        layout_state: &'a mut LayoutState,
    ) -> Self {
        Self {
            mutator: doc.mutate(),
            state,
            layout_state,
        }
    }

    fn node(&self, element: ElementId) -> Option<NodeId> {
        self.state.node_of(element)
    }

    fn push_mapped(&mut self, node: NodeId, element: ElementId) {
        if let Some(previous) = self.node(element) {
            let detached = self
                .mutator
                .doc
                .get_node(previous)
                .is_some_and(|n| n.parent.is_none());
            if detached && !self.state.stack.contains(&previous) {
                self.drop_detached_subtree(previous);
            }
        }
        self.forget_subtree(node);
        self.state.map(node, element);
        self.state.stack.push(node);
    }

    /// Drops any listener record left on these node ids. blitz-dom reuses the ids of nodes
    /// it has dropped, and a new node must not inherit a dead node's handlers.
    fn forget_subtree(&mut self, node: NodeId) {
        self.state.forget(node);
        self.layout_state.forget(node);
        for child in self.mutator.child_ids(node) {
            self.forget_subtree(child);
        }
    }

    /// Recursively drops a detached node and its descendants from both `WriterState` and
    /// `blitz-dom`'s slab when the node is not waiting on the writer's stack.
    fn drop_detached_subtree(&mut self, node: NodeId) {
        if self.state.stack.contains(&node) {
            return;
        }
        let children = self.mutator.child_ids(node);
        for child in children {
            self.mutator.remove_node(child);
            self.drop_detached_subtree(child);
        }
        self.state.forget(node);
        self.layout_state.forget(node);
        self.mutator.remove_node_if_unparented(node);
    }

    /// Scans a cloned template subtree for the first element with an `autofocus` attribute
    /// and checks whether it introduces a `<style>` element.
    fn scan_autofocus(&mut self, node: NodeId) {
        if let Some(n) = self.mutator.doc.get_node(node) {
            if n.attr(local("autofocus")).is_some() {
                self.state.pending_autofocus = Some(node);
            }
            if n.data.is_element_with_tag_name(&local("style")) {
                self.state.stylesheet_dirty = true;
            }
        }
        for child in self.mutator.child_ids(node) {
            self.scan_autofocus(child);
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

pub(crate) fn attribute_name(name: &str, namespace: Option<&str>) -> QualName {
    QualName::new(
        None,
        Namespace::from(namespace.unwrap_or("")),
        LocalName::from(name),
    )
}

fn is_definitely_non_inherited_property(prop: &str) -> bool {
    let prop = prop.trim();
    if prop.starts_with("--") {
        return false;
    }
    matches!(
        prop.to_ascii_lowercase().as_str(),
        "top"
            | "left"
            | "right"
            | "bottom"
            | "inset"
            | "width"
            | "height"
            | "min-width"
            | "min-height"
            | "max-width"
            | "max-height"
            | "transform"
            | "transform-origin"
            | "opacity"
            | "background"
            | "background-color"
            | "background-image"
            | "background-position"
            | "background-size"
            | "background-repeat"
            | "border"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "border-width"
            | "border-style"
            | "border-color"
            | "border-radius"
            | "margin"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "padding"
            | "padding-top"
            | "padding-right"
            | "padding-bottom"
            | "padding-left"
            | "z-index"
            | "display"
            | "position"
            | "overflow"
            | "overflow-x"
            | "overflow-y"
            | "flex"
            | "flex-grow"
            | "flex-shrink"
            | "flex-basis"
            | "flex-direction"
            | "flex-wrap"
            | "align-items"
            | "align-self"
            | "align-content"
            | "justify-content"
            | "gap"
            | "row-gap"
            | "column-gap"
            | "outline"
            | "outline-color"
            | "outline-style"
            | "outline-width"
            | " outline-offset"
            | "box-shadow"
            | "box-sizing"
            | "pointer-events"
    )
}

fn style_str_only_non_inherited(css: &str) -> bool {
    for decl in css.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let Some((name, val)) = decl.split_once(':') else {
            return false;
        };
        if val.contains("var(") || !is_definitely_non_inherited_property(name) {
            return false;
        }
    }
    true
}

fn has_slow_selector_flags(mutator: &DocumentMutator<'_>, node: NodeId) -> bool {
    let Some(n) = mutator.doc.get_node(node) else {
        return false;
    };
    let flags = *n.selector_flags.borrow();
    if !(flags - flags.for_self()).is_empty() || !flags.is_empty() {
        return true;
    }
    if let Some(parent_id) = n.parent
        && let Some(parent) = mutator.doc.get_node(parent_id)
        && !parent.selector_flags.borrow().is_empty()
    {
        return true;
    }
    false
}

fn update_inline_style_declarations(
    mutator: &mut DocumentMutator<'_>,
    node: NodeId,
    old_css: Option<&str>,
    new_css: Option<&str>,
) {
    let mut old_props = Vec::new();
    if let Some(css) = old_css {
        for decl in css.split(';') {
            if let Some((name, _)) = decl.trim().split_once(':') {
                let name = name.trim();
                if !name.is_empty() {
                    old_props.push(name.to_ascii_lowercase());
                }
            }
        }
    }
    let mut new_props = Vec::new();
    if let Some(css) = new_css {
        for decl in css.split(';') {
            if let Some((name, val)) = decl.trim().split_once(':') {
                let name = name.trim();
                let val = val.trim();
                if !name.is_empty() {
                    new_props.push((name.to_ascii_lowercase(), val));
                }
            }
        }
    }
    for old_name in &old_props {
        if !new_props.iter().any(|(n, _)| n == old_name) {
            mutator.remove_style_property(node, old_name);
        }
    }
    for (name, val) in &new_props {
        mutator.set_style_property(node, name, val);
    }
    if let Some(el) = mutator
        .doc
        .get_node_mut(node)
        .and_then(|n| n.element_data_mut())
    {
        let qname = attribute_name("style", None);
        match new_css {
            Some(v) => {
                if let Some(existing) = el.attrs.iter_mut().find(|a| a.name.local == local("style"))
                {
                    existing.value.clear();
                    existing.value.push_str(v);
                } else {
                    el.attrs.push(blitz_dom::node::Attribute {
                        name: qname,
                        value: v.to_string(),
                    });
                }
            }
            None => {
                el.attrs.retain(|a| a.name.local != local("style"));
            }
        }
    }
}

fn mark_inline_style_restyle(
    mutator: &mut DocumentMutator<'_>,
    node: NodeId,
    non_inherited_only: bool,
) {
    let styled = mutator
        .doc
        .get_node(node)
        .is_some_and(|n| n.primary_styles().is_some());
    if !styled {
        return;
    }
    if has_slow_selector_flags(mutator, node) {
        mutator.doc.snapshot_node(node);
        if let Some(target) = mutator.doc.get_node_mut(node) {
            target.set_restyle_hint(RestyleHint::restyle_subtree());
            target.insert_damage(RestyleDamage::all());
        }
        if let Some(parent) = mutator.doc.get_node(node).and_then(|n| n.parent)
            && let Some(parent_node) = mutator.doc.get_node_mut(parent)
        {
            parent_node.set_restyle_hint(RestyleHint::restyle_subtree());
        }
    } else if let Some(target) = mutator.doc.get_node_mut(node) {
        let hint = if non_inherited_only {
            RestyleHint::RESTYLE_SELF
        } else {
            RestyleHint::restyle_subtree()
        };
        target.set_restyle_hint(hint);
        target.insert_damage(RestyleDamage::all());
    }
}

fn preserve_parent_hint_across_child_list_change(
    mutator: &mut DocumentMutator<'_>,
    parent: Option<NodeId>,
    op: impl FnOnce(&mut DocumentMutator<'_>),
) {
    let saved = parent.and_then(|pid| {
        let p = mutator.doc.get_node(pid)?;
        if !p.selector_flags.borrow().is_empty() {
            return None;
        }
        let hint = p
            .stylo_element_data
            .borrow()
            .as_ref()
            .map(|d| d.hint)
            .unwrap_or_else(RestyleHint::empty);
        Some((pid, hint))
    });
    op(mutator);
    if let Some((pid, prior_hint)) = saved
        && let Some(p) = mutator.doc.get_node_mut(pid)
        && let Some(data) = p.stylo_element_data.borrow_mut().as_mut()
    {
        data.hint = prior_hint | RestyleHint::RESTYLE_SELF;
    }
}

fn set_attribute_value(
    mutator: &mut DocumentMutator<'_>,
    node: NodeId,
    name: &str,
    namespace: Option<&str>,
    value: Option<&str>,
) -> bool {
    // Dioxus writes single CSS properties (`width: "10px"` in rsx) as attributes in the
    // "style" namespace.
    if namespace == Some("style") {
        let non_inherited =
            is_definitely_non_inherited_property(name) && value.is_none_or(|v| !v.contains("var("));
        let fast_non_inherited = non_inherited
            && mutator
                .doc
                .get_node(node)
                .is_some_and(|n| n.primary_styles().is_some())
            && !has_slow_selector_flags(mutator, node);
        mark_inline_style_restyle(mutator, node, non_inherited);
        match value {
            Some(value) => mutator.set_style_property(node, name, value),
            None => mutator.remove_style_property(node, name),
        }
        return fast_non_inherited;
    }
    let qual_name = attribute_name(name, namespace);
    if namespace.is_none() && name == "style" {
        let fast = mutator
            .doc
            .get_node(node)
            .is_some_and(|n| n.primary_styles().is_some())
            && !has_slow_selector_flags(mutator, node);
        if fast {
            let old_css = mutator
                .doc
                .get_node(node)
                .and_then(|n| n.attr(local("style")))
                .map(str::to_string);
            let old_non_inherited = old_css.as_deref().is_none_or(style_str_only_non_inherited);
            let new_non_inherited = value.is_none_or(style_str_only_non_inherited);
            update_inline_style_declarations(mutator, node, old_css.as_deref(), value);
            let non_inherited_only = old_non_inherited && new_non_inherited;
            if let Some(target) = mutator.doc.get_node_mut(node) {
                let hint = if non_inherited_only {
                    RestyleHint::RESTYLE_SELF
                } else {
                    RestyleHint::restyle_subtree()
                };
                target.set_restyle_hint(hint);
                target.insert_damage(RestyleDamage::all());
            }
            return non_inherited_only;
        }
    }
    match value {
        None => mutator.clear_attribute(node, qual_name),
        Some(value) if name == "dangerous_inner_html" => mutator.set_inner_html(node, value),
        Some(value) => mutator.set_attribute(node, qual_name, value),
    }
    false
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
        "checked" | "disabled" | "readonly" | "hidden" | "selected" | "multiple" | "autofocus"
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
        let moved = self.detach_moved(&children);
        if let Some(parent) = self.node(id) {
            preserve_parent_hint_across_child_list_change(&mut self.mutator, Some(parent), |m| {
                m.append_children(parent, &children);
            });
            self.state.mark_children_dirty(parent);
            if moved {
                for &child in &children {
                    self.state.mark_tree_dirty(child);
                }
            }
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
                self.drop_detached_subtree(previous);
            }
        }
        if let Some(node) = self.node_at_path(path) {
            self.state.map(node, id);
        }
    }

    fn create_placeholder(&mut self, id: ElementId) {
        let node = self.mutator.create_comment_node();
        self.push_mapped(node, id);
        self.state.mark_tree_dirty(node);
    }

    fn create_text_node(&mut self, value: &str, id: ElementId) {
        let node = self.mutator.create_text_node(value);
        self.push_mapped(node, id);
        self.state.mark_tree_dirty(node);
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
        self.scan_autofocus(clone);
        self.state.mark_tree_dirty(clone);
    }

    fn replace_node_with(&mut self, id: ElementId, m: usize) {
        let replacements = self.state.take_stack(m);
        if let Some(anchor) = self.node(id) {
            let parent = self.mutator.doc.get_node(anchor).and_then(|n| n.parent);
            if let Some(parent) = parent {
                self.state.mark_children_dirty(parent);
            }
            for &node in &replacements {
                self.state.mark_tree_dirty(node);
            }
            preserve_parent_hint_across_child_list_change(&mut self.mutator, parent, |m| {
                m.replace_node_with(anchor, &replacements);
            });
            self.drop_detached_subtree(anchor);
        }
    }

    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize) {
        // Take the new nodes first: the path is relative to the node below them.
        let replacements = self.state.take_stack(m);
        if let Some(anchor) = self.node_at_path(path) {
            let parent = self.mutator.doc.get_node(anchor).and_then(|n| n.parent);
            if let Some(parent) = parent {
                self.state.mark_children_dirty(parent);
            }
            for &node in &replacements {
                self.state.mark_tree_dirty(node);
            }
            preserve_parent_hint_across_child_list_change(&mut self.mutator, parent, |m| {
                m.replace_node_with(anchor, &replacements);
            });
            self.drop_detached_subtree(anchor);
        }
    }

    fn insert_nodes_after(&mut self, id: ElementId, m: usize) {
        let nodes = self.state.take_stack(m);
        let moved = self.detach_moved(&nodes);
        if let Some(anchor) = self.node(id) {
            let parent = self.mutator.doc.get_node(anchor).and_then(|n| n.parent);
            if let Some(parent) = parent {
                self.state.mark_children_dirty(parent);
            }
            if moved {
                for &node in &nodes {
                    self.state.mark_tree_dirty(node);
                }
            }
            preserve_parent_hint_across_child_list_change(&mut self.mutator, parent, |m| {
                m.insert_nodes_after(anchor, &nodes);
            });
        }
    }

    fn insert_nodes_before(&mut self, id: ElementId, m: usize) {
        let nodes = self.state.take_stack(m);
        let moved = self.detach_moved(&nodes);
        if let Some(anchor) = self.node(id) {
            let parent = self.mutator.doc.get_node(anchor).and_then(|n| n.parent);
            if let Some(parent) = parent {
                self.state.mark_children_dirty(parent);
            }
            if moved {
                for &node in &nodes {
                    self.state.mark_tree_dirty(node);
                }
            }
            preserve_parent_hint_across_child_list_change(&mut self.mutator, parent, |m| {
                m.insert_nodes_before(anchor, &nodes);
            });
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
        let style_only = namespace == Some("style")
            || (namespace.is_none() && matches!(name, "style" | "class"));
        if is_boolean_off(name, value) {
            let non_inherited = set_attribute_value(&mut self.mutator, node, name, namespace, None);
            if namespace.is_none() && name == "checked" {
                set_checkedness(&mut self.mutator, node, false);
            }
            if non_inherited {
                self.state.mark_non_inherited_style_dirty(node);
            } else if style_only {
                self.state.mark_style_dirty(node);
            } else {
                self.state.mark_tree_dirty(node);
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
        let non_inherited =
            set_attribute_value(&mut self.mutator, node, name, namespace, text.as_deref());
        if namespace.is_none() && name == "checked" {
            // Present means checked, whatever the value; `None` removed it.
            set_checkedness(&mut self.mutator, node, text.is_some());
        }
        if namespace.is_none() && name == "autofocus" && text.is_some() {
            self.state.pending_autofocus = Some(node);
        }
        if non_inherited {
            self.state.mark_non_inherited_style_dirty(node);
        } else if style_only {
            self.state.mark_style_dirty(node);
        } else {
            self.state.mark_tree_dirty(node);
        }
    }

    fn set_node_text(&mut self, value: &str, id: ElementId) {
        if let Some(node) = self.node(id) {
            self.mutator.set_node_text(node, value);
            self.state.mark_tree_dirty(node);
            if let Some(parent) = self.mutator.doc.get_node(node).and_then(|n| n.parent) {
                self.state.mark_tree_dirty(parent);
                if self
                    .mutator
                    .doc
                    .get_node(parent)
                    .is_some_and(|p| p.data.is_element_with_tag_name(&local("style")))
                {
                    self.state.stylesheet_dirty = true;
                }
            }
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
            self.state.listeners_dirty = true;
        }
        if name == "mounted" && !self.state.pending_mounts.contains(&node) {
            self.state.pending_mounts.push(node);
        }
        self.state.dirty = true;
    }

    fn remove_event_listener(&mut self, name: &'static str, id: ElementId) {
        let Some(node) = self.node(id) else {
            return;
        };
        if let Some(names) = self.state.listeners.get_mut(&node) {
            let before = names.len();
            names.retain(|existing| *existing != name);
            if names.len() != before {
                self.state.listeners_dirty = true;
            }
        }
        if name == "mounted" {
            self.state.pending_mounts.retain(|&n| n != node);
        }
        self.state.dirty = true;
    }

    fn remove_node(&mut self, id: ElementId) {
        if let Some(node) = self.node(id) {
            if self
                .mutator
                .doc
                .get_node(node)
                .is_some_and(|n| n.data.is_element_with_tag_name(&local("style")))
            {
                self.state.stylesheet_dirty = true;
            }
            if let Some(parent) = self.mutator.doc.get_node(node).and_then(|n| n.parent) {
                self.state.mark_children_dirty(parent);
            }
            self.mutator.remove_node(node);
            self.drop_detached_subtree(node);
            self.state.dirty = true;
            self.state.tree_dirty = true;
        }
    }

    fn push_root(&mut self, id: ElementId) {
        if let Some(node) = self.node(id) {
            self.state.stack.push(node);
        }
    }
}
