//! A container widget that is given `on_click` writes a `Clickable` modifier and calls the
//! handler when the Renderer reports the press; one that is not given it writes none.

use dioxus_compose::Box as ComposeBox;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch};
use dioxus_compose::schema::Modifier;
use dioxus_compose::{EventPayload, Host};
use std::cell::Cell;

thread_local! {
    static CLICKS: Cell<u32> = const { Cell::new(0) };
}

fn clicked() {
    CLICKS.with(|clicks| clicks.set(clicks.get() + 1));
}

fn clickable_nodes(batch: &[u8]) -> Vec<(u32, u64)> {
    decode_batch(batch)
        .unwrap()
        .into_iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::Clickable { handler_id },
                ..
            } => Some((node_id, handler_id)),
            _ => None,
        })
        .collect()
}

/// Builds the app, checks that one node is clickable, presses it, and checks the handler ran.
fn press_the_only_clickable_node(app: fn() -> Element) {
    CLICKS.with(|clicks| clicks.set(0));
    let mut host = Host::new(app);
    let batch = host.rebuild().unwrap().to_vec();
    let nodes = clickable_nodes(&batch);
    assert_eq!(nodes.len(), 1, "one clickable node expected");
    let (node_id, handler_id) = nodes[0];
    host.dispatch(HostEvent {
        node_id,
        handler_id,
        payload: EventPayload::Clicked,
    })
    .expect("the press is dispatched");
    assert_eq!(CLICKS.with(Cell::get), 1);
}

fn row_app() -> Element {
    rsx! { Row { on_click: move |()| clicked(), Text { text: "x" } } }
}
fn column_app() -> Element {
    rsx! { Column { on_click: move |()| clicked(), Text { text: "x" } } }
}
fn box_app() -> Element {
    rsx! { ComposeBox { on_click: move |()| clicked(), Text { text: "x" } } }
}
fn card_app() -> Element {
    rsx! { Card { on_click: move |()| clicked(), Text { text: "x" } } }
}
fn surface_app() -> Element {
    rsx! { Surface { on_click: move |()| clicked(), Text { text: "x" } } }
}
fn absolute_box_app() -> Element {
    rsx! { AbsoluteBox { on_click: move |()| clicked(), Text { text: "x" } } }
}

#[test]
fn fr15_clickable_row_sends_clickable_and_calls_handler() {
    press_the_only_clickable_node(row_app);
}

#[test]
fn fr15_clickable_column_sends_clickable_and_calls_handler() {
    press_the_only_clickable_node(column_app);
}

#[test]
fn fr15_clickable_box_sends_clickable_and_calls_handler() {
    press_the_only_clickable_node(box_app);
}

#[test]
fn fr15_clickable_card_sends_clickable_and_calls_handler() {
    press_the_only_clickable_node(card_app);
}

#[test]
fn fr15_clickable_surface_sends_clickable_and_calls_handler() {
    press_the_only_clickable_node(surface_app);
}

#[test]
fn fr15_clickable_absolute_box_sends_clickable_and_calls_handler() {
    press_the_only_clickable_node(absolute_box_app);
}

fn plain_app() -> Element {
    rsx! {
        Column {
            Row { Text { text: "a" } }
            ComposeBox { Text { text: "b" } }
            Card { Text { text: "c" } }
            Surface { Text { text: "d" } }
            AbsoluteBox { Text { text: "e" } }
        }
    }
}

#[test]
fn fr15_containers_without_on_click_send_no_clickable() {
    let mut host = Host::new(plain_app);
    let batch = host.rebuild().unwrap().to_vec();
    assert!(clickable_nodes(&batch).is_empty());
}

fn raw_app() -> Element {
    rsx! { column { onclickable: move |_| clicked(), Text { text: "x" } } }
}

#[test]
fn fr15_onclickable_is_writable_from_rsx_on_a_bare_element() {
    press_the_only_clickable_node(raw_app);
}

#[test]
fn fr15_container_without_on_click_renders_without_a_protocol_error() {
    let mut host = Host::new(plain_app);
    assert!(host.rebuild().is_ok());
}
