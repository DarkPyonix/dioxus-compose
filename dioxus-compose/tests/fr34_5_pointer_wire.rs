//! The pointer subscription of an HTML page on the wire: the properties the bridge writes
//! for a node, a pointer event the renderer sends reaching the page's handler, and the
//! restyle that follows. The renderer is the mock one, so these read the records it would
//! receive.

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{HtmlConfig, PointerKind as Kind};
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    Color, EventPayload, Modifier, Paint, PointerKind, PointerType, PropertyKind, WheelDeltaMode,
};

thread_local! {
    static HEARD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn take_heard() -> Vec<String> {
    HEARD.with(|heard| std::mem::take(&mut *heard.borrow_mut()))
}

const SHEET: &str = "
html, body { margin: 0; padding: 0; }
.row { width: 100px; height: 20px; background-color: rgb(0, 0, 255); }
.row:hover { background-color: rgb(255, 0, 0); }
#a { cursor: col-resize; user-select: none; }
";

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![SHEET.to_string()],
        ..HtmlConfig::default()
    }
}

fn page() -> Element {
    rsx! {
        div { id: "a", class: "row",
            onmousemove: move |_| HEARD.with(|heard| heard.borrow_mut().push("move a".into())),
            oncontextmenu: move |event| {
                HEARD.with(|heard| heard.borrow_mut().push("contextmenu a".into()));
                event.prevent_default();
            },
        }
        div { id: "b", class: "row" }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Record {
    Int(u32, PropertyKind, i64),
    Cleared(u32, PropertyKind),
    Background(u32, Paint),
}

fn records(bytes: &[u8]) -> Vec<Record> {
    decode_batch(bytes)
        .expect("the batch decodes")
        .into_iter()
        .filter_map(|record| match record {
            Mutation::SetProp {
                node_id,
                property,
                value: PropertyValue::Integer(number),
            } => Some(Record::Int(node_id, property, number)),
            Mutation::SetProp {
                node_id,
                property,
                value: PropertyValue::None,
            } => Some(Record::Cleared(node_id, property)),
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::Background(paint),
                ..
            } => Some(Record::Background(node_id, paint)),
            _ => None,
        })
        .collect()
}

fn rgb(r: u8, g: u8, b: u8) -> Paint {
    Paint::Literal(Color::argb(
        0xff00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
    ))
}

fn value_of(records: &[Record], node: u32, property: PropertyKind) -> Option<i64> {
    records.iter().find_map(|record| match record {
        Record::Int(id, given, value) if *id == node && *given == property => Some(*value),
        _ => None,
    })
}

/// The node subscribed to `kinds` and only that, and its handler.
#[track_caller]
fn subscribed(records: &[Record], kinds: u32) -> (u32, u64) {
    let nodes: Vec<u32> = records
        .iter()
        .filter_map(|record| match record {
            Record::Int(node, PropertyKind::PointerKinds, value) if *value == i64::from(kinds) => {
                Some(*node)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        nodes.len(),
        1,
        "one node takes exactly {kinds}: {records:#?}"
    );
    let handler = value_of(records, nodes[0], PropertyKind::OnPointer)
        .expect("a subscribed node has a pointer handler");
    (nodes[0], handler as u64)
}

fn pointer(kind: PointerKind, x: f32, y: f32) -> EventPayload<'static> {
    EventPayload::Pointer {
        kind,
        pointer_id: 1,
        pointer_type: PointerType::Mouse,
        x,
        y,
        buttons: 0,
        button: 0,
        modifiers: 0,
        click_count: 0,
        delta_x: 0.0,
        delta_y: 0.0,
        delta_mode: WheelDeltaMode::Pixel,
        timestamp_nanos: 0,
    }
}

fn send(host: &mut Host, (node_id, handler_id): (u32, u64), payload: EventPayload<'static>) -> i64 {
    host.dispatch(HostEvent {
        node_id,
        handler_id,
        payload,
    })
    .expect("the pointer event is dispatched")
    .1
}

#[test]
fn fr34_5_subscription_properties_are_written() {
    let mut host = Host::html(page, config);
    let first = records(host.rebuild().unwrap());
    let (a, _) = subscribed(&first, Kind::Move.bit() | Kind::ContextMenu.bit());
    // Cursor `col-resize` is 21 and `user-select: none` is 1 on the wire.
    assert_eq!(value_of(&first, a, PropertyKind::Cursor), Some(21));
    assert_eq!(value_of(&first, a, PropertyKind::UserSelect), Some(1));
    // A node written a mask has a handler, and no mask of 0 is ever written.
    for record in &first {
        if let Record::Int(node, PropertyKind::PointerKinds, value) = record {
            assert_ne!(*value, 0);
            assert!(value_of(&first, *node, PropertyKind::OnPointer).is_some());
        }
    }
    // The other row has no listener and no cursor or selection of its own.
    let others = first
        .iter()
        .filter(|record| matches!(record, Record::Int(node, PropertyKind::Cursor, _) if *node != a))
        .count();
    assert_eq!(others, 0, "{first:#?}");
}

#[test]
fn fr34_5_pointer_event_reaches_handler_and_hovers() {
    take_heard();
    let mut host = Host::html(page, config);
    let first = records(host.rebuild().unwrap());
    let target = subscribed(&first, Kind::Move.bit() | Kind::ContextMenu.bit());
    let before = first
        .iter()
        .filter(|record| matches!(record, Record::Background(node, _) if *node == target.0))
        .count();
    assert_eq!(before, 1, "{first:#?}");

    assert_eq!(
        send(&mut host, target, pointer(PointerKind::Move, 5.0, 5.0)),
        0
    );
    assert_eq!(take_heard(), ["move a"]);
    // The next frame carries the `:hover` background and nothing else of the box.
    let next = records(host.render_frame(0).unwrap());
    assert!(
        next.contains(&Record::Background(target.0, rgb(255, 0, 0))),
        "{next:#?}"
    );

    // A handler that prevents the default answers that the event was used.
    assert_eq!(
        send(
            &mut host,
            target,
            pointer(PointerKind::ContextMenu, 5.0, 5.0)
        ),
        1
    );
    assert_eq!(take_heard(), ["contextmenu a"]);

    // The pointer leaving the page takes the hover away again.
    send(&mut host, target, pointer(PointerKind::Leave, 5.0, -40.0));
    let after = records(host.render_frame(0).unwrap());
    assert!(
        after.contains(&Record::Background(target.0, rgb(0, 0, 255))),
        "{after:#?}"
    );
}

fn changing() -> Element {
    rsx! {
        div { id: "c", class: "row", onmousemove: move |_| {} }
        div { id: "d", class: "row" }
        div { id: "e", class: "row" }
    }
}

fn changing_config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![format!(
            "{SHEET} #e {{ cursor: pointer; }} #e:hover {{ cursor: grab; }}"
        )],
        ..HtmlConfig::default()
    }
}

#[test]
fn fr34_5_root_hover_rule_works_without_a_listener() {
    // The page has `:hover` rules and no listener on `#b`, whose box is not subscribed.
    let mut host = Host::html(page, config);
    let first = records(host.rebuild().unwrap());
    let root = subscribed(&first, 63);
    let b_background = first
        .iter()
        .filter_map(|record| match record {
            Record::Background(node, _) => Some(*node),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(b_background.len() >= 2, "{first:#?}");
    // The renderer reports the pointer at (5, 25) to the root: over the second row.
    assert_eq!(
        send(&mut host, root, pointer(PointerKind::Move, 5.0, 25.0)),
        0
    );
    let next = records(host.render_frame(0).unwrap());
    assert!(
        next.iter().any(|record| matches!(
            record,
            Record::Background(_, paint) if *paint == rgb(255, 0, 0)
        )),
        "{next:#?}"
    );
    // Only the row under the pointer turned: the first row stays blue.
    let reds = next
        .iter()
        .filter(|record| matches!(record, Record::Background(_, paint) if *paint == rgb(255, 0, 0)))
        .count();
    assert_eq!(reds, 1, "{next:#?}");
}

#[test]
fn fr34_5_hover_rule_changing_only_the_cursor_is_resent() {
    let mut host = Host::html(changing, changing_config);
    let first = records(host.rebuild().unwrap());
    let root = subscribed(&first, 63);
    // `#e` is pointer (2) before the pointer is over it.
    let e = first
        .iter()
        .find_map(|record| match record {
            Record::Int(node, PropertyKind::Cursor, 2) => Some(*node),
            _ => None,
        })
        .expect("a pointer cursor is written");
    send(&mut host, root, pointer(PointerKind::Move, 5.0, 45.0));
    let next = records(host.render_frame(0).unwrap());
    // `grab` is 7. The plan node of `#e` did not change, only its cursor.
    assert!(
        next.contains(&Record::Int(e, PropertyKind::Cursor, 7)),
        "{next:#?}"
    );
}
