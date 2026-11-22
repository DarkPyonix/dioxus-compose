//! Live `HtmlRuntime` integration tests for focus synchronization (`FocusGained`,
//! `FocusLost`, `RequestFocus`) and keyboard event metadata (`keydown`, `keypress`,
//! `keyup`, physical `Code`, `Location`, `repeat`, `is_composing`).

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::dioxus_elements::input_data::keyboard_types::Modifiers;
use dioxus_compose::html::prelude::dioxus_elements::point_interaction::ModifiersInteraction;
use dioxus_compose::html::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, Key as WireKey, PropertyKind, WidgetKind};

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn push_log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn created_nodes(records: &[Mutation], kind: WidgetKind) -> Vec<u32> {
    records
        .iter()
        .filter_map(|m| match m {
            Mutation::Create { node_id, widget } if *widget == kind => Some(*node_id),
            _ => None,
        })
        .collect()
}

fn node_handler(records: &[Mutation], node: u32, prop: PropertyKind) -> Option<u64> {
    records.iter().find_map(|m| match m {
        Mutation::SetProp {
            node_id,
            property,
            value: PropertyValue::Integer(handler),
        } if *node_id == node && *property == prop => Some(*handler as u64),
        _ => None,
    })
}

fn all_handlers(records: &[Mutation], prop: PropertyKind) -> Vec<(u32, u64)> {
    records
        .iter()
        .filter_map(|m| match m {
            Mutation::SetProp {
                node_id,
                property,
                value: PropertyValue::Integer(handler),
            } if *property == prop => Some((*node_id, *handler as u64)),
            _ => None,
        })
        .collect()
}

fn request_focus_targets(records: &[Mutation]) -> Vec<u32> {
    records
        .iter()
        .filter_map(|m| match m {
            Mutation::RequestFocus { node_id } => Some(*node_id),
            _ => None,
        })
        .collect()
}

#[test]
fn fr34_5_runtime_focus_gained_and_lost_without_typing() {
    take_log();

    fn app() -> Element {
        rsx! {
            div {
                onfocusin: move |_| push_log("div:focusin"),
                onfocusout: move |_| push_log("div:focusout"),
                input {
                    value: "",
                    onfocus: move |_| push_log("input:focus"),
                    onblur: move |_| push_log("input:blur"),
                    onchange: move |_| push_log("input:change"),
                }
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let field_id = created_nodes(&first, WidgetKind::TextField)[0];
    let gained = node_handler(&first, field_id, PropertyKind::OnFocusGained)
        .expect("TextField must register OnFocusGained");
    let lost = node_handler(&first, field_id, PropertyKind::OnFocusLost)
        .expect("TextField must register OnFocusLost");

    let (gain_batch, _) = host
        .dispatch(HostEvent::new(field_id, gained, EventPayload::FocusGained))
        .expect("FocusGained dispatches");
    assert_eq!(
        take_log(),
        vec!["input:focus".to_string(), "div:focusin".to_string()],
        "FocusGained without typing must deliver focus and focusin"
    );
    assert!(
        request_focus_targets(&decode_batch(gain_batch).expect("batch decodes")).is_empty(),
        "renderer-originated FocusGained must not echo RequestFocus"
    );

    let (loss_batch, _) = host
        .dispatch(HostEvent::new(field_id, lost, EventPayload::FocusLost))
        .expect("FocusLost dispatches");
    assert_eq!(
        take_log(),
        vec!["input:blur".to_string(), "div:focusout".to_string()],
        "FocusLost without typing must deliver blur and focusout without change"
    );
    assert!(
        request_focus_targets(&decode_batch(loss_batch).expect("batch decodes")).is_empty(),
        "renderer-originated FocusLost must not echo RequestFocus"
    );
}

#[test]
fn fr34_5_runtime_tab_moves_focus_and_emits_request_focus() {
    take_log();

    fn app() -> Element {
        rsx! {
            div {
                input {
                    id: "first",
                    onfocus: move |_| push_log("first:focus"),
                    onblur: move |_| push_log("first:blur"),
                }
                input {
                    id: "second",
                    onfocus: move |_| push_log("second:focus"),
                    onblur: move |_| push_log("second:blur"),
                }
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first_batch =
        decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let fields = created_nodes(&first_batch, WidgetKind::TextField);
    assert_eq!(fields.len(), 2, "two TextFields must be created");
    let first_field = fields[0];
    let second_field = fields[1];

    let first_gained = node_handler(&first_batch, first_field, PropertyKind::OnFocusGained)
        .expect("first field registers OnFocusGained");
    let first_keydown = node_handler(&first_batch, first_field, PropertyKind::OnKeyDown)
        .expect("first field registers OnKeyDown");

    host.dispatch(HostEvent::new(
        first_field,
        first_gained,
        EventPayload::FocusGained,
    ))
    .expect("FocusGained dispatches");
    assert_eq!(take_log(), vec!["first:focus".to_string()]);

    let (tab_batch, consumed) = host
        .dispatch(HostEvent::new(
            first_field,
            first_keydown,
            EventPayload::KeyDown {
                key: WireKey::Tab,
                shift_key: false,
                ctrl_key: false,
                alt_key: false,
                meta_key: false,
                repeat: false,
                code: 0x2B,
                text: "Tab",
            },
        ))
        .expect("Tab KeyDown dispatches");
    assert_eq!(consumed, 1, "Tab focus traversal consumes the key");
    assert_eq!(
        take_log(),
        vec!["first:blur".to_string(), "second:focus".to_string()],
        "Tab must blur the first input and focus the second"
    );
    let targets = request_focus_targets(&decode_batch(tab_batch).expect("batch decodes"));
    assert_eq!(
        targets,
        vec![second_field],
        "DOM focus move on Tab must emit RequestFocus for the second TextField"
    );
}

#[test]
fn fr34_5_runtime_keydown_and_keyup_forward_metadata_and_hid_code() {
    take_log();

    fn app() -> Element {
        rsx! {
            div {
                onkeydown: move |evt| {
                    push_log(format!(
                        "down:{:?}:{:?}:{:?}:{}:{}:{}",
                        evt.key(),
                        evt.code(),
                        evt.location(),
                        evt.modifiers().contains(Modifiers::SHIFT),
                        evt.is_auto_repeating(),
                        evt.is_composing()
                    ));
                },
                onkeypress: move |evt| {
                    push_log(format!(
                        "press:{:?}:{:?}:{:?}:{}:{}",
                        evt.key(),
                        evt.code(),
                        evt.location(),
                        evt.is_auto_repeating(),
                        evt.is_composing()
                    ));
                },
                onkeyup: move |evt| {
                    push_log(format!(
                        "up:{:?}:{:?}:{:?}:{}:{}:{}",
                        evt.key(),
                        evt.code(),
                        evt.location(),
                        evt.modifiers().contains(Modifiers::SHIFT),
                        evt.is_auto_repeating(),
                        evt.is_composing()
                    ));
                },
                input { id: "editor" }
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let field_id = created_nodes(&first, WidgetKind::TextField)[0];
    let field_down = node_handler(&first, field_id, PropertyKind::OnKeyDown)
        .expect("TextField must register OnKeyDown");
    let field_up = node_handler(&first, field_id, PropertyKind::OnKeyUp)
        .expect("TextField must register OnKeyUp");
    let box_ups: Vec<(u32, u64)> = all_handlers(&first, PropertyKind::OnKeyUp)
        .into_iter()
        .filter(|(node, _)| *node != field_id)
        .collect();
    assert_eq!(
        box_ups.len(),
        1,
        "listening div box must also register OnKeyUp"
    );

    // Printable letter with USB HID usage 0x04 -> Code::KeyA, Location::Standard.
    host.dispatch(HostEvent::new(
        field_id,
        field_down,
        EventPayload::KeyDown {
            key: WireKey::Other,
            shift_key: false,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            repeat: false,
            code: 0x04,
            text: "a",
        },
    ))
    .expect("letter KeyDown dispatches");
    assert_eq!(
        take_log(),
        vec![
            "down:Character(\"a\"):KeyA:Standard:false:false:false".to_string(),
            "press:Character(\"a\"):KeyA:Standard:false:false".to_string(),
        ]
    );

    // Repeated numpad key with Shift held: USB HID 0x59 -> Code::Numpad1, Location::Numpad.
    host.dispatch(HostEvent::new(
        field_id,
        field_down,
        EventPayload::KeyDown {
            key: WireKey::Other,
            shift_key: true,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            repeat: true,
            code: 0x59,
            text: "1",
        },
    ))
    .expect("numpad repeat KeyDown dispatches");
    assert_eq!(
        take_log(),
        vec![
            "down:Character(\"1\"):Numpad1:Numpad:true:true:false".to_string(),
            "press:Character(\"1\"):Numpad1:Numpad:true:false".to_string(),
        ]
    );

    // Right Shift key: USB HID 0xE5 -> Key::Shift, Code::ShiftRight, Location::Right.
    host.dispatch(HostEvent::new(
        field_id,
        field_down,
        EventPayload::KeyDown {
            key: WireKey::Other,
            shift_key: true,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            repeat: false,
            code: 0xE5,
            text: "Shift",
        },
    ))
    .expect("ShiftRight KeyDown dispatches");
    assert_eq!(
        take_log(),
        vec!["down:Shift:ShiftRight:Right:true:false:false".to_string()]
    );

    // KeyUp on the TextField carries the same metadata.
    let (_, consumed) = host
        .dispatch(HostEvent::new(
            field_id,
            field_up,
            EventPayload::KeyUp {
                key: WireKey::Other,
                shift_key: false,
                ctrl_key: false,
                alt_key: false,
                meta_key: false,
                repeat: false,
                code: 0x04,
                text: "a",
            },
        ))
        .expect("field KeyUp dispatches");
    assert_eq!(consumed, 0, "unprevented field KeyUp is not consumed");
    assert_eq!(
        take_log(),
        vec!["up:Character(\"a\"):KeyA:Standard:false:false:false".to_string()]
    );

    // KeyUp on a listening box reaches its handler and is consumed.
    let (box_node, box_up_handler) = box_ups[0];
    let (_, box_consumed) = host
        .dispatch(HostEvent::new(
            box_node,
            box_up_handler,
            EventPayload::KeyUp {
                key: WireKey::Escape,
                shift_key: false,
                ctrl_key: false,
                alt_key: false,
                meta_key: false,
                repeat: false,
                code: 0x29,
                text: "Escape",
            },
        ))
        .expect("box KeyUp dispatches");
    assert_eq!(box_consumed, 1, "box KeyUp is consumed after bubbling");
    assert_eq!(
        take_log(),
        vec!["up:Escape:Escape:Standard:false:false:false".to_string()]
    );
}

#[test]
fn fr34_5_runtime_key_events_carry_is_composing_during_ime_composition() {
    take_log();

    fn app() -> Element {
        rsx! {
            input {
                onkeydown: move |evt| {
                    push_log(format!("down:{:?}:{}", evt.code(), evt.is_composing()));
                },
                onkeypress: move |evt| {
                    push_log(format!("press:{:?}:{}", evt.code(), evt.is_composing()));
                },
                onkeyup: move |evt| {
                    push_log(format!("up:{:?}:{}", evt.code(), evt.is_composing()));
                },
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let field_id = created_nodes(&first, WidgetKind::TextField)[0];
    let comp = node_handler(&first, field_id, PropertyKind::OnCompositionChange)
        .expect("TextField registers OnCompositionChange");
    let down = node_handler(&first, field_id, PropertyKind::OnKeyDown)
        .expect("TextField registers OnKeyDown");
    let up =
        node_handler(&first, field_id, PropertyKind::OnKeyUp).expect("TextField registers OnKeyUp");

    host.dispatch(HostEvent::new(
        field_id,
        comp,
        EventPayload::CompositionChanged { composing: true },
    ))
    .expect("composition start dispatches");

    host.dispatch(HostEvent::new(
        field_id,
        down,
        EventPayload::KeyDown {
            key: WireKey::Other,
            shift_key: false,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            repeat: false,
            code: 0x04,
            text: "a",
        },
    ))
    .expect("composing KeyDown dispatches");
    host.dispatch(HostEvent::new(
        field_id,
        up,
        EventPayload::KeyUp {
            key: WireKey::Other,
            shift_key: false,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            repeat: false,
            code: 0x04,
            text: "a",
        },
    ))
    .expect("composing KeyUp dispatches");
    assert_eq!(
        take_log(),
        vec!["down:KeyA:true".to_string(), "up:KeyA:true".to_string()],
        "while composing, keydown and keyup carry is_composing=true and keypress is suppressed"
    );

    host.dispatch(HostEvent::new(
        field_id,
        comp,
        EventPayload::CompositionChanged { composing: false },
    ))
    .expect("composition end dispatches");

    host.dispatch(HostEvent::new(
        field_id,
        down,
        EventPayload::KeyDown {
            key: WireKey::Other,
            shift_key: false,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            repeat: false,
            code: 0x04,
            text: "a",
        },
    ))
    .expect("non-composing KeyDown dispatches");
    assert_eq!(
        take_log(),
        vec![
            "down:KeyA:false".to_string(),
            "press:KeyA:false".to_string(),
        ],
        "after composition ends, is_composing returns to false and keypress resumes"
    );
}
