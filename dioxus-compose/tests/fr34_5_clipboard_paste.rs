//! HTML clipboard (paste, copy, cut) and IME composition events.
//!
//! Verifies:
//! 1. `dom.paste` delivers `ClipboardEventData` and bubbles up the DOM hierarchy.
//! 2. `dom.paste` into an `<input>` or `<textarea>` updates its value and emits `input`
//!    unless prevented or the field is disabled/readonly.
//! 3. `dom.copy` and `dom.cut` dispatch clipboard events to handlers.
//! 4. Live Host: Mod+V on focused input issues `Mutation::ReadClipboard`, and the renderer's
//!    `EventPayload::ClipboardText` reply delivers `onpaste` and updates the field.
//! 5. Live Host: `EventPayload::CompositionChanged` and `TextChanged` deliver
//!    `compositionstart`, `compositionupdate`, and `compositionend`.

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{ClipboardEventData, HtmlConfig, HtmlDom};
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, Key as WireKey, PropertyKind, WidgetKind};

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[test]
fn fr34_5_paste_delivers_clipboard_event_data_and_bubbles() {
    take_log();

    fn page() -> Element {
        rsx! {
            div {
                id: "parent",
                onpaste: move |evt| {
                    let text = evt
                        .data()
                        .downcast::<ClipboardEventData>()
                        .map(|d| d.text().to_string())
                        .unwrap_or_default();
                    log(format!("parent paste: {text}"));
                },
                span {
                    id: "child",
                    onpaste: move |evt| {
                        let text = evt
                            .data()
                            .downcast::<ClipboardEventData>()
                            .map(|d| d.text().to_string())
                            .unwrap_or_default();
                        log(format!("child paste: {text}"));
                    },
                    "Target"
                }
            }
        }
    }

    let mut dom = HtmlDom::new(page);
    let child = dom.element_by_id("child").expect("child exists");

    dom.paste(child, "pasted text");
    dom.render();

    assert_eq!(
        take_log(),
        vec![
            "child paste: pasted text".to_string(),
            "parent paste: pasted text".to_string(),
        ]
    );

    // When child stops propagation, parent does not receive paste.
    fn page_stopped() -> Element {
        rsx! {
            div {
                id: "parent",
                onpaste: move |_| log("parent received"),
                span {
                    id: "child",
                    onpaste: move |evt| {
                        evt.stop_propagation();
                        log("child stopped");
                    },
                    "Target"
                }
            }
        }
    }

    let mut dom = HtmlDom::new(page_stopped);
    let child = dom.element_by_id("child").expect("child exists");
    dom.paste(child, "text");
    dom.render();

    assert_eq!(take_log(), vec!["child stopped".to_string()]);
}

#[test]
fn fr34_5_paste_into_input_updates_value_and_fires_input() {
    take_log();

    fn page() -> Element {
        rsx! {
            div {
                input {
                    id: "text-input",
                    value: "hello ",
                    oninput: move |evt| log(format!("input: {}", evt.value())),
                }
                textarea {
                    id: "text-area",
                    value: "line1 ",
                    oninput: move |evt| log(format!("textarea: {}", evt.value())),
                }
                input {
                    id: "prevented-input",
                    value: "stay ",
                    onpaste: move |evt| {
                        evt.prevent_default();
                        log("prevented paste");
                    },
                    oninput: move |evt| log(format!("prevented input: {}", evt.value())),
                }
                input {
                    id: "disabled-input",
                    value: "fixed ",
                    disabled: true,
                    oninput: move |evt| log(format!("disabled input: {}", evt.value())),
                }
            }
        }
    }

    let mut dom = HtmlDom::new(page);
    let text_input = dom.element_by_id("text-input").expect("text-input exists");
    let text_area = dom.element_by_id("text-area").expect("text-area exists");
    let prevented = dom
        .element_by_id("prevented-input")
        .expect("prevented-input exists");
    let disabled = dom
        .element_by_id("disabled-input")
        .expect("disabled-input exists");

    // 1. Paste into unprevented input appends text and emits input.
    dom.paste(text_input, "world");
    dom.render();
    assert_eq!(take_log(), vec!["input: hello world".to_string()]);

    // 2. Paste into textarea appends text and emits input.
    dom.paste(text_area, "line2");
    dom.render();
    assert_eq!(take_log(), vec!["textarea: line1 line2".to_string()]);

    // 3. Prevent default paste suppresses value update and input event.
    dom.paste(prevented, "fail");
    dom.render();
    assert_eq!(take_log(), vec!["prevented paste".to_string()]);

    // 4. Disabled input ignores default paste.
    dom.paste(disabled, "blocked");
    dom.render();
    assert!(take_log().is_empty());
}

#[test]
fn fr34_5_copy_and_cut_dispatch_clipboard_events() {
    take_log();

    fn page() -> Element {
        rsx! {
            div {
                id: "target",
                oncopy: move |_| log("copy event"),
                oncut: move |_| log("cut event"),
            }
        }
    }

    let mut dom = HtmlDom::new(page);
    let target = dom.element_by_id("target").expect("target exists");

    dom.copy(target);
    dom.render();
    assert_eq!(take_log(), vec!["copy event".to_string()]);

    dom.cut(target);
    dom.render();
    assert_eq!(take_log(), vec!["cut event".to_string()]);
}

fn field_handler(records: &[Mutation], node: u32, prop: PropertyKind) -> Option<u64> {
    records.iter().find_map(|m| match m {
        Mutation::SetProp {
            node_id,
            property,
            value: PropertyValue::Integer(handler),
        } if *node_id == node && *property == prop => Some(*handler as u64),
        _ => None,
    })
}

fn created_text_field(records: &[Mutation]) -> u32 {
    records
        .iter()
        .find_map(|m| match m {
            Mutation::Create { node_id, widget } if *widget == WidgetKind::TextField => {
                Some(*node_id)
            }
            _ => None,
        })
        .expect("a TextField must be created")
}

#[test]
fn fr34_5_mod_v_reads_clipboard_and_dispatches_paste() {
    take_log();

    fn app() -> Element {
        rsx! {
            input {
                id: "editor",
                value: "prefix: ",
                onpaste: move |evt| {
                    let text = evt
                        .data()
                        .downcast::<ClipboardEventData>()
                        .map(|d| d.text().to_string())
                        .unwrap_or_default();
                    log(format!("onpaste: {text}"));
                },
                oninput: move |evt| log(format!("oninput: {}", evt.value())),
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first_batch = host.rebuild().expect("rebuild succeeds");
    let mutations = decode_batch(first_batch).expect("batch decodes");
    let field = created_text_field(&mutations);
    let key_handler = field_handler(&mutations, field, PropertyKind::OnKeyDown)
        .expect("field has OnKeyDown handler");

    // 1. Dispatch Mod+V keydown event.
    let is_mac = cfg!(target_os = "macos");
    let (key_batch, _) = host
        .dispatch(HostEvent {
            node_id: field,
            handler_id: key_handler,
            payload: EventPayload::KeyDown {
                key: WireKey::Other,
                shift_key: false,
                ctrl_key: !is_mac,
                alt_key: false,
                meta_key: is_mac,
                repeat: false,
                code: 0,
                text: "v",
            },
        })
        .expect("keydown dispatches");

    let key_mutations = decode_batch(key_batch).expect("key batch decodes");
    let read_request = key_mutations
        .into_iter()
        .find_map(|m| match m {
            Mutation::ReadClipboard { request } => Some(request),
            _ => None,
        })
        .expect("Mod+V must emit Mutation::ReadClipboard");

    // 2. Dispatch renderer's ClipboardText reply with status 1 (TEXT).
    let clip_text = "pasted";
    let (reply_batch, _) = host
        .dispatch(HostEvent {
            node_id: 0,
            handler_id: 0,
            payload: EventPayload::ClipboardText {
                request: read_request,
                status: 1,
                len: clip_text.len() as u32,
                text: clip_text,
            },
        })
        .expect("clipboard reply dispatches");

    let _ = reply_batch;

    assert_eq!(
        take_log(),
        vec![
            "onpaste: pasted".to_string(),
            "oninput: prefix: pasted".to_string(),
        ]
    );
}

#[test]
fn fr34_5_composition_events_from_live_renderer() {
    take_log();

    fn app() -> Element {
        rsx! {
            input {
                id: "korean-input",
                oncompositionstart: move |evt| log(format!("start: {}", evt.data().data())),
                oncompositionupdate: move |evt| log(format!("update: {}", evt.data().data())),
                oncompositionend: move |evt| log(format!("end: {}", evt.data().data())),
                oninput: move |evt| log(format!("input: {}", evt.value())),
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first_batch = host.rebuild().expect("rebuild succeeds");
    let mutations = decode_batch(first_batch).expect("batch decodes");
    let field = created_text_field(&mutations);
    let comp_handler = field_handler(&mutations, field, PropertyKind::OnCompositionChange)
        .expect("field has OnCompositionChange handler");
    let text_handler = field_handler(&mutations, field, PropertyKind::OnValueChange)
        .expect("field has OnValueChange handler");

    // 1. Composition starts: composing = true
    host.dispatch(HostEvent {
        node_id: field,
        handler_id: comp_handler,
        payload: EventPayload::CompositionChanged { composing: true },
    })
    .expect("composition start dispatches");

    assert_eq!(take_log(), vec!["start: ".to_string()]);

    // 2. IME updates composed text: TextChanged("ㅎ")
    host.dispatch(HostEvent {
        node_id: field,
        handler_id: text_handler,
        payload: EventPayload::TextChanged("ㅎ"),
    })
    .expect("text change dispatches");

    assert_eq!(
        take_log(),
        vec!["update: ㅎ".to_string(), "input: ㅎ".to_string()]
    );

    // 3. IME updates composed text again: TextChanged("하")
    host.dispatch(HostEvent {
        node_id: field,
        handler_id: text_handler,
        payload: EventPayload::TextChanged("하"),
    })
    .expect("text change dispatches");

    assert_eq!(
        take_log(),
        vec!["update: 하".to_string(), "input: 하".to_string()]
    );

    // 4. Composition commits / ends: composing = false
    host.dispatch(HostEvent {
        node_id: field,
        handler_id: comp_handler,
        payload: EventPayload::CompositionChanged { composing: false },
    })
    .expect("composition end dispatches");

    assert_eq!(take_log(), vec!["end: 하".to_string()]);
}
