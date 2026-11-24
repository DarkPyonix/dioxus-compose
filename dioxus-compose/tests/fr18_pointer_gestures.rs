//! Discrete pointer gesture attributes on widgets, backed by `pointer_kinds` and
//! `on_pointer` on the wire.

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    EventPayload, PointerKind, PointerType, PropertyKind, WheelDeltaMode,
};

thread_local! {
    static HEARD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn record(label: impl Into<String>) {
    HEARD.with(|heard| heard.borrow_mut().push(label.into()));
}

fn take_heard() -> Vec<String> {
    HEARD.with(|heard| std::mem::take(&mut *heard.borrow_mut()))
}

fn pointer_payload(kind: PointerKind) -> EventPayload<'static> {
    EventPayload::Pointer {
        kind,
        pointer_id: 1,
        pointer_type: PointerType::Mouse,
        x: 12.0,
        y: 24.0,
        buttons: 0,
        button: 0,
        modifiers: 0,
        click_count: 1,
        delta_x: 3.0,
        delta_y: -6.0,
        delta_mode: WheelDeltaMode::Pixel,
        timestamp_nanos: 100,
    }
}

/// Finds the single node subscribed to exactly `kinds`, and returns `(node_id, handler_id)`.
#[track_caller]
fn subscribed(batch: &[u8], kinds: PointerKinds) -> (u32, u64) {
    let mut matching_nodes = Vec::new();
    let mut handlers = Vec::new();
    for mutation in decode_batch(batch).expect("batch decodes") {
        if let Mutation::SetProp {
            node_id,
            property,
            value: PropertyValue::Integer(number),
        } = mutation
        {
            if property == PropertyKind::PointerKinds && number == i64::from(kinds.bits()) {
                matching_nodes.push(node_id);
            }
            if property == PropertyKind::OnPointer {
                handlers.push((node_id, number as u64));
            }
        }
    }
    assert_eq!(
        matching_nodes.len(),
        1,
        "expected exactly one node subscribed to {kinds:?} (bits {})",
        kinds.bits()
    );
    let node_id = matching_nodes[0];
    let handler_id = handlers
        .iter()
        .find_map(|(owner, handler)| (*owner == node_id).then_some(*handler))
        .expect("subscribed node must announce OnPointer");
    (node_id, handler_id)
}

fn send(host: &mut Host, (node_id, handler_id): (u32, u64), kind: PointerKind) -> i64 {
    let (_, result) = host
        .dispatch(HostEvent::new(node_id, handler_id, pointer_payload(kind)))
        .expect("pointer event dispatches");
    result
}

#[test]
fn fr18_unsubscribed_widgets_emit_no_pointer_properties() {
    fn app() -> Element {
        rsx! {
            Column {
                Row {
                    Text { text: "plain" }
                }
            }
        }
    }
    let mut host = Host::new(app);
    let batch = host.rebuild().unwrap();
    for mutation in decode_batch(batch).unwrap() {
        if let Mutation::SetProp { property, .. } = mutation {
            assert_ne!(property, PropertyKind::PointerKinds);
            assert_ne!(property, PropertyKind::OnPointer);
        }
    }
}

#[test]
fn fr18_on_pointer_enter_registers_enter_bit_and_routes() {
    fn app() -> Element {
        rsx! {
            Row {
                on_pointer_enter: move |ev: PointerEvent| {
                    record(format!("enter:{:?}:{}:{}", ev.kind(), ev.x(), ev.y()));
                },
                Text { text: "enter" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::ENTER);
    assert_eq!(send(&mut host, target, PointerKind::Leave), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::Enter), 0);
    assert_eq!(take_heard(), ["enter:Enter:12:24"]);
}

#[test]
fn fr18_on_pointer_leave_registers_leave_bit_and_routes() {
    fn app() -> Element {
        rsx! {
            Column {
                on_pointer_leave: move |ev: PointerEvent| {
                    record(format!("leave:{:?}", ev.kind()));
                },
                Text { text: "leave" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::LEAVE);
    assert_eq!(send(&mut host, target, PointerKind::Enter), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::Leave), 0);
    assert_eq!(take_heard(), ["leave:Leave"]);
}

#[test]
fn fr18_on_pointer_move_registers_move_bit_and_routes() {
    fn app() -> Element {
        rsx! {
            Card {
                on_pointer_move: move |ev: PointerEvent| {
                    record(format!("move:{:?}", ev.kind()));
                },
                Text { text: "move" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::MOVE);
    assert_eq!(send(&mut host, target, PointerKind::Down), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::Move), 0);
    assert_eq!(take_heard(), ["move:Move"]);
}

#[test]
fn fr18_on_long_press_registers_down_bit_and_routes() {
    fn app() -> Element {
        rsx! {
            Surface {
                on_long_press: move |ev: PointerEvent| {
                    record(format!("long_press:{:?}", ev.kind()));
                },
                Text { text: "long_press" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::DOWN);
    assert_eq!(send(&mut host, target, PointerKind::Up), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::Down), 0);
    assert_eq!(take_heard(), ["long_press:Down"]);
}

#[test]
fn fr18_on_context_menu_registers_context_menu_bit_and_consumes() {
    fn app() -> Element {
        rsx! {
            dioxus_compose::Box {
                on_context_menu: move |ev: PointerEvent| {
                    record(format!("context_menu:{:?}", ev.kind()));
                    ev.consume();
                },
                Text { text: "menu" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::CONTEXT_MENU);
    assert_eq!(send(&mut host, target, PointerKind::Down), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::ContextMenu), 1);
    assert_eq!(take_heard(), ["context_menu:ContextMenu"]);
}

#[test]
fn fr18_on_drag_start_registers_down_bit_and_routes() {
    fn app() -> Element {
        rsx! {
            AbsoluteBox {
                on_drag_start: move |ev: PointerEvent| {
                    record(format!("drag_start:{:?}", ev.kind()));
                },
                Text { text: "drag_start" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::DOWN);
    assert_eq!(send(&mut host, target, PointerKind::Move), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::Down), 0);
    assert_eq!(take_heard(), ["drag_start:Down"]);
}

#[test]
fn fr18_on_drag_move_registers_move_bit_and_routes() {
    fn app() -> Element {
        rsx! {
            ScrollColumn {
                on_drag_move: move |ev: PointerEvent| {
                    record(format!("drag_move:{:?}", ev.kind()));
                },
                Text { text: "drag_move" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::MOVE);
    assert_eq!(send(&mut host, target, PointerKind::Up), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::Move), 0);
    assert_eq!(take_heard(), ["drag_move:Move"]);
}

#[test]
fn fr18_on_drag_end_registers_up_and_cancel_bits_and_routes() {
    fn app() -> Element {
        rsx! {
            ScrollRow {
                on_drag_end: move |ev: PointerEvent| {
                    record(format!("drag_end:{:?}", ev.kind()));
                },
                Text { text: "drag_end" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(
        host.rebuild().unwrap(),
        PointerKinds::UP | PointerKinds::CANCEL,
    );
    assert_eq!(send(&mut host, target, PointerKind::Move), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::Up), 0);
    assert_eq!(send(&mut host, target, PointerKind::Cancel), 0);
    assert_eq!(take_heard(), ["drag_end:Up", "drag_end:Cancel"]);
}

#[test]
fn fr18_on_scroll_registers_wheel_bit_and_routes() {
    fn app() -> Element {
        rsx! {
            SelectionContainer {
                on_scroll: move |ev: PointerEvent| {
                    record(format!("scroll:{:?}", ev.kind()));
                },
                Text { text: "scroll" }
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::WHEEL);
    assert_eq!(send(&mut host, target, PointerKind::Move), 0);
    assert!(take_heard().is_empty());
    assert_eq!(send(&mut host, target, PointerKind::Wheel), 0);
    assert_eq!(take_heard(), ["scroll:Wheel"]);
}

#[test]
fn fr18_combined_gestures_and_explicit_on_pointer_union_masks_and_dispatch() {
    fn app() -> Element {
        rsx! {
            Text {
                text: "combined",
                pointer_kinds: PointerKinds::WHEEL,
                on_pointer: move |ev: PointerEvent| {
                    record(format!("raw:{:?}", ev.kind()));
                },
                on_drag_start: move |_| record("start"),
                on_drag_move: move |_| record("move"),
                on_drag_end: move |_| record("end"),
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let expected = PointerKinds::WHEEL
        | PointerKinds::DOWN
        | PointerKinds::MOVE
        | PointerKinds::UP
        | PointerKinds::CANCEL;
    let target = subscribed(host.rebuild().unwrap(), expected);

    send(&mut host, target, PointerKind::Down);
    send(&mut host, target, PointerKind::Move);
    send(&mut host, target, PointerKind::Up);
    send(&mut host, target, PointerKind::Wheel);

    assert_eq!(
        take_heard(),
        [
            "start",
            "raw:Down",
            "move",
            "raw:Move",
            "end",
            "raw:Up",
            "raw:Wheel",
        ]
    );
}

#[test]
fn fr18_spacer_accepts_discrete_gesture() {
    fn app() -> Element {
        rsx! {
            Spacer {
                on_pointer_enter: move |_| record("spacer_enter"),
            }
        }
    }
    take_heard();
    let mut host = Host::new(app);
    let target = subscribed(host.rebuild().unwrap(), PointerKinds::ENTER);
    send(&mut host, target, PointerKind::Enter);
    assert_eq!(take_heard(), ["spacer_enter"]);
}
