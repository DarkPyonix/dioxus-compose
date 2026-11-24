//! Modals and comboboxes on an HTML page reach the renderer's accessibility tree.
//!
//! `role="combobox"`, `"listbox"` and `"option"` become the renderer's `ComboBox`, `ListBox`
//! and `Option` roles. `aria-modal="true"`, and a `<dialog>` opened with `showModal()`,
//! carry the modal bit of the semantics state, so nothing outside them is in the tree while
//! they are open. `aria-activedescendant` is sent as the renderer node of the element whose
//! `id` it names, and only a change of it is sent again.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint};
use dioxus_compose::prelude::WritableExt;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    EventPayload, Modifier, PropertyKind, SEMANTICS_MODAL_BIT, SemanticsRole,
};
use dioxus_elements::MountedData;
use dioxus_hooks::use_signal;

struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let width = match request.width {
            WidthConstraint::AtMost(width) => {
                (request.text.chars().count() as f32 * 10.0).min(width.max(0.0))
            }
            _ => request.text.chars().count() as f32 * 10.0,
        };
        TextMetrics {
            width,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        // A dialog is drawn as a block of its own whether or not it is open, so its box is
        // in every frame and only its semantics change.
        stylesheets: vec![
            "html, body { margin: 0; padding: 0; } dialog { display: block; }".to_string(),
        ],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

/// One record of a batch, owned: only what these tests read.
#[derive(Clone, Debug, PartialEq)]
enum Record {
    Prop(u32, PropertyKind, Option<i64>, Option<String>),
    Modifier(u32, Modifier),
    Other,
}

fn records(bytes: &[u8]) -> Vec<Record> {
    decode_batch(bytes)
        .expect("the batch decodes")
        .into_iter()
        .map(|record| match record {
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => match value {
                PropertyValue::Integer(number) => {
                    Record::Prop(node_id, property, Some(number), None)
                }
                PropertyValue::String(text) => {
                    Record::Prop(node_id, property, None, Some(text.to_string()))
                }
                _ => Record::Prop(node_id, property, None, None),
            },
            Mutation::SetModifier {
                node_id, modifier, ..
            } => Record::Modifier(node_id, modifier),
            _ => Record::Other,
        })
        .filter(|record| *record != Record::Other)
        .collect()
}

/// The nodes given a `Semantics` modifier with `role`, and the state each was given.
fn semantics_of(records: &[Record], role: SemanticsRole) -> Vec<(u32, u32)> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Modifier(node, Modifier::Semantics { role: r, state }) if *r == role => {
                Some((*node, *state))
            }
            _ => None,
        })
        .collect()
}

fn accessible_name(records: &[Record], node: u32) -> Option<String> {
    records.iter().rev().find_map(|record| match record {
        Record::Prop(id, PropertyKind::AccessibleName, _, Some(text)) if *id == node => {
            Some(text.clone())
        }
        _ => None,
    })
}

/// The one node with `role` named `name`.
#[track_caller]
fn named(records: &[Record], role: SemanticsRole, name: &str) -> u32 {
    let found: Vec<u32> = semantics_of(records, role)
        .into_iter()
        .map(|(node, _)| node)
        .filter(|node| accessible_name(records, *node).as_deref() == Some(name))
        .collect();
    assert_eq!(found.len(), 1, "one {role:?} named {name}: {records:#?}");
    found[0]
}

/// Every `ActiveDescendant` record: the node it is on, and the node it names (`None` when it
/// clears the property).
fn active_descendants(records: &[Record]) -> Vec<(u32, Option<i64>)> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Prop(node, PropertyKind::ActiveDescendant, value, _) => Some((*node, *value)),
            _ => None,
        })
        .collect()
}

/// The handler of the node's `Clickable` modifier.
#[track_caller]
fn click_handler(records: &[Record], node: u32) -> u64 {
    records
        .iter()
        .find_map(|record| match record {
            Record::Modifier(id, Modifier::Clickable { handler_id }) if *id == node => {
                Some(*handler_id)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("node {node} takes clicks: {records:#?}"))
}

fn click(host: &mut Host, node: u32, handler: u64) -> Vec<Record> {
    let (batch, _) = host
        .dispatch(HostEvent::new(node, handler, EventPayload::Clicked))
        .expect("the click is dispatched");
    records(batch)
}

fn combobox_page() -> Element {
    rsx! {
        input { role: "combobox", "aria-label": "Go to file", "aria-expanded": "true" }
        div { role: "listbox", "aria-label": "Files",
            div { role: "option", "aria-selected": "true", "a.rs" }
            div { role: "option", "b.rs" }
        }
        ul { li { "a list is still a list" } }
    }
}

/// `role="combobox"`, `"listbox"` and `"option"` are the renderer's `ComboBox`, `ListBox` and
/// `Option`, no longer a text box, a list and a list item. An option is named by its text, and
/// a `<ul>` and its `<li>` stay a list and a list item.
#[test]
fn fr34_aria_combobox_listbox_option_roles_reach_the_renderer() {
    let mut host = Host::html(combobox_page, config);
    let first = records(host.rebuild().unwrap());

    named(&first, SemanticsRole::ComboBox, "Go to file");
    named(&first, SemanticsRole::ListBox, "Files");
    named(&first, SemanticsRole::Option, "a.rs");
    named(&first, SemanticsRole::Option, "b.rs");
    assert_eq!(
        semantics_of(&first, SemanticsRole::List).len(),
        1,
        "{first:#?}"
    );
    assert_eq!(
        semantics_of(&first, SemanticsRole::ListItem).len(),
        1,
        "{first:#?}"
    );
    assert!(
        semantics_of(&first, SemanticsRole::TextBox).is_empty(),
        "the combobox role wins over the input's own: {first:#?}"
    );
}

thread_local! {
    static DIALOG: RefCell<Option<Rc<MountedData>>> = const { RefCell::new(None) };
}

fn settings_dialog() -> Rc<MountedData> {
    DIALOG.with(|dialog| dialog.borrow().clone().expect("the dialog was mounted"))
}

fn modal_page() -> Element {
    let mut shown = use_signal(|| false);
    rsx! {
        div { role: "dialog", "aria-modal": "true", "aria-label": "Trust", "Trust this folder?" }
        div { role: "dialog", "aria-modal": "false", "aria-label": "Find", "Find" }
        dialog { open: true, "aria-label": "Open", "Not modal" }
        dialog {
            "aria-label": "Settings",
            onmounted: move |evt| DIALOG.with(|dialog| *dialog.borrow_mut() = Some(evt.data())),
            "Settings"
        }
        button {
            "aria-label": "Toggle",
            onclick: move |_| {
                if shown() {
                    settings_dialog().close();
                } else {
                    settings_dialog().show_modal();
                }
                shown.set(!shown());
            },
            "Toggle"
        }
    }
}

/// The modal bit of the node a dialog draws, by its name.
#[track_caller]
fn modal_bit(records: &[Record], name: &str) -> u32 {
    let node = records
        .iter()
        .find_map(|record| match record {
            Record::Prop(node, PropertyKind::AccessibleName, _, Some(text)) if text == name => {
                Some(*node)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("a node named {name}: {records:#?}"));
    records
        .iter()
        .rev()
        .find_map(|record| match record {
            Record::Modifier(id, Modifier::Semantics { state, .. }) if *id == node => {
                Some(state & SEMANTICS_MODAL_BIT)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name} carries semantics: {records:#?}"))
}

/// `aria-modal="true"` sets the modal bit and `"false"` does not. A `<dialog open>` is not
/// modal; the same element opened with `showModal()` is, until `close()`.
#[test]
fn fr34_aria_modal_sets_the_modal_bit() {
    let mut host = Host::html(modal_page, config);
    let first = records(host.rebuild().unwrap());
    assert_eq!(modal_bit(&first, "Trust"), SEMANTICS_MODAL_BIT);
    assert_eq!(modal_bit(&first, "Find"), 0);
    assert_eq!(modal_bit(&first, "Open"), 0, "open without showModal()");

    let toggle = named(&first, SemanticsRole::Button, "Toggle");
    let handler = click_handler(&first, toggle);
    let settings = first
        .iter()
        .find_map(|record| match record {
            Record::Prop(node, PropertyKind::AccessibleName, _, Some(text))
                if text == "Settings" =>
            {
                Some(*node)
            }
            _ => None,
        })
        .expect("the closed dialog still has its box and its name");

    let shown = records(host.render_frame(0).unwrap());
    assert!(shown.is_empty(), "nothing changed yet: {shown:#?}");

    let opened = click(&mut host, toggle, handler);
    assert!(
        opened.iter().any(|record| matches!(record,
            Record::Modifier(node, Modifier::Semantics { state, .. })
                if *node == settings && state & SEMANTICS_MODAL_BIT != 0)),
        "showModal() makes it modal: {opened:#?}"
    );

    let closed = click(&mut host, toggle, handler);
    assert!(
        closed.iter().any(|record| matches!(record,
            Record::Modifier(node, Modifier::Semantics { state, .. })
                if *node == settings && state & SEMANTICS_MODAL_BIT == 0)),
        "close() ends it: {closed:#?}"
    );
}

fn picker_page() -> Element {
    let mut active = use_signal(|| "opt-a");
    rsx! {
        div {
            role: "combobox",
            "aria-label": "Pick",
            "aria-activedescendant": active(),
            onclick: move |_| {
                let next = match active() {
                    "opt-a" => "opt-b",
                    _ => "",
                };
                active.set(next);
            },
            "pick"
        }
        div { role: "listbox", "aria-label": "Files",
            div { role: "option", id: "opt-a", "a.rs" }
            div { role: "option", id: "opt-b", "b.rs" }
        }
        div { role: "combobox", "aria-label": "Lost", "aria-activedescendant": "missing", "lost" }
    }
}

/// `aria-activedescendant` is sent as the renderer node of the option whose `id` it names,
/// though the option comes after the combobox. An `id` no element has sends nothing.
#[test]
fn fr34_aria_activedescendant_sends_the_option_node() {
    let mut host = Host::html(picker_page, config);
    let first = records(host.rebuild().unwrap());
    let combobox = named(&first, SemanticsRole::ComboBox, "Pick");
    let option_a = named(&first, SemanticsRole::Option, "a.rs");
    let lost = named(&first, SemanticsRole::ComboBox, "Lost");

    assert_eq!(
        active_descendants(&first),
        vec![(combobox, Some(i64::from(option_a)))],
        "{first:#?}"
    );
    assert!(
        active_descendants(&first)
            .iter()
            .all(|(node, _)| *node != lost),
        "an id nothing has is not sent"
    );
    assert!(records(host.render_frame(0).unwrap()).is_empty());
}

/// A change of `aria-activedescendant` sends one `ActiveDescendant` record on the combobox,
/// and one that names nothing clears it.
#[test]
fn fr34_aria_activedescendant_change_sends_one_record() {
    let mut host = Host::html(picker_page, config);
    let first = records(host.rebuild().unwrap());
    let combobox = named(&first, SemanticsRole::ComboBox, "Pick");
    let option_b = named(&first, SemanticsRole::Option, "b.rs");
    let handler = click_handler(&first, combobox);

    let moved = click(&mut host, combobox, handler);
    assert_eq!(
        active_descendants(&moved),
        vec![(combobox, Some(i64::from(option_b)))],
        "{moved:#?}"
    );
    let others: Vec<&Record> = moved
        .iter()
        .filter(|record| {
            !matches!(
                record,
                Record::Prop(_, PropertyKind::ActiveDescendant, _, _)
            )
        })
        .collect();
    assert!(
        others.is_empty(),
        "nothing else changed on the page: {others:#?}"
    );

    let cleared = click(&mut host, combobox, handler);
    assert_eq!(
        active_descendants(&cleared),
        vec![(combobox, None)],
        "{cleared:#?}"
    );
}
