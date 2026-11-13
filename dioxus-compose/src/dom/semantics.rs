//! What an element is to assistive technology: its `role` (or the role its tag implies), its
//! name, and its `aria-*` state, as the role and state of the renderer's `Semantics` modifier
//! and the text of its `AccessibleName` property.
//!
//! The name is `aria-label`, then `title`. Without either, an element whose role takes its
//! name from its content (a button, a tab, a tree item, a heading) is named by its text, so
//! a button with words on it reads as those words and only an icon button needs a label.
//!
//! An element that says nothing of the kind is left alone: the renderer then derives what it
//! can from the widget the element becomes (a clickable box is still a button, a run of text
//! is still text).

use blitz_dom::{BaseDocument, NodeData};
use compose_rust::schema::SemanticsRole;
use compose_rust::ui::SemanticsState;

use super::HtmlDom;
use crate::html::NodeId;
use crate::layout::local;
use crate::widgets::Semantics;

/// The semantics of one element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSemantics {
    pub(crate) semantics: Semantics,
    /// The accessible name; `None` when the element has none to give.
    pub(crate) label: Option<String>,
}

fn role_of_word(word: &str) -> Option<SemanticsRole> {
    Some(match word {
        "button" => SemanticsRole::Button,
        "checkbox" => SemanticsRole::Checkbox,
        "switch" => SemanticsRole::Switch,
        "radio" => SemanticsRole::RadioButton,
        "tab" => SemanticsRole::Tab,
        "tablist" => SemanticsRole::TabList,
        "tree" => SemanticsRole::Tree,
        "treeitem" => SemanticsRole::TreeItem,
        "menu" | "menubar" => SemanticsRole::Menu,
        "menuitem" | "menuitemradio" | "menuitemcheckbox" => SemanticsRole::MenuItem,
        "textbox" | "searchbox" => SemanticsRole::TextBox,
        "heading" => SemanticsRole::Heading,
        "navigation" => SemanticsRole::Navigation,
        "list" | "listbox" => SemanticsRole::List,
        "listitem" | "option" => SemanticsRole::ListItem,
        "group" | "dialog" | "alertdialog" | "region" | "toolbar" => SemanticsRole::Group,
        "img" | "image" => SemanticsRole::Image,
        _ => return None,
    })
}

fn role_of_tag(tag: &str, attr: &dyn Fn(&str) -> Option<String>) -> Option<SemanticsRole> {
    Some(match tag {
        "button" => SemanticsRole::Button,
        "nav" => SemanticsRole::Navigation,
        "ul" | "ol" => SemanticsRole::List,
        "li" => SemanticsRole::ListItem,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => SemanticsRole::Heading,
        "textarea" => SemanticsRole::TextBox,
        "img" => SemanticsRole::Image,
        "input" => match attr("type")
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("checkbox") => SemanticsRole::Checkbox,
            Some("radio") => SemanticsRole::RadioButton,
            Some("button" | "submit" | "reset") => SemanticsRole::Button,
            _ => SemanticsRole::TextBox,
        },
        _ => return None,
    })
}

/// Whether an element playing `role` is named by its text when nothing labels it.
fn named_by_content(role: SemanticsRole) -> bool {
    matches!(
        role,
        SemanticsRole::Button
            | SemanticsRole::Checkbox
            | SemanticsRole::Switch
            | SemanticsRole::RadioButton
            | SemanticsRole::Tab
            | SemanticsRole::TreeItem
            | SemanticsRole::MenuItem
            | SemanticsRole::Heading
            | SemanticsRole::ListItem
    )
}

/// `true` / `false` for an ARIA state attribute, `None` when it is absent or not a boolean
/// (`mixed`, `undefined`).
fn aria_bool(value: Option<String>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// The semantics of an element with tag `tag` and attributes read through `attr`.
/// `takes_clicks` is whether a click on it does anything, which exposes the click action, and
/// `content` gives the element's text when its role takes its name from it. `None` when the
/// element names nothing: no role, no name, no state.
pub(crate) fn semantics_for(
    tag: &str,
    attr: &dyn Fn(&str) -> Option<String>,
    takes_clicks: bool,
    content: &dyn Fn() -> String,
) -> Option<NodeSemantics> {
    let explicit = attr("role").and_then(|roles| {
        roles
            .split_ascii_whitespace()
            .find_map(|word| role_of_word(&word.to_ascii_lowercase()))
    });
    let role = explicit.or_else(|| role_of_tag(tag, attr));
    let label = attr("aria-label")
        .or_else(|| attr("title"))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .or_else(|| {
            role.filter(|role| named_by_content(*role))
                .map(|_| content())
                .filter(|text| !text.is_empty())
        });
    let expanded = aria_bool(attr("aria-expanded"));
    let selected = aria_bool(attr("aria-selected"));
    // A native checkbox or radio says it with the `checked` attribute.
    let checked = aria_bool(attr("aria-checked")).or_else(|| attr("checked").map(|_| true));
    let disabled = aria_bool(attr("aria-disabled")).or_else(|| attr("disabled").map(|_| true));
    let hidden = aria_bool(attr("aria-hidden")) == Some(true);
    let stated = expanded.is_some()
        || selected.is_some()
        || checked.is_some()
        || disabled.is_some()
        || hidden;
    if role.is_none() && label.is_none() && !stated {
        return None;
    }
    Some(NodeSemantics {
        semantics: Semantics {
            role,
            state: SemanticsState {
                checked,
                selected,
                expanded,
                disabled,
                focused: None,
                hidden,
                click_action: takes_clicks,
                focus_action: false,
            },
        },
        label,
    })
}

/// The text inside `node`, its descendants' text joined and every run of white space
/// folded to one space. What `aria-hidden="true"` leaves out of the accessibility tree does
/// not name its ancestor either.
fn text_content(doc: &BaseDocument, node: NodeId) -> String {
    fn collect(doc: &BaseDocument, node: NodeId, out: &mut String) {
        let Some(node) = doc.get_node(node) else {
            return;
        };
        match &node.data {
            NodeData::Text(text) => out.push_str(&text.content),
            NodeData::Element(_) => {
                if aria_bool(node.attr(local("aria-hidden")).map(str::to_string)) == Some(true) {
                    return;
                }
                for &child in &node.children {
                    collect(doc, child, out);
                }
            }
            _ => {}
        }
    }
    let mut raw = String::new();
    if let Some(element) = doc.get_node(node) {
        for &child in &element.children {
            collect(doc, child, &mut raw);
        }
    }
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl HtmlDom {
    /// What the element `node` is to assistive technology, when it says anything.
    pub(crate) fn node_semantics(&self, node: NodeId, takes_clicks: bool) -> Option<NodeSemantics> {
        let element = self.doc.get_node(node)?;
        let data = element.data.downcast_element()?;
        let tag = data.name.local.as_ref().to_ascii_lowercase();
        let attr = |name: &str| element.attr(local(name)).map(str::to_string);
        let content = || text_content(&self.doc, node);
        semantics_for(&tag, &attr, takes_clicks, &content)
    }
}
