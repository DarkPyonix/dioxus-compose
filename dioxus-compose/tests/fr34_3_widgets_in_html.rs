//! Compose widgets inside an HTML page: `WidgetView`, sized by the renderer's measure call.

use std::cell::RefCell;
use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::measure::{MEASURE_RECORD_LEN, MeasureResult, RESULT_OK};
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, Modifier, PropertyKind};
use dioxus_compose::window::reset_window_size;

/// The size the test renderer reports for every node it is asked about.
const WIDGET_WIDTH: f32 = 120.0;
const WIDGET_HEIGHT: f32 = 40.0;

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

/// A renderer that knows every node and says each one is 120 by 40.
unsafe extern "C" fn measure_every_node(
    _requests: *const u8,
    length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    assert!(length as usize >= count as usize * MEASURE_RECORD_LEN);
    for index in 0..count as usize {
        // SAFETY: the caller hands room for `count` results.
        unsafe {
            results.add(index).write(MeasureResult {
                width: WIDGET_WIDTH,
                height: WIDGET_HEIGHT,
                status: RESULT_OK,
                ..MeasureResult::default()
            });
        }
    }
    0
}

fn install() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: measure_every_node,
    });
}

thread_local! {
    static LOG: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: &'static str) {
    LOG.with(|log| log.borrow_mut().push(entry));
}

mod widgets {
    use super::log;
    use dioxus_compose::prelude::*;

    pub fn field() -> Element {
        rsx! {
            Button { text: "press", on_click: move |_| log("widget") }
        }
    }
}

mod page {
    use super::{log, widgets};
    use dioxus_compose::html::prelude::*;

    pub fn app() -> Element {
        rsx! {
            div { style: "width: 400px",
                div { style: "height: 30px" }
                div { id: "wrap", onclick: move |_| log("wrapper"),
                    WidgetView { app: widgets::field, style: "display: block" }
                }
            }
        }
    }
}

fn host() -> Host {
    install();
    reset_window_size();
    Host::html(page::app, HtmlConfig::default)
}

/// The renderer node the page drew for the widget's place: the one the widget tree was
/// inserted under.
fn widget_parent(mutations: &[Mutation<'_>]) -> u32 {
    mutations
        .iter()
        .rev()
        .find_map(|mutation| match mutation {
            Mutation::Insert {
                parent_id, node_id, ..
            } if *node_id >= 0x4000_0000 && *node_id < 0x8000_0000 => Some(*parent_id),
            _ => None,
        })
        .expect("the widget tree was not inserted under a box of the page")
}

fn modifiers_of(mutations: &[Mutation<'_>], node: u32) -> Vec<Modifier> {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id, modifier, ..
            } if *node_id == node => Some(modifier.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn fr34_3_widget_is_inserted_under_the_box_the_page_drew() {
    let mut host = host();
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let parent = widget_parent(&first);
    assert!(
        first.iter().any(|mutation| matches!(
            mutation,
            Mutation::Create { node_id, .. } if *node_id == parent
        )),
        "the box the widget hangs from was not created by the page",
    );
}

#[test]
fn fr34_3_widget_box_takes_the_size_the_renderer_measured() {
    let mut host = host();
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let parent = widget_parent(&first);
    // The renderer did not know the widget when the first batch was computed, so the box
    // is still empty there.
    assert!(!modifiers_of(&first, parent).iter().any(|modifier| matches!(
        modifier,
        Modifier::RequiredSize { width, height }
            if *width == WIDGET_WIDTH && *height == WIDGET_HEIGHT
    )));

    let second = decode_batch(host.render_frame(0).unwrap()).unwrap();
    assert!(
        modifiers_of(&second, parent).iter().any(|modifier| matches!(
            modifier,
            Modifier::RequiredSize { width, height }
                if *width == WIDGET_WIDTH && *height == WIDGET_HEIGHT
        )),
        "the next frame did not lay the box out at the measured size: {:?}",
        modifiers_of(&second, parent),
    );
}

#[test]
fn fr34_3_widget_is_placed_at_the_offset_the_page_computed() {
    let mut host = host();
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let parent = widget_parent(&first);
    // A box is placed inside its container, so the widget's box sits at the top of the
    // wrapping div, and the wrapping div is the one 30px below the block before it.
    let container = first
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Insert {
                parent_id, node_id, ..
            } if *node_id == parent => Some(*parent_id),
            _ => None,
        })
        .expect("the widget's box is not inside a container");
    // Copied out of the first batch, because rendering the second frame borrows the host
    // mutably and the batch cannot outlive that.
    let first_container = modifiers_of(&first, container);
    let first_parent = modifiers_of(&first, parent);
    let second = decode_batch(host.render_frame(0).unwrap()).unwrap();
    let offsets = |before: &[Modifier], node| {
        before
            .iter()
            .cloned()
            .chain(modifiers_of(&second, node))
            .filter_map(|modifier| match modifier {
                Modifier::Offset { y, .. } => Some(y),
                _ => None,
            })
            .collect::<Vec<f32>>()
    };
    assert!(
        offsets(&first_container, container).contains(&30.0),
        "the wrapping div is not below the 30px block before it: {:?}",
        offsets(&first_container, container),
    );
    assert!(
        offsets(&first_parent, parent).iter().all(|y| *y == 0.0),
        "the widget's box is not at the top of the wrapping div: {:?}",
        offsets(&first_parent, parent),
    );
}

/// The click handler of the widget's button, which a widget tree sends as the `OnClick`
/// property with a handler id from its own range (they start at `1 << 48`).
fn widget_click(mutations: &[Mutation<'_>]) -> Option<(u32, u64)> {
    mutations.iter().find_map(|mutation| match mutation {
        Mutation::SetProp {
            node_id,
            property: PropertyKind::OnClick,
            value: PropertyValue::Integer(handler_id),
        } if *handler_id >= 1 << 48 => Some((*node_id, *handler_id as u64)),
        _ => None,
    })
}

/// The click handler of a box the page drew, which the HTML path sends as a `Clickable`
/// modifier with a small handler id.
fn page_click(mutations: &[Mutation<'_>]) -> Option<(u32, u64)> {
    mutations.iter().find_map(|mutation| match mutation {
        Mutation::SetModifier {
            node_id,
            modifier: Modifier::Clickable { handler_id },
            ..
        } if *handler_id < 1 << 40 => Some((*node_id, *handler_id)),
        _ => None,
    })
}

#[test]
fn fr34_3_click_on_a_widget_runs_its_handler_then_the_wrapper_once() {
    let mut host = host();
    LOG.with(|log| log.borrow_mut().clear());
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let (button, handler) = widget_click(&first).expect("the widget's button has no click handler");
    let (wrapper, wrapper_handler) = page_click(&first).expect("the wrapping div is not clickable");

    // The renderer gives a press to the widget first. The widget does not consume it, so
    // the renderer offers it to the clickable box around it.
    let (_, consumed) = host
        .dispatch(HostEvent {
            node_id: button,
            handler_id: handler,
            payload: EventPayload::Clicked,
        })
        .unwrap();
    assert_eq!(consumed, 0, "a consumed click would never reach the wrapper");
    assert_eq!(
        LOG.with(|log| log.borrow().clone()),
        vec!["widget"],
        "the Host must not run the wrapper itself: the renderer delivers that click",
    );

    host.dispatch(HostEvent {
        node_id: wrapper,
        handler_id: wrapper_handler,
        payload: EventPayload::Clicked,
    })
    .unwrap();
    assert_eq!(
        LOG.with(|log| log.borrow().clone()),
        vec!["widget", "wrapper"],
        "the widget's handler runs first, then the wrapping div's, once each",
    );
}

mod island_page {
    use super::widgets;
    use dioxus_compose::html::prelude::*;

    pub fn app() -> Element {
        rsx! {
            div { style: "width: 300px",
                WidgetView { app: widgets::field, style: "display: block" }
            }
        }
    }
}

fn screen() -> dioxus_compose::Element {
    use dioxus_compose::prelude::*;
    rsx! {
        Column {
            HtmlView { app: island_page::app, height: 200.0 }
        }
    }
}

#[test]
fn fr34_3_widget_inside_an_htmlview_island_hangs_from_the_islands_box() {
    install();
    reset_window_size();
    let mut host = Host::new(screen);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let parent = widget_parent(&first);
    assert!(
        parent >= 0x8000_0000,
        "the widget was inserted under node {parent}, which is not one of the island's boxes",
    );
    assert!(
        first.iter().any(|mutation| matches!(
            mutation,
            Mutation::Create { node_id, .. } if *node_id == parent
        )),
        "the island's box was not created by the island",
    );
}
