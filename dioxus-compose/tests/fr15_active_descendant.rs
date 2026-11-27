//! The option a combobox's keys point at, written with the Compose widgets.
//!
//! A widget names itself with `descendant_key`, and the combobox names that key with
//! `active_descendant`, the way `aria-activedescendant` names an element by its `id` on a
//! page. The renderer is sent the option's node in the `ActiveDescendant` property, cleared
//! while no widget has the key, and sent again only when it changes.

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, PropertyKind};

/// What one batch set: `(node, property, integer value, string value)`.
type Prop = (u32, PropertyKind, Option<i64>, Option<String>);

fn props(bytes: &[u8]) -> Vec<Prop> {
    decode_batch(bytes)
        .expect("the batch decodes")
        .into_iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => Some(match value {
                PropertyValue::Integer(number) => (node_id, property, Some(number), None),
                PropertyValue::String(text) => (node_id, property, None, Some(text.to_string())),
                _ => (node_id, property, None, None),
            }),
            _ => None,
        })
        .collect()
}

/// The node whose accessible name is `name`.
#[track_caller]
fn named(props: &[Prop], name: &str) -> u32 {
    props
        .iter()
        .find_map(|(node, property, _, text)| {
            (*property == PropertyKind::AccessibleName && text.as_deref() == Some(name))
                .then_some(*node)
        })
        .unwrap_or_else(|| panic!("a node named {name}: {props:#?}"))
}

/// Every `ActiveDescendant` the batch set, with the node it names (`None` clears it).
fn active_descendants(props: &[Prop]) -> Vec<(u32, Option<i64>)> {
    props
        .iter()
        .filter(|(_, property, _, _)| *property == PropertyKind::ActiveDescendant)
        .map(|(node, _, value, _)| (*node, *value))
        .collect()
}

fn picker() -> Element {
    let mut active = use_signal(|| "a");
    rsx! {
        Column {
            Column { accessible_name: "Pick", active_descendant: active(),
                Text { text: "pick" }
            }
            Column {
                Row { accessible_name: "a.rs", descendant_key: "a", Text { text: "a.rs" } }
                Row { accessible_name: "b.rs", descendant_key: "b", Text { text: "b.rs" } }
            }
            Column { accessible_name: "Lost", active_descendant: "missing",
                Text { text: "lost" }
            }
            Button {
                text: "Next",
                accessible_name: "Next",
                on_click: move |_| {
                    let next = if active() == "a" { "b" } else { "" };
                    active.set(next);
                },
            }
        }
    }
}

/// `active_descendant` is sent as the node of the widget whose `descendant_key` it names,
/// though that widget comes after it. A key no widget has sends nothing.
#[test]
fn fr15_active_descendant_names_the_keyed_widget() {
    let mut host = Host::new(picker);
    let first = props(host.rebuild().unwrap());
    let combobox = named(&first, "Pick");
    let option_a = named(&first, "a.rs");
    let lost = named(&first, "Lost");

    assert_eq!(
        active_descendants(&first),
        vec![(combobox, Some(i64::from(option_a)))],
        "{first:#?}"
    );
    assert!(
        active_descendants(&first)
            .iter()
            .all(|(node, _)| *node != lost),
        "a key no widget has is not sent"
    );
}

/// A change of `active_descendant` sends one `ActiveDescendant` record on the combobox, and
/// one that names nothing clears it.
#[test]
fn fr15_active_descendant_change_sends_one_record() {
    let mut host = Host::new(picker);
    let first = props(host.rebuild().unwrap());
    let combobox = named(&first, "Pick");
    let option_b = named(&first, "b.rs");
    let next = named(&first, "Next");
    let handler = first
        .iter()
        .find_map(|(node, property, value, _)| {
            (*node == next && *property == PropertyKind::OnClick)
                .then_some(*value)
                .flatten()
        })
        .expect("the button has a click handler") as u64;

    let (batch, _) = host
        .dispatch(HostEvent::new(next, handler, EventPayload::Clicked))
        .expect("the click is dispatched");
    let moved = props(batch);
    assert_eq!(
        active_descendants(&moved),
        vec![(combobox, Some(i64::from(option_b)))],
        "{moved:#?}"
    );

    let (batch, _) = host
        .dispatch(HostEvent::new(next, handler, EventPayload::Clicked))
        .expect("the click is dispatched");
    let cleared = props(batch);
    assert_eq!(
        active_descendants(&cleared),
        vec![(combobox, None)],
        "{cleared:#?}"
    );
}
