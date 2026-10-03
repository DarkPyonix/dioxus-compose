//! Form controls the way a browser treats them: what a field holds, which form it belongs
//! to, what clicking it does before and after the click handlers run, what a form submits,
//! and the data an `input`, `change` or `submit` event carries to its handler.
//!
//! The text of a field belongs to the renderer while the user edits it. What the renderer
//! commits (never text still being composed by an input method) is kept per node until the
//! app sets the field's value again, and that is what a field holds here.

use std::any::Any;
use std::collections::HashMap;

use blitz_dom::{BaseDocument, Node, NodeData};
use dioxus_html::{FileData, FormValue, HasFileData, HasFormData};
use style::invalidation::element::restyle_hints::RestyleHint;

use crate::html::NodeId;
use crate::layout::local;

/// The data an `input`, `change` or `submit` event carries.
#[derive(Clone, Debug)]
pub(crate) struct FormEventData {
    /// The field's value: its text, the chosen option's value, or `"true"` and `"false"`
    /// for a checkbox or radio button, so that `FormData::checked` reads it. Empty for a
    /// submit.
    pub value: String,
    /// The named values of the form the field belongs to, in tree order, as the form would
    /// submit them. Empty for a field outside any form.
    pub values: Vec<(String, FormValue)>,
}

impl HasFileData for FormEventData {
    fn files(&self) -> Vec<FileData> {
        // No field here holds files: there is no file input.
        Vec::new()
    }
}

impl HasFormData for FormEventData {
    fn value(&self) -> String {
        self.value.clone()
    }

    fn valid(&self) -> bool {
        true
    }

    fn values(&self) -> Vec<(String, FormValue)> {
        self.values.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// One `<option>` of a `<select>`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectOption {
    /// What the option submits: its `value`, or its text.
    pub value: String,
    /// What the option shows: its `label`, or its text.
    pub label: String,
    /// Whether it carries the `selected` attribute.
    pub marked: bool,
}

/// What clicking an element does once the click handlers have run, as a browser does it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Activation {
    Checkbox(NodeId),
    Radio(NodeId),
    /// A submit button: submits `form`.
    Submit {
        button: NodeId,
        form: NodeId,
    },
    /// A `<label>`: the control it is for is clicked too, after the label.
    Label(NodeId),
}

/// The element's tag, for a DOM element (not an anonymous box).
pub(crate) fn tag(node: &Node) -> Option<&str> {
    match &node.data {
        NodeData::Element(element) => Some(&*element.name.local),
        _ => None,
    }
}

fn is_tag(doc: &BaseDocument, node: NodeId, name: &str) -> bool {
    doc.get_node(node).and_then(tag) == Some(name)
}

/// An `<input>`'s `type`, lower case; `text` when it has none.
pub(crate) fn input_type(node: &Node) -> String {
    node.attr(local("type"))
        .map(str::to_ascii_lowercase)
        .filter(|kind| !kind.is_empty())
        .unwrap_or_else(|| "text".to_string())
}

fn is_disabled(node: &Node) -> bool {
    node.attr(local("disabled"))
        .is_some_and(|value| value != "false")
}

/// Whether `node` is a button that submits its form: `<input type="submit">`, or a
/// `<button>` whose `type` is `submit` or missing or not one it knows.
fn is_submit_button(node: &Node) -> bool {
    match tag(node) {
        Some("input") => matches!(input_type(node).as_str(), "submit" | "image"),
        Some("button") => !matches!(
            node.attr(local("type"))
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("button" | "reset")
        ),
        _ => false,
    }
}

/// The form a control belongs to: the form its `form` attribute names, or else the nearest
/// form around it.
pub(crate) fn form_owner(doc: &BaseDocument, node: NodeId) -> Option<NodeId> {
    let control = doc.get_node(node)?;
    if let Some(id) = control.attr(local("form")) {
        return crate::dom::element_by_id(doc, id).filter(|&form| is_tag(doc, form, "form"));
    }
    let mut current = control.parent;
    while let Some(id) = current {
        if is_tag(doc, id, "form") {
            return Some(id);
        }
        current = doc.get_node(id)?.parent;
    }
    None
}

/// What clicking `hit` activates: the nearest element at or around it that does something
/// when clicked. A disabled control does nothing, and a submit button outside any form has
/// nothing to submit.
pub(crate) fn activation_target(doc: &BaseDocument, hit: NodeId) -> Option<Activation> {
    let mut current = Some(hit);
    while let Some(id) = current {
        let node = doc.get_node(id)?;
        let activation = match tag(node) {
            Some("input") if input_type(node) == "checkbox" => Some(Activation::Checkbox(id)),
            Some("input") if input_type(node) == "radio" => Some(Activation::Radio(id)),
            _ if is_submit_button(node) => {
                form_owner(doc, id).map(|form| Activation::Submit { button: id, form })
            }
            _ => None,
        };
        let interactive = matches!(tag(node), Some("input" | "button" | "select" | "textarea"));
        if interactive {
            return activation.filter(|_| !is_disabled(node));
        }
        if tag(node) == Some("label") {
            return labelled_control(doc, id).map(Activation::Label);
        }
        current = node.parent;
    }
    None
}

/// The control a `<label>` is for: the element its `for` attribute names, or else the
/// first control inside it.
fn labelled_control(doc: &BaseDocument, label: NodeId) -> Option<NodeId> {
    fn labelable(node: &Node) -> bool {
        match tag(node) {
            Some("input") => input_type(node) != "hidden",
            Some("button" | "select" | "textarea") => true,
            _ => false,
        }
    }
    fn first_inside(doc: &BaseDocument, parent: NodeId) -> Option<NodeId> {
        for &child in &doc.get_node(parent)?.children {
            let Some(node) = doc.get_node(child) else {
                continue;
            };
            if labelable(node) {
                return Some(child);
            }
            if let Some(found) = first_inside(doc, child) {
                return Some(found);
            }
        }
        None
    }
    if let Some(id) = doc.get_node(label)?.attr(local("for")) {
        return crate::dom::element_by_id(doc, id)
            .filter(|&control| doc.get_node(control).is_some_and(labelable));
    }
    first_inside(doc, label)
}

/// Whether a checkbox or radio button is checked: the state the document keeps for it once
/// it has been laid out, its `checked` attribute before that.
pub(crate) fn checkedness(doc: &BaseDocument, node: NodeId) -> bool {
    let Some(node) = doc.get_node(node) else {
        return false;
    };
    node.element_data()
        .and_then(|element| element.checkbox_input_checked())
        .unwrap_or_else(|| {
            node.attr(local("checked"))
                .is_some_and(|value| value != "false")
        })
}

/// Sets a laid-out checkbox or radio button's checkedness and has its style worked out
/// again, so `:checked` rules follow. Does nothing to a node that has no such state yet.
pub(crate) fn set_checkedness(doc: &mut BaseDocument, node: NodeId, checked: bool) {
    let Some(state) = doc
        .get_node_mut(node)
        .and_then(|node| node.element_data_mut())
        .and_then(|element| element.checkbox_input_checked_mut())
    else {
        return;
    };
    if *state == checked {
        return;
    }
    *state = checked;
    // As blitz-dom does when an attribute changes: the node and its parent are styled
    // again, so that rules on the control's siblings (`:checked + label`) follow too.
    doc.snapshot_node(node);
    let parent = doc.get_node(node).and_then(|node| node.parent);
    for id in std::iter::once(node).chain(parent) {
        if let Some(target) = doc.get_node(id)
            && let Some(data) = target.stylo_element_data.borrow_mut().as_mut()
        {
            data.hint |= RestyleHint::restyle_subtree();
        }
    }
}

/// The checkedness of every control a click on a checkbox or radio button changes, as it
/// was before, so that a handler that prevents the default can have it put back.
pub(crate) struct Toggled {
    pub target: NodeId,
    pub before: Vec<(NodeId, bool)>,
}

impl Toggled {
    /// Whether the clicked control's own state changed.
    pub fn target_changed(&self, doc: &BaseDocument) -> bool {
        self.before
            .iter()
            .find(|(node, _)| *node == self.target)
            .is_some_and(|&(node, was)| checkedness(doc, node) != was)
    }

    pub fn undo(self, doc: &mut BaseDocument) {
        for (node, was) in self.before {
            set_checkedness(doc, node, was);
        }
    }
}

/// What a browser does to a checkbox or radio button before the click handlers run: a
/// checkbox flips, a radio button is checked and the others of its group are not.
pub(crate) fn toggle(doc: &mut BaseDocument, activation: Activation) -> Option<Toggled> {
    match activation {
        Activation::Checkbox(node) => {
            let was = checkedness(doc, node);
            set_checkedness(doc, node, !was);
            Some(Toggled {
                target: node,
                before: vec![(node, was)],
            })
        }
        Activation::Radio(node) => {
            let mut before = vec![(node, checkedness(doc, node))];
            for other in radio_group(doc, node) {
                if other != node {
                    before.push((other, checkedness(doc, other)));
                }
            }
            for &(id, _) in &before {
                set_checkedness(doc, id, id == node);
            }
            Some(Toggled {
                target: node,
                before,
            })
        }
        Activation::Submit { .. } | Activation::Label(_) => None,
    }
}

/// The radio buttons in the document with the same non-empty `name` and the same form as
/// `node`, `node` included.
fn radio_group(doc: &BaseDocument, node: NodeId) -> Vec<NodeId> {
    let Some(name) = doc
        .get_node(node)
        .and_then(|node| node.attr(local("name")))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
    else {
        return vec![node];
    };
    let owner = form_owner(doc, node);
    connected_elements(doc)
        .into_iter()
        .filter(|&id| {
            doc.get_node(id).is_some_and(|other| {
                tag(other) == Some("input")
                    && input_type(other) == "radio"
                    && other.attr(local("name")) == Some(name.as_str())
            }) && form_owner(doc, id) == owner
        })
        .collect()
}

/// Every element hanging from the document, in tree order.
fn connected_elements(doc: &BaseDocument) -> Vec<NodeId> {
    fn walk(doc: &BaseDocument, node: NodeId, out: &mut Vec<NodeId>) {
        let Some(current) = doc.get_node(node) else {
            return;
        };
        if matches!(current.data, NodeData::Element(_)) {
            out.push(node);
        }
        for &child in &current.children {
            walk(doc, child, out);
        }
    }
    let mut out = Vec::new();
    walk(doc, 0, &mut out);
    out
}

/// White space collapsed and trimmed, as an option's text shows.
fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The options of a `<select>`, in document order, those inside an `<optgroup>` included.
pub(crate) fn select_options(doc: &BaseDocument, select: NodeId) -> Vec<SelectOption> {
    fn collect(doc: &BaseDocument, parent: NodeId, out: &mut Vec<SelectOption>) {
        let Some(parent) = doc.get_node(parent) else {
            return;
        };
        for &child in &parent.children {
            let Some(node) = doc.get_node(child) else {
                continue;
            };
            match tag(node) {
                Some("option") => {
                    let text = collapsed(&node.text_content());
                    out.push(SelectOption {
                        value: node
                            .attr(local("value"))
                            .map(str::to_string)
                            .unwrap_or_else(|| text.clone()),
                        label: node
                            .attr(local("label"))
                            .map(str::to_string)
                            .unwrap_or(text),
                        marked: node
                            .attr(local("selected"))
                            .is_some_and(|value| value != "false"),
                    });
                }
                Some("optgroup") => collect(doc, child, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    collect(doc, select, &mut out);
    out
}

/// The option a `<select>` shows as the app set it: the one whose value is the select's
/// `value` attribute, else the last one marked `selected`, else the first.
pub(crate) fn selected_index(
    doc: &BaseDocument,
    select: NodeId,
    options: &[SelectOption],
) -> Option<usize> {
    if options.is_empty() {
        return None;
    }
    let value = doc
        .get_node(select)
        .and_then(|node| node.attr(local("value")));
    value
        .and_then(|value| options.iter().position(|option| option.value == value))
        .or_else(|| options.iter().rposition(|option| option.marked))
        .or(Some(0))
}

/// What a field holds now: the text the renderer committed since the app last set its
/// value, else the value the app set. For a `<select>`, the chosen option's value.
pub(crate) fn field_value(
    doc: &BaseDocument,
    committed: &HashMap<NodeId, String>,
    node: NodeId,
) -> String {
    if let Some(value) = committed.get(&node) {
        return value.clone();
    }
    let Some(field) = doc.get_node(node) else {
        return String::new();
    };
    match tag(field) {
        Some("select") => {
            let options = select_options(doc, node);
            selected_index(doc, node, &options)
                .map(|index| options[index].value.clone())
                .unwrap_or_default()
        }
        Some("textarea") => field
            .attr(local("value"))
            .map(str::to_string)
            .unwrap_or_else(|| field.text_content()),
        _ => field
            .attr(local("value"))
            .map(str::to_string)
            .unwrap_or_default(),
    }
}

/// The value an `input` or `change` event at `node` carries: `"true"` or `"false"` for a
/// checkbox or radio button, what the field holds otherwise.
pub(crate) fn event_value(
    doc: &BaseDocument,
    committed: &HashMap<NodeId, String>,
    node: NodeId,
) -> String {
    let checkable = doc.get_node(node).is_some_and(|field| {
        tag(field) == Some("input") && matches!(input_type(field).as_str(), "checkbox" | "radio")
    });
    if checkable {
        checkedness(doc, node).to_string()
    } else {
        field_value(doc, committed, node)
    }
}

/// What `form` would submit, in tree order: each named, enabled control's name and value,
/// a checkbox or radio button only when checked, and of the buttons only `submitter`.
pub(crate) fn form_values(
    doc: &BaseDocument,
    committed: &HashMap<NodeId, String>,
    form: NodeId,
    submitter: Option<NodeId>,
) -> Vec<(String, FormValue)> {
    let mut values = Vec::new();
    for id in connected_elements(doc) {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        let Some(name) = tag(node) else {
            continue;
        };
        if !matches!(name, "input" | "textarea" | "select" | "button") {
            continue;
        }
        if form_owner(doc, id) != Some(form) || is_disabled(node) {
            continue;
        }
        let kind = input_type(node);
        let is_button = name == "button"
            || (name == "input"
                && matches!(kind.as_str(), "submit" | "button" | "reset" | "image"));
        if is_button && Some(id) != submitter {
            continue;
        }
        let Some(field_name) = node
            .attr(local("name"))
            .filter(|field_name| !field_name.is_empty())
        else {
            continue;
        };
        let value = if name == "input" && matches!(kind.as_str(), "checkbox" | "radio") {
            if !checkedness(doc, id) {
                continue;
            }
            node.attr(local("value")).unwrap_or("on").to_string()
        } else {
            field_value(doc, committed, id)
        };
        values.push((field_name.to_string(), FormValue::Text(value)));
    }
    values
}

/// The button that submits `form` when Enter is pressed in one of its fields: its first
/// submit button in tree order, if that one is enabled.
pub(crate) fn default_button(doc: &BaseDocument, form: NodeId) -> Option<NodeId> {
    let first = connected_elements(doc).into_iter().find(|&id| {
        doc.get_node(id).is_some_and(is_submit_button) && form_owner(doc, id) == Some(form)
    })?;
    doc.get_node(first)
        .filter(|node| !is_disabled(node))
        .map(|_| first)
}
