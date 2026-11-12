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
    assert!(
        !modifiers_of(&first, parent).iter().any(|modifier| matches!(
            modifier,
            Modifier::RequiredSize { width, height }
                if *width == WIDGET_WIDTH && *height == WIDGET_HEIGHT
        ))
    );

    let second = decode_batch(host.render_frame(0).unwrap()).unwrap();
    assert!(
        modifiers_of(&second, parent)
            .iter()
            .any(|modifier| matches!(
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
    assert_eq!(
        consumed, 0,
        "a consumed click would never reach the wrapper"
    );
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

mod editor_widgets {
    use dioxus_compose::prelude::*;
    use dioxus_compose::schema::RevealAlign;

    /// A code editor in a widget tree, with a button that asks its handle for an edit, a selection
    /// and a reveal.
    pub fn editor() -> Element {
        let handle = use_code_editor();
        rsx! {
            Column {
                CodeEditor { text: "let a = 1;\n", handle }
                Button {
                    text: "go",
                    on_click: move |_| {
                        handle.edit(7, 0, CodeRange::of(0, 4, 0, 5), "b");
                        handle.select(0, Position::new(0, 4), Position::new(0, 5));
                        handle.reveal(0, CodeRange::of(0, 4, 0, 5), RevealAlign::Center);
                    },
                }
            }
        }
    }
}

mod editor_page {
    use super::editor_widgets;
    use dioxus_compose::html::prelude::*;

    pub fn app() -> Element {
        rsx! {
            div { style: "width: 400px",
                WidgetView { app: editor_widgets::editor, style: "display: block" }
            }
        }
    }
}

/// What a code editor handle in a widget tree asks goes to the editor that carries it. The page's
/// own flush does not know a node of the tree, so without the tree's own flush the request was
/// dropped and a find bar's selection, reveal and replace never reached the editor.
#[test]
fn fr38_a_code_editor_handle_in_a_widget_tree_reaches_its_editor() {
    install();
    reset_window_size();
    let mut host = Host::html(editor_page::app, HtmlConfig::default);
    let (button, handler, editor) = {
        let first = decode_batch(host.rebuild().unwrap()).unwrap();
        let (button, handler) = widget_click(&first).expect("the widget's button");
        let editor = first
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::Create {
                    node_id,
                    widget: dioxus_compose::schema::WidgetKind::CodeEditor,
                } => Some(*node_id),
                _ => None,
            })
            .expect("the editor was created");
        (button, handler, editor)
    };
    let (batch, _) = host
        .dispatch(HostEvent {
            node_id: button,
            handler_id: handler,
            payload: EventPayload::Clicked,
        })
        .unwrap();
    let after = decode_batch(batch).unwrap();
    assert!(
        after.iter().any(|mutation| matches!(
            mutation,
            Mutation::EditCode { node_id, request_id: 7, base_version: 0, .. } if *node_id == editor
        )),
        "the edit did not reach the editor: {after:#?}"
    );
    assert!(
        after.iter().any(|mutation| matches!(
            mutation,
            Mutation::SetSelection { node_id, base_version: 0, anchor_line: 0, anchor_column: 4, active_line: 0, active_column: 5 } if *node_id == editor
        )),
        "the selection did not reach the editor: {after:#?}"
    );
    assert!(
        after.iter().any(|mutation| matches!(
            mutation,
            Mutation::RevealRange { node_id, .. } if *node_id == editor
        )),
        "the reveal did not reach the editor: {after:#?}"
    );
}

mod line_widgets {
    use dioxus_compose::prelude::*;

    pub fn name() -> Element {
        rsx! {
            TextField { placeholder: "name" }
        }
    }

    pub fn day() -> Element {
        rsx! {
            DatePicker { value: 20_000 }
        }
    }
}

mod line_page {
    use super::line_widgets;
    use dioxus_compose::html::prelude::*;

    pub fn app() -> Element {
        rsx! {
            div { style: "width: 600px",
                "ab"
                WidgetView { app: line_widgets::name, style: "display: inline-block" }
                "cd"
                WidgetView { app: line_widgets::day, style: "display: inline-block" }
                "ef"
            }
        }
    }
}

/// The last offset and size a node was given, from its modifiers in order.
fn placement(modifiers: impl IntoIterator<Item = Modifier>) -> (f32, f32, f32, f32) {
    let (mut x, mut y, mut width, mut height) = (f32::NAN, f32::NAN, f32::NAN, f32::NAN);
    for modifier in modifiers {
        {
            match &modifier {
                Modifier::Offset { x: ox, y: oy } => (x, y) = (*ox, *oy),
                Modifier::RequiredSize {
                    width: w,
                    height: h,
                } => (width, height) = (*w, *h),
                Modifier::Width(w) => width = *w,
                _ => {}
            }
        }
    }
    (x, y, width, height)
}

fn text_node(mutations: &[Mutation<'_>], text: &str) -> u32 {
    mutations
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::Text,
                value: PropertyValue::String(value),
            } if *value == text => Some(*node_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text node holds {text:?}"))
}

fn parent_of(mutations: &[Mutation<'_>], node: u32) -> u32 {
    mutations
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Insert {
                parent_id, node_id, ..
            } if *node_id == node => Some(*parent_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("node {node} was never inserted"))
}

fn root_of(mutations: &[Mutation<'_>], widget: dioxus_compose::schema::WidgetKind) -> u32 {
    mutations
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Create {
                node_id,
                widget: kind,
            } if *kind == widget => Some(*node_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {widget:?} was created"))
}

#[test]
fn fr34_3_textfield_and_datepicker_sit_in_the_line_at_their_measured_size() {
    use dioxus_compose::schema::WidgetKind;
    install();
    reset_window_size();
    let mut host = Host::html(line_page::app, HtmlConfig::default);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let field_box = parent_of(&first, root_of(&first, WidgetKind::TextField));
    let picker_box = parent_of(&first, root_of(&first, WidgetKind::DatePicker));
    let [ab, cd, ef] = ["ab", "cd", "ef"].map(|text| text_node(&first, text));
    let line = parent_of(&first, ab);
    for node in [cd, ef, field_box, picker_box] {
        assert_eq!(
            parent_of(&first, node),
            line,
            "node {node} is not in the line's box"
        );
    }
    // Copied out of the first batch, which cannot outlive the next frame's borrow.
    let nodes = [ab, cd, ef, field_box, picker_box, line];
    let before: Vec<Vec<Modifier>> = nodes.iter().map(|n| modifiers_of(&first, *n)).collect();
    let second = decode_batch(host.render_frame(0).unwrap()).unwrap();
    let at = |node: u32| {
        let index = nodes.iter().position(|n| *n == node).unwrap();
        placement(
            before[index]
                .iter()
                .cloned()
                .chain(modifiers_of(&second, node)),
        )
    };

    // Each widget's box is the size the renderer measured it at.
    for node in [field_box, picker_box] {
        let (_, _, width, height) = at(node);
        assert_eq!((width, height), (WIDGET_WIDTH, WIDGET_HEIGHT), "box {node}");
    }
    // Text, field, text, picker, text, left to right, each one starting where the one
    // before it ends: the text after a widget moved over by the widget's measured width.
    let (ab_x, ab_y, _, _) = at(ab);
    let (field_x, field_y, _, _) = at(field_box);
    let (cd_x, cd_y, cd_width, _) = at(cd);
    let (picker_x, picker_y, _, _) = at(picker_box);
    let (ef_x, ef_y, _, _) = at(ef);
    assert_eq!(ab_x, 0.0);
    assert!(field_x > ab_x, "the field is not after the first text");
    assert!(
        (cd_x - (field_x + WIDGET_WIDTH)).abs() < 0.01,
        "cd at {cd_x}, field ends at {}",
        field_x + WIDGET_WIDTH
    );
    assert!(
        (picker_x - (cd_x + cd_width)).abs() < 0.01,
        "the picker at {picker_x}, cd ends at {}",
        cd_x + cd_width
    );
    assert!(
        (ef_x - (picker_x + WIDGET_WIDTH)).abs() < 0.01,
        "ef at {ef_x}, picker ends at {}",
        picker_x + WIDGET_WIDTH
    );
    // One line: the texts share a baseline, the widgets share a top, and the line is as
    // tall as the widgets, so each text sits within the widgets' height.
    assert_eq!(ab_y, cd_y);
    assert_eq!(cd_y, ef_y);
    assert_eq!(field_y, picker_y);
    assert!(
        ab_y > field_y && ab_y < field_y + WIDGET_HEIGHT,
        "text at {ab_y}, widgets at {field_y}"
    );
    let (_, _, _, line_height) = at(line);
    assert!(
        line_height >= WIDGET_HEIGHT,
        "the line is {line_height} tall"
    );
}
