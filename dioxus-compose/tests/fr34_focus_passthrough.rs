//! Verifies that `autofocus`, programmatic focus requests, and renderer `FocusGained`
//! events cross the wire between `HtmlRuntime` and the renderer.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus_compose::Host;
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, Modifier, PropertyKind, WidgetKind};
use dioxus_elements::MountedData;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static TARGET: RefCell<Option<Rc<MountedData>>> = const { RefCell::new(None) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
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

#[test]
fn fr34_autofocus_button_emits_request_focus() {
    fn app() -> Element {
        rsx! {
            div {
                button { autofocus: true, "OK" }
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let targets = request_focus_targets(&first);
    assert_eq!(
        targets.len(),
        1,
        "initial frame with autofocus button must emit one RequestFocus: {targets:?}"
    );
    assert_ne!(targets[0], 0, "RequestFocus must target the button node");

    // Unchanged next frame must not re-emit RequestFocus.
    let second = decode_batch(host.render_frame(0).expect("frame encodes")).expect("batch decodes");
    assert!(
        request_focus_targets(&second).is_empty(),
        "unchanged frame must not repeat RequestFocus"
    );
}

#[test]
fn fr34_autofocus_input_emits_request_focus_on_text_field() {
    fn app() -> Element {
        rsx! {
            input { autofocus: true, value: "hello" }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let field_id = first
        .iter()
        .find_map(|m| match m {
            Mutation::Create { node_id, widget } if *widget == WidgetKind::TextField => {
                Some(*node_id)
            }
            _ => None,
        })
        .expect("a TextField must be created");
    assert_eq!(
        request_focus_targets(&first),
        vec![field_id],
        "autofocus input must emit RequestFocus for the TextField node"
    );
    assert!(
        field_handler(&first, field_id, PropertyKind::OnFocusGained).is_some(),
        "TextField must register OnFocusGained"
    );
}

#[test]
fn fr34_programmatic_focus_request_reaches_renderer() {
    TARGET.with(|slot| *slot.borrow_mut() = None);

    fn app() -> Element {
        rsx! {
            div {
                button {
                    id: "trigger",
                    onclick: move |_| {
                        if let Some(el) = TARGET.with(|slot| slot.borrow().clone()) {
                            dioxus_core::spawn(async move {
                                let _ = el.set_focus(true).await;
                            });
                        }
                    },
                    "Focus"
                }
                button {
                    id: "target",
                    onmounted: move |evt| {
                        TARGET.with(|slot| *slot.borrow_mut() = Some(evt.data()));
                    },
                    "Target"
                }
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    assert!(
        request_focus_targets(&first).is_empty(),
        "no RequestFocus before programmatic call"
    );

    let (trigger_node, trigger_handler) = first
        .iter()
        .find_map(|m| match m {
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::Clickable { handler_id },
                ..
            } => Some((*node_id, *handler_id)),
            _ => None,
        })
        .expect("trigger button must have Clickable modifier");

    let (batch, _) = host
        .dispatch(HostEvent::new(
            trigger_node,
            trigger_handler,
            EventPayload::Clicked,
        ))
        .expect("click dispatches");
    let after_click = decode_batch(batch).expect("batch decodes");
    let mut targets = request_focus_targets(&after_click);
    let next = decode_batch(host.render_frame(0).expect("frame encodes")).expect("batch decodes");
    targets.extend(request_focus_targets(&next));
    assert_eq!(
        targets.len(),
        1,
        "programmatic set_focus(true) must emit RequestFocus: {targets:?}"
    );
    assert_ne!(targets[0], 0);
}

#[test]
fn fr34_renderer_focus_gained_delivers_onfocus_without_echoing_request_focus() {
    take_log();

    fn app() -> Element {
        rsx! {
            input {
                value: "",
                onfocus: move |_| log("focused"),
                onblur: move |_| log("blurred"),
            }
        }
    }

    let mut host = Host::html(app, HtmlConfig::default);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let field_id = first
        .iter()
        .find_map(|m| match m {
            Mutation::Create { node_id, widget } if *widget == WidgetKind::TextField => {
                Some(*node_id)
            }
            _ => None,
        })
        .expect("TextField created");
    let gained = field_handler(&first, field_id, PropertyKind::OnFocusGained)
        .expect("OnFocusGained handler registered");
    let lost = field_handler(&first, field_id, PropertyKind::OnFocusLost)
        .expect("OnFocusLost handler registered");

    let (gain_batch, _) = host
        .dispatch(HostEvent::new(field_id, gained, EventPayload::FocusGained))
        .expect("FocusGained dispatches");
    assert_eq!(take_log(), vec!["focused".to_string()]);
    assert!(
        request_focus_targets(&decode_batch(gain_batch).expect("batch decodes")).is_empty(),
        "renderer-originated FocusGained must not echo RequestFocus"
    );

    let (loss_batch, _) = host
        .dispatch(HostEvent::new(field_id, lost, EventPayload::FocusLost))
        .expect("FocusLost dispatches");
    assert_eq!(take_log(), vec!["blurred".to_string()]);
    assert!(
        request_focus_targets(&decode_batch(loss_batch).expect("batch decodes")).is_empty(),
        "renderer-originated FocusLost must not echo RequestFocus(0)"
    );
}
