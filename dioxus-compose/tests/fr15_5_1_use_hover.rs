//! `use_hover`: the hover state of a node as a signal, through the public surface and the
//! mock renderer.

use std::cell::Cell;

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    EventPayload, PointerKind, PointerType, PropertyKind, WheelDeltaMode,
};

thread_local! {
    static RENDERS: Cell<u32> = const { Cell::new(0) };
    static HOVERED: Cell<bool> = const { Cell::new(false) };
    static OUTER_HEARD: Cell<u32> = const { Cell::new(0) };
}

fn row_app() -> Element {
    let hover = use_hover();
    RENDERS.with(|renders| renders.set(renders.get() + 1));
    HOVERED.with(|hovered| hovered.set(hover.is_hovered()));
    rsx! {
        Row {
            pointer_kinds: hover.kinds(),
            on_pointer: hover.handler(),
            Text { text: "row" }
        }
    }
}

fn nested_app() -> Element {
    let hover = use_hover();
    rsx! {
        Column {
            pointer_kinds: PointerKinds::ENTER,
            on_pointer: move |_| OUTER_HEARD.with(|heard| heard.set(heard.get() + 1)),
            Row {
                pointer_kinds: hover.kinds(),
                on_pointer: hover.handler(),
                Text { text: "row" }
            }
        }
    }
}

/// The node that subscribed to exactly `kinds`, and its handler.
fn subscribed(batch: &[u8], kinds: u32) -> (u32, u64) {
    let mut mask = None;
    let mut handlers = Vec::new();
    for record in decode_batch(batch).unwrap() {
        if let Mutation::SetProp {
            node_id,
            property,
            value: PropertyValue::Integer(number),
        } = record
        {
            if property == PropertyKind::PointerKinds && number == i64::from(kinds) {
                mask = Some(node_id);
            }
            if property == PropertyKind::OnPointer {
                handlers.push((node_id, number as u64));
            }
        }
    }
    let node = mask.unwrap_or_else(|| panic!("no node subscribed to {kinds}"));
    let handler = handlers
        .iter()
        .find_map(|(owner, handler)| (*owner == node).then_some(*handler))
        .expect("a subscribed node has a handler");
    (node, handler)
}

fn pointer(kind: PointerKind) -> EventPayload<'static> {
    EventPayload::Pointer {
        kind,
        pointer_id: 1,
        pointer_type: PointerType::Mouse,
        x: 1.0,
        y: 1.0,
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

/// Returns the handler result: whether the event was consumed.
fn send(host: &mut Host, (node_id, handler_id): (u32, u64), kind: PointerKind) -> i64 {
    let (_, result) = host
        .dispatch(HostEvent {
            node_id,
            handler_id,
            payload: pointer(kind),
        })
        .expect("the pointer event is dispatched");
    result
}

const ENTER_LEAVE: u32 = PointerKinds::ENTER.bits() | PointerKinds::LEAVE.bits();

#[test]
fn fr15_5_1_enter_leave_toggle_hover() {
    let mut host = Host::new(row_app);
    let target = subscribed(host.rebuild().unwrap(), ENTER_LEAVE);
    assert!(!HOVERED.with(Cell::get));
    let before = RENDERS.with(Cell::get);

    send(&mut host, target, PointerKind::Enter);
    assert!(HOVERED.with(Cell::get));
    assert_eq!(RENDERS.with(Cell::get), before + 1);

    send(&mut host, target, PointerKind::Leave);
    assert!(!HOVERED.with(Cell::get));
    assert_eq!(RENDERS.with(Cell::get), before + 2);
}

#[test]
fn fr15_5_1_repeated_enter_does_not_rerender() {
    let mut host = Host::new(row_app);
    let target = subscribed(host.rebuild().unwrap(), ENTER_LEAVE);
    send(&mut host, target, PointerKind::Enter);
    let after_first = RENDERS.with(Cell::get);
    send(&mut host, target, PointerKind::Enter);
    send(&mut host, target, PointerKind::Enter);
    assert_eq!(RENDERS.with(Cell::get), after_first);
    assert!(HOVERED.with(Cell::get));
}

#[test]
fn fr15_5_1_subscribes_enter_and_leave_only() {
    let mut host = Host::new(row_app);
    let first = host.rebuild().unwrap().to_vec();
    subscribed(&first, ENTER_LEAVE);
    let masks: Vec<i64> = decode_batch(&first)
        .unwrap()
        .into_iter()
        .filter_map(|record| match record {
            Mutation::SetProp {
                property: PropertyKind::PointerKinds,
                value: PropertyValue::Integer(number),
                ..
            } => Some(number),
            _ => None,
        })
        .collect();
    assert_eq!(masks, [i64::from(ENTER_LEAVE)]);
    assert_eq!(masks[0] & i64::from(PointerKinds::MOVE.bits()), 0);
}

#[test]
fn fr15_5_1_does_not_consume() {
    OUTER_HEARD.with(|heard| heard.set(0));
    let mut host = Host::new(nested_app);
    let target = subscribed(host.rebuild().unwrap(), ENTER_LEAVE);
    // The hover handler does not consume, so each event bubbles on to the ancestor's
    // handler as well: it hears the Enter and the Leave sent to the row.
    assert_eq!(send(&mut host, target, PointerKind::Enter), 0);
    assert_eq!(OUTER_HEARD.with(Cell::get), 1);
    assert_eq!(send(&mut host, target, PointerKind::Leave), 0);
    assert_eq!(OUTER_HEARD.with(Cell::get), 2);
}
