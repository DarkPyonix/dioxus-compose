//! Regression tests for removing focused, hovered, or active DOM nodes.
//!
//! When an overlay or panel with a focused input or hovered/active element unmounts (for
//! example when closing a command palette by clicking outside or resizing the viewport),
//! subsequent pointer reports, focus transitions, or blur events must never index a
//! dropped node id in `blitz-dom`'s slab.

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::html::prelude::dioxus_signals::{ReadableExt, WritableExt, use_signal};
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{HtmlConfig, HtmlDom, PointerInput, PointerKind as DomPointerKind};
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    EventPayload, Key as WireKey, Modifier, PointerKind, PointerType, PropertyKind,
    WheelDeltaMode, WidgetKind,
};
use dioxus_elements::Key;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|entries| entries.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|entries| std::mem::take(&mut *entries.borrow_mut()))
}

const PALETTE_CSS: &str = "
html, body { margin: 0; padding: 0; width: 400px; height: 300px; }
#outside { width: 200px; height: 40px; background-color: rgb(200, 200, 200); }
#outside:hover { background-color: rgb(180, 180, 180); }
#outside:active { background-color: rgb(160, 160, 160); }
#backdrop { position: fixed; left: 0; top: 0; width: 400px; height: 300px; }
#backdrop:hover { background-color: rgba(0, 0, 0, 0.2); }
#backdrop:active { background-color: rgba(0, 0, 0, 0.3); }
#palette { position: fixed; left: 80px; top: 40px; width: 240px; height: 120px; }
#palette-input { width: 200px; height: 28px; }
#outside-input { width: 200px; height: 28px; }
";

fn palette_config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![PALETTE_CSS.to_string()],
        ..HtmlConfig::default()
    }
}

fn pointer_payload(kind: PointerKind, x: f32, y: f32) -> EventPayload<'static> {
    EventPayload::Pointer {
        kind,
        pointer_id: 1,
        pointer_type: PointerType::Mouse,
        x,
        y,
        buttons: u32::from(kind == PointerKind::Down),
        button: u32::from(matches!(kind, PointerKind::Down | PointerKind::Up)),
        modifiers: 0,
        click_count: u32::from(matches!(kind, PointerKind::Down | PointerKind::Up)),
        delta_x: 0.0,
        delta_y: 0.0,
        delta_mode: WheelDeltaMode::Pixel,
        timestamp_nanos: 0,
    }
}

fn root_pointer_handler(mutations: &[Mutation]) -> (u32, u64) {
    mutations
        .iter()
        .find_map(|m| match m {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnPointer,
                value: PropertyValue::Integer(handler),
            } => Some((*node_id, *handler as u64)),
            _ => None,
        })
        .expect("page root must register an OnPointer handler")
}

fn prop_handler(mutations: &[Mutation], target_node: u32, prop: PropertyKind) -> Option<u64> {
    mutations.iter().find_map(|m| match m {
        Mutation::SetProp {
            node_id,
            property,
            value: PropertyValue::Integer(handler),
        } if *node_id == target_node && *property == prop => Some(*handler as u64),
        _ => None,
    })
}

#[test]
fn fr34_unmounting_focused_overlay_then_clicking_outside_does_not_panic() {
    fn app() -> Element {
        let mut open = use_signal(|| true);
        rsx! {
            div { id: "root",
                button { id: "outside", onclick: move |_| {}, "Outside" }
                if *open.read() {
                    div {
                        id: "backdrop",
                        onclick: move |_| open.set(false),
                        div { id: "palette",
                            input { id: "palette-input", autofocus: true, value: "" }
                        }
                    }
                }
            }
        }
    }

    let mut dom = HtmlDom::with_config(app, palette_config());
    dom.layout(400.0, 300.0, 1.0);

    let input = dom
        .element_by_id("palette-input")
        .expect("palette input exists");
    let backdrop = dom.element_by_id("backdrop").expect("backdrop exists");
    let outside = dom.element_by_id("outside").expect("outside exists");
    let html_root = dom.document().root_element().id;
    assert_eq!(dom.focused(), Some(input));

    // Hover and press down on the backdrop so pointer hover, active, and capture hold it.
    dom.pointer_event(html_root, &PointerInput::new(DomPointerKind::Move, 10.0, 10.0));
    dom.pointer_event(html_root, &PointerInput::press(10.0, 10.0, 1, 1));
    assert!(dom.is_hovered(backdrop));
    assert!(dom.is_active(backdrop));

    // Clicking the backdrop closes the palette and removes both backdrop and focused input.
    dom.click_node(backdrop);
    dom.render();
    assert!(dom.element_by_id("palette-input").is_none());
    assert!(dom.element_by_id("backdrop").is_none());
    assert_eq!(
        dom.focused(),
        None,
        "document focus must be cleared before dropping the focused node"
    );

    dom.layout(400.0, 300.0, 1.0);

    // Releasing the pointer, moving over `#outside`, and moving focus must not panic.
    dom.pointer_event(html_root, &PointerInput::release(10.0, 10.0, 1, 1));
    dom.pointer_event(html_root, &PointerInput::new(DomPointerKind::Move, 20.0, 20.0));
    dom.focus(outside);
    assert_eq!(dom.focused(), Some(outside));

    // Stale focus/blur calls targeting the dropped input id must also be no-ops.
    dom.blur(input);
    dom.focus(input);
    assert_eq!(dom.focused(), Some(outside));
}

#[test]
fn fr34_unmounting_hovered_and_active_element_on_resize_does_not_panic() {
    const RESIZE_CSS: &str = "
    html, body { margin: 0; padding: 0; width: 100%; height: 100%; }
    #container { width: 100%; height: 200px; }
    #panel { width: 200px; height: 100px; background-color: rgb(100, 100, 100); }
    #panel:hover { background-color: rgb(150, 150, 150); }
    #panel:active { background-color: rgb(200, 200, 200); }
    ";

    fn app() -> Element {
        let mut wide = use_signal(|| true);
        rsx! {
            div {
                id: "container",
                onresize: move |evt| {
                    if let Ok(size) = evt.data().get_content_box_size() {
                        wide.set(size.width >= 500.0);
                    }
                },
                if *wide.read() {
                    div { id: "panel",
                        input { id: "panel-input", autofocus: true, value: "" }
                    }
                }
            }
        }
    }

    let mut dom = HtmlDom::with_config(
        app,
        HtmlConfig {
            stylesheets: vec![RESIZE_CSS.to_string()],
            ..HtmlConfig::default()
        },
    );
    dom.layout(800.0, 600.0, 1.0);
    dom.render();
    dom.layout(800.0, 600.0, 1.0);

    let panel = dom.element_by_id("panel").expect("panel exists at wide width");
    let html_root = dom.document().root_element().id;
    dom.pointer_event(html_root, &PointerInput::new(DomPointerKind::Move, 20.0, 20.0));
    dom.pointer_event(html_root, &PointerInput::press(20.0, 20.0, 1, 1));
    assert!(dom.is_hovered(panel));
    assert!(dom.is_active(panel));

    // Shrink the viewport: onresize unmounts the hovered, active, focused panel.
    dom.layout(400.0, 600.0, 1.0);
    dom.render();
    assert!(dom.element_by_id("panel").is_none());
    assert_eq!(dom.focused(), None);
    dom.layout(400.0, 600.0, 1.0);

    // Subsequent pointer release and move must not panic on the removed panel node.
    dom.pointer_event(html_root, &PointerInput::release(20.0, 20.0, 1, 1));
    dom.pointer_event(html_root, &PointerInput::new(DomPointerKind::Move, 30.0, 30.0));
}

#[test]
fn fr34_host_dispatch_overlay_close_and_click_outside_returns_ok() {
    take_log();

    fn app() -> Element {
        let mut open = use_signal(|| true);
        rsx! {
            div { id: "root",
                input { id: "outside-input", value: "" }
                button {
                    id: "outside",
                    onclick: move |_| log("outside clicked"),
                    "Outside"
                }
                if *open.read() {
                    div {
                        id: "backdrop",
                        onclick: move |_| {
                            log("backdrop clicked");
                            open.set(false);
                        },
                        div { id: "palette",
                            input { id: "palette-input", autofocus: true, value: "" }
                        }
                    }
                }
            }
        }
    }

    let mut host = Host::html(app, palette_config);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let (pointer_node, pointer_handler) = root_pointer_handler(&first);

    let text_fields: Vec<u32> = first
        .iter()
        .filter_map(|m| match m {
            Mutation::Create { node_id, widget } if *widget == WidgetKind::TextField => {
                Some(*node_id)
            }
            _ => None,
        })
        .collect();
    assert_eq!(text_fields.len(), 2, "expected outside and palette inputs");
    let outside_field = text_fields[0];
    let outside_focus_gained = prop_handler(&first, outside_field, PropertyKind::OnFocusGained)
        .expect("outside input registers OnFocusGained");

    let clickables: Vec<(u32, u64)> = first
        .iter()
        .filter_map(|m| match m {
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::Clickable { handler_id },
                ..
            } => Some((*node_id, *handler_id)),
            _ => None,
        })
        .collect();
    assert_eq!(clickables.len(), 2, "expected outside button and backdrop");
    let (outside_btn_node, outside_btn_handler) = clickables[0];
    let (backdrop_node, backdrop_handler) = clickables[1];

    // Hover and press down on the backdrop.
    host.dispatch(HostEvent::new(
        pointer_node,
        pointer_handler,
        pointer_payload(PointerKind::Move, 10.0, 10.0),
    ))
    .expect("hover on backdrop must succeed");
    host.dispatch(HostEvent::new(
        pointer_node,
        pointer_handler,
        pointer_payload(PointerKind::Down, 10.0, 10.0),
    ))
    .expect("press on backdrop must succeed");

    // Click backdrop to close the palette (unmounts backdrop and focused palette input).
    host.dispatch(HostEvent::new(
        backdrop_node,
        backdrop_handler,
        EventPayload::Clicked,
    ))
    .expect("click closing palette must succeed");

    // Pointer release, pointer move outside, focus on outside input, and click outside
    // must all return Ok without panicking in blitz-dom.
    host.dispatch(HostEvent::new(
        pointer_node,
        pointer_handler,
        pointer_payload(PointerKind::Up, 10.0, 10.0),
    ))
    .expect("pointer up after palette close must succeed");
    host.dispatch(HostEvent::new(
        pointer_node,
        pointer_handler,
        pointer_payload(PointerKind::Move, 20.0, 15.0),
    ))
    .expect("pointer move outside after palette close must succeed");
    host.dispatch(HostEvent::new(
        outside_field,
        outside_focus_gained,
        EventPayload::FocusGained,
    ))
    .expect("focusing outside input after palette close must succeed");
    host.dispatch(HostEvent::new(
        outside_btn_node,
        outside_btn_handler,
        EventPayload::Clicked,
    ))
    .expect("clicking outside button after palette close must succeed");

    assert_eq!(
        take_log(),
        vec![
            "backdrop clicked".to_string(),
            "outside clicked".to_string(),
        ]
    );
}

#[test]
fn fr34_host_escape_on_focused_input_reaches_onkeydown_and_closes_palette() {
    take_log();

    fn app() -> Element {
        let mut open = use_signal(|| true);
        rsx! {
            div { id: "root",
                if *open.read() {
                    div {
                        id: "palette",
                        onkeydown: move |evt| {
                            if evt.key() == Key::Escape {
                                evt.prevent_default();
                                log("palette escape");
                                open.set(false);
                            }
                        },
                        input { id: "palette-input", autofocus: true, value: "" }
                    }
                }
            }
        }
    }

    let mut host = Host::html(app, palette_config);
    let first = decode_batch(host.rebuild().expect("rebuild encodes")).expect("batch decodes");
    let (pointer_node, pointer_handler) = root_pointer_handler(&first);
    let field_node = first
        .iter()
        .find_map(|m| match m {
            Mutation::Create { node_id, widget } if *widget == WidgetKind::TextField => {
                Some(*node_id)
            }
            _ => None,
        })
        .expect("palette TextField must be created");
    let field_keydown = prop_handler(&first, field_node, PropertyKind::OnKeyDown)
        .expect("TextField must register OnKeyDown");

    // Hover palette before pressing Escape.
    host.dispatch(HostEvent::new(
        pointer_node,
        pointer_handler,
        pointer_payload(PointerKind::Move, 100.0, 60.0),
    ))
    .expect("hover on palette must succeed");

    // Escape on the focused TextField must bubble to the palette container's onkeydown
    // and close the palette cleanly.
    let (_, consumed) = host
        .dispatch(HostEvent::new(
            field_node,
            field_keydown,
            EventPayload::KeyDown {
                key: WireKey::Escape,
                shift_key: false,
                ctrl_key: false,
                alt_key: false,
                meta_key: false,
                repeat: false,
                code: 0,
                text: "",
            },
        ))
        .expect("Escape keydown on TextField must succeed");
    assert_eq!(consumed, 1, "prevent_default() on Escape must consume key");
    assert_eq!(take_log(), vec!["palette escape".to_string()]);

    // A subsequent pointer move must not panic on the unmounted palette.
    host.dispatch(HostEvent::new(
        pointer_node,
        pointer_handler,
        pointer_payload(PointerKind::Move, 10.0, 10.0),
    ))
    .expect("pointer move after Escape close must succeed");
}
