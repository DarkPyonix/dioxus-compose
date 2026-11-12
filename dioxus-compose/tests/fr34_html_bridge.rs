//! A screen written with HTML elements and CSS, drawn through compose-rust.
//!
//! Each test runs an app through a Host the way a window runs it, and reads the records
//! the renderer would receive: the first batch draws the whole page, later batches carry
//! only what changed, and events the renderer sends reach the DOM node's handler.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px. The
//! renderer has not reported a window size, so pages are laid out at 800 by 600.

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::html::prelude::dioxus_elements::point_interaction::ModifiersInteraction;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    HtmlConfig, TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint,
};
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    Color, EventPayload, Modifier, Paint, PropertyKind, TextOverflow, WidgetKind,
};
use dioxus_compose::spans::{TextSpan, TextSpans};
use dioxus_hooks::use_signal;

struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let width = match request.width {
            WidthConstraint::AtMost(width) => {
                (request.text.chars().count() as f32 * 10.0).min(width.max(0.0))
            }
            _ => request.text.chars().count() as f32 * 10.0,
        };
        TextMetrics {
            width,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

/// Names `cat.png` as asset 7, 40 by 30.
struct Cat;

impl ImageResolver for Cat {
    fn resolve(&mut self, url: &str) -> Option<AssetId> {
        (url == "cat.png").then_some(AssetId(7))
    }

    fn size(&mut self, url: &str) -> Option<ImageSize> {
        (url == "cat.png").then_some(ImageSize::new(40, 30))
    }
}

fn config_with_cat() -> HtmlConfig {
    HtmlConfig {
        images: Some(Box::new(Cat)),
        ..config()
    }
}

/// A property value, owned, so records outlive the batch they came in.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    None,
    Str(String),
    Bool(bool),
    Int(i64),
    Float(f32),
    Bytes(Vec<u8>),
}

/// One record of a batch, owned.
#[derive(Clone, Debug, PartialEq)]
enum Record {
    Create(u32, WidgetKind),
    Prop(u32, PropertyKind, Value),
    Modifier(u32, u16, Modifier),
    Insert {
        parent: u32,
        node: u32,
        index: u32,
    },
    Move {
        parent: u32,
        node: u32,
        index: u32,
    },
    Remove(u32),
    SetText(u32, String),
    /// A record that is not about the tree: the theme, the window, an asset.
    Other,
}

fn records(bytes: &[u8]) -> Vec<Record> {
    decode_batch(bytes)
        .expect("the batch decodes")
        .into_iter()
        .map(|record| match record {
            Mutation::Create { node_id, widget } => Record::Create(node_id, widget),
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => Record::Prop(
                node_id,
                property,
                match value {
                    PropertyValue::None => Value::None,
                    PropertyValue::String(text) => Value::Str(text.to_string()),
                    PropertyValue::Bool(flag) => Value::Bool(flag),
                    PropertyValue::Integer(number) => Value::Int(number),
                    PropertyValue::Float(number) => Value::Float(number),
                    PropertyValue::Bytes(bytes) => Value::Bytes(bytes.to_vec()),
                },
            ),
            Mutation::SetModifier {
                node_id,
                index,
                modifier,
            } => Record::Modifier(node_id, index, modifier),
            Mutation::Insert {
                parent_id,
                node_id,
                index,
            } => Record::Insert {
                parent: parent_id,
                node: node_id,
                index,
            },
            Mutation::Move {
                parent_id,
                node_id,
                index,
            } => Record::Move {
                parent: parent_id,
                node: node_id,
                index,
            },
            Mutation::Remove { node_id } => Record::Remove(node_id),
            Mutation::SetText { node_id, text, .. } => Record::SetText(node_id, text.to_string()),
            _ => Record::Other,
        })
        .collect()
}

/// The records about the tree, without the theme and window every rebuild starts with.
fn tree(records: Vec<Record>) -> Vec<Record> {
    records
        .into_iter()
        .filter(|record| *record != Record::Other)
        .collect()
}

fn rgb(r: u8, g: u8, b: u8) -> Paint {
    Paint::Literal(Color::argb(
        0xff00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
    ))
}

/// The modifier list each node ends up with after `records`, by index.
fn modifiers_of(records: &[Record], node: u32) -> Vec<Modifier> {
    let mut list: Vec<Modifier> = Vec::new();
    for record in records {
        if let Record::Modifier(id, index, modifier) = record
            && *id == node
        {
            let index = usize::from(*index);
            if list.len() <= index {
                list.resize(index + 1, Modifier::Empty);
            }
            list[index] = modifier.clone();
        }
    }
    list.retain(|modifier| *modifier != Modifier::Empty);
    list
}

/// The one node given `modifier`.
#[track_caller]
fn node_with(records: &[Record], modifier: &Modifier) -> u32 {
    let nodes: Vec<u32> = records
        .iter()
        .filter_map(|record| match record {
            Record::Modifier(node, _, given) if given == modifier => Some(*node),
            _ => None,
        })
        .collect();
    assert_eq!(nodes.len(), 1, "one node has {modifier:?}: {records:#?}");
    nodes[0]
}

/// The handler a node was given for `property`.
#[track_caller]
fn handler_of(records: &[Record], node: u32, property: PropertyKind) -> u64 {
    records
        .iter()
        .find_map(|record| match record {
            Record::Prop(id, given, Value::Int(handler)) if *id == node && *given == property => {
                Some(*handler as u64)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("node {node} has a {property:?} handler: {records:#?}"))
}

/// The nodes created as `widget`.
fn created(records: &[Record], widget: WidgetKind) -> Vec<u32> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Create(node, kind) if *kind == widget => Some(*node),
            _ => None,
        })
        .collect()
}

/// The node with a `Clickable` modifier, and its handler. There is exactly one.
#[track_caller]
fn clickable(records: &[Record]) -> (u32, u64) {
    let found: Vec<(u32, u64)> = records
        .iter()
        .filter_map(|record| match record {
            Record::Modifier(node, _, Modifier::Clickable { handler_id }) => {
                Some((*node, *handler_id))
            }
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one clickable node: {records:#?}");
    found[0]
}

fn click(host: &mut Host, (node_id, handler_id): (u32, u64)) -> Vec<Record> {
    let (batch, _) = host
        .dispatch(HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::Clicked,
        })
        .expect("the click is dispatched");
    tree(records(batch))
}

fn texts(records: &[Record]) -> Vec<String> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Prop(_, PropertyKind::Text, Value::Str(text)) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn every_kind() -> Element {
    rsx! {
        div { style: "width: 200px; height: 40px",
            div { style: "width: 10px; height: 10px; background-color: rgb(0, 0, 255)" }
        }
        p { "words" }
        img { src: "cat.png" }
        div { style: "width: 60px; height: 60px; overflow: auto",
            div { style: "width: 200px; height: 200px; background-color: rgb(255, 0, 0)" }
        }
        input { value: "Ada", style: "width: 120px; height: 24px" }
        input { r#type: "checkbox" }
        input { r#type: "radio" }
        select {
            option { "one" }
            option { "two" }
        }
    }
}

/// The first frame creates every plan kind as the widget it is drawn with: boxes holding
/// others as `AbsoluteBox`, an empty one as `Box`, a run as `Text`, an image as an `Image`
/// with its asset, both scroll axes, and the four form controls. Every node but the
/// window's own scroll is inserted somewhere, so the page is one tree.
#[test]
fn fr42_first_frame_creates_the_widget_of_every_plan_kind() {
    let mut host = Host::html(every_kind, config_with_cat);
    let first = tree(records(host.rebuild().unwrap()));

    for widget in [
        WidgetKind::AbsoluteBox,
        WidgetKind::Box,
        WidgetKind::Text,
        WidgetKind::Image,
        WidgetKind::ScrollColumn,
        WidgetKind::ScrollRow,
        WidgetKind::TextField,
        WidgetKind::Checkbox,
        WidgetKind::RadioButton,
        WidgetKind::Dropdown,
    ] {
        assert!(
            !created(&first, widget).is_empty(),
            "a {widget:?} is created: {first:#?}"
        );
    }

    let image = created(&first, WidgetKind::Image);
    assert_eq!(image.len(), 1);
    assert!(
        first.contains(&Record::Prop(image[0], PropertyKind::Asset, Value::Int(7))),
        "the image draws asset 7"
    );
    assert_eq!(
        modifiers_of(&first, image[0]),
        vec![
            Modifier::Offset { x: 0.0, y: 0.0 },
            Modifier::RequiredSize {
                width: 40.0,
                height: 30.0
            },
        ]
    );

    let dropdown = created(&first, WidgetKind::Dropdown)[0];
    let options: Vec<u32> = first
        .iter()
        .filter_map(|record| match record {
            Record::Insert { parent, node, .. } if *parent == dropdown => Some(*node),
            _ => None,
        })
        .collect();
    assert_eq!(options.len(), 2, "the select's two options: {first:#?}");
    let labels = texts(&first);
    assert!(labels.contains(&"one".to_string()) && labels.contains(&"two".to_string()));

    let field = created(&first, WidgetKind::TextField)[0];
    assert!(first.contains(&Record::Prop(
        field,
        PropertyKind::Text,
        Value::Str("Ada".to_string())
    )));

    // The window's own box, which names the factor the page is drawn at, is the one node
    // nothing inserts. The page's scroll hangs from it.
    let window = match first.iter().rev().find_map(|record| match record {
        Record::Create(node, WidgetKind::AbsoluteBox) => Some(*node),
        _ => None,
    }) {
        Some(node) => node,
        None => panic!("the window's box is created: {first:#?}"),
    };
    assert!(
        first.iter().any(|record| matches!(
            record,
            Record::Insert { parent, node, .. }
                if *parent == window && created(&first, WidgetKind::ScrollColumn).contains(node)
        )),
        "the window's scroll hangs from the window's box: {first:#?}"
    );
    for record in &first {
        if let Record::Create(node, _) = record
            && *node != window
        {
            assert!(
                first
                    .iter()
                    .any(|record| matches!(record, Record::Insert { node: inserted, .. } if inserted == node)),
                "node {node} is inserted: {first:#?}"
            );
        }
    }
}

/// A frame in which nothing changed sends nothing.
#[test]
fn fr34_unchanged_frame_sends_nothing() {
    let mut host = Host::html(every_kind, config_with_cat);
    host.rebuild().unwrap();
    let next = tree(records(host.render_frame(0).unwrap()));
    assert_eq!(next, Vec::new());
}

fn boxed_inlines() -> Element {
    rsx! {
        img { src: "cat.png" }
        input { r#type: "checkbox" }
    }
}

/// An anonymous block that wraps only nodes with boxes of their own (an image and a form
/// control) is listed under the same key in every layout, so a second frame resends nothing.
#[test]
fn fr34_anonymous_block_of_boxed_inlines_keeps_its_key() {
    let mut host = Host::html(boxed_inlines, config_with_cat);
    host.rebuild().unwrap();
    let next = tree(records(host.render_frame(0).unwrap()));
    assert_eq!(next, Vec::new());
}

fn bars() -> Element {
    let mut presses = use_signal(|| 0u32);
    let colour = if presses() > 0 {
        "rgb(255, 0, 0)"
    } else {
        "rgb(0, 0, 255)"
    };
    rsx! {
        div { style: "display: flex; flex-direction: column; width: 100px",
            div {
                style: "height: 20px; background-color: rgb(0, 128, 0)",
                onclick: move |_| presses += 1,
            }
            div { style: "height: 20px; background-color: {colour}" }
            div { style: "height: 20px; background-color: rgb(0, 128, 0)" }
        }
    }
}

/// One box's background changes, and the batch that follows is that one modifier on that
/// one node: nothing else is resent.
#[test]
fn fr34_background_change_sends_only_that_box() {
    let mut host = Host::html(bars, config);
    let first = tree(records(host.rebuild().unwrap()));
    let target = node_with(&first, &Modifier::Background(rgb(0, 0, 255)));
    let index = first
        .iter()
        .find_map(|record| match record {
            Record::Modifier(node, index, Modifier::Background(_)) if *node == target => {
                Some(*index)
            }
            _ => None,
        })
        .unwrap();

    let after = click(&mut host, clickable(&first));
    assert_eq!(
        after,
        vec![Record::Modifier(
            target,
            index,
            Modifier::Background(rgb(255, 0, 0))
        )]
    );
}

fn counter() -> Element {
    let mut presses = use_signal(|| 0u32);
    let count = presses();
    rsx! {
        div {
            style: "width: 100px; height: 20px",
            onclick: move |_| presses += 1,
            "pressed {count}"
        }
    }
}

/// The renderer presses the box it drew for a node with a click handler, the event goes
/// back through the Host to that node's Dioxus handler, and the next batch draws what the
/// handler changed. An event naming the handler from another node is refused.
#[test]
fn fr34_click_reaches_the_dom_handler_through_the_renderer() {
    let mut host = Host::html(counter, config);
    let first = tree(records(host.rebuild().unwrap()));
    assert!(texts(&first).contains(&"pressed 0".to_string()));
    let (node, handler) = clickable(&first);
    assert!(
        created(&first, WidgetKind::AbsoluteBox).contains(&node),
        "the clickable node is the box: {first:#?}"
    );

    assert!(
        host.dispatch(HostEvent {
            node_id: node + 1000,
            handler_id: handler,
            payload: EventPayload::Clicked,
        })
        .is_err(),
        "a handler is only reached from the node it was set on"
    );

    let after = click(&mut host, (node, handler));
    assert_eq!(texts(&after), vec!["pressed 1".to_string()]);
    let after = click(&mut host, (node, handler));
    assert_eq!(texts(&after), vec!["pressed 2".to_string()]);
}

fn page_with_script() -> Element {
    rsx! {
        p { "visible" }
        script { "globalThis.ran = true; while (true) ;" }
    }
}

/// A `<script>` reaches the renderer as nothing: no text of its source is drawn, and the
/// page around it is.
#[test]
fn fr34_script_is_not_run() {
    let mut host = Host::html(page_with_script, config);
    let first = tree(records(host.rebuild().unwrap()));
    let drawn = texts(&first);
    assert!(drawn.contains(&"visible".to_string()), "{drawn:?}");
    assert!(
        drawn.iter().all(|text| !text.contains("globalThis")),
        "the script's source is not drawn: {drawn:?}"
    );
}

fn rounded() -> Element {
    rsx! {
        div { style: "direction: rtl; position: relative; width: 300px; height: 200px",
            div { style: "position: absolute; left: 10px; top: 5px; width: 100px; height: 100px; border-radius: 4px 8px 12px 16px; background-color: rgb(255, 0, 0)" }
            div { style: "position: absolute; left: 150px; top: 5px; width: 100px; height: 100px; border-radius: 6px; background-color: rgb(0, 0, 255)" }
        }
    }
}

/// Corners go one radius per corner, in physical order (top left, top right, bottom
/// right, bottom left), on a right-to-left page as on any other, and four equal corners
/// still go as `CornerEach`, never as the design system's `Shape`. Offsets are physical
/// too: `left: 10px` is ten from the left.
#[test]
fn fr42_corners_are_sent_per_corner_and_physical() {
    let mut host = Host::html(rounded, config);
    let first = tree(records(host.rebuild().unwrap()));

    let mixed = node_with(&first, &Modifier::Background(rgb(255, 0, 0)));
    let mixed_modifiers = modifiers_of(&first, mixed);
    assert!(
        mixed_modifiers.contains(&Modifier::CornerEach {
            top_left: 4.0,
            top_right: 8.0,
            bottom_right: 12.0,
            bottom_left: 16.0,
        }),
        "{mixed_modifiers:?}"
    );
    assert_eq!(mixed_modifiers[0], Modifier::Offset { x: 10.0, y: 5.0 });

    let even = node_with(&first, &Modifier::Background(rgb(0, 0, 255)));
    let even_modifiers = modifiers_of(&first, even);
    assert!(
        even_modifiers.contains(&Modifier::CornerEach {
            top_left: 6.0,
            top_right: 6.0,
            bottom_right: 6.0,
            bottom_left: 6.0,
        }),
        "{even_modifiers:?}"
    );
    assert_eq!(even_modifiers[0], Modifier::Offset { x: 150.0, y: 5.0 });

    assert!(
        first
            .iter()
            .all(|record| !matches!(record, Record::Modifier(_, _, Modifier::Shape { .. }))),
        "no corner goes as Shape: {first:#?}"
    );
}

fn shadowed() -> Element {
    rsx! {
        div { style: "width: 50px; height: 50px; background-color: rgb(255, 255, 255); box-shadow: 1px 2px 3px 0px rgb(255, 0, 0), 4px 5px 6px 0px rgb(0, 0, 255)" }
    }
}

/// CSS draws the first shadow of a list on top, and a modifier list draws its later
/// entries over its earlier ones, so the list goes last to first: the blue shadow, then the
/// red one over it.
#[test]
fn fr42_shadows_go_last_to_first() {
    let mut host = Host::html(shadowed, config);
    let first = tree(records(host.rebuild().unwrap()));
    let card = node_with(&first, &Modifier::Background(rgb(255, 255, 255)));
    let shadows: Vec<Modifier> = modifiers_of(&first, card)
        .into_iter()
        .filter(|modifier| matches!(modifier, Modifier::Shadow { .. }))
        .collect();
    assert_eq!(
        shadows,
        vec![
            Modifier::Shadow {
                x: 4.0,
                y: 5.0,
                blur: 6.0,
                spread: 0.0,
                paint: rgb(0, 0, 255),
            },
            Modifier::Shadow {
                x: 1.0,
                y: 2.0,
                blur: 3.0,
                spread: 0.0,
                paint: rgb(255, 0, 0),
            },
        ]
    );
    // Drawn under the background, so both come before it.
    let list = modifiers_of(&first, card);
    let background = list
        .iter()
        .position(|modifier| matches!(modifier, Modifier::Background(_)))
        .unwrap();
    assert!(
        list[..background]
            .iter()
            .filter(|modifier| matches!(modifier, Modifier::Shadow { .. }))
            .count()
            == 2
    );
}

fn overflowing() -> Element {
    rsx! {
        div { style: "width: 50px; height: 50px; overflow: hidden; background-color: rgb(0, 0, 255)",
            div { style: "width: 100px; height: 100px; background-color: rgb(255, 0, 0)" }
        }
        div { style: "width: 50px; height: 50px; border-radius: 8px; background-color: rgb(0, 128, 0)",
            div { style: "width: 100px; height: 100px; background-color: rgb(255, 255, 0)" }
        }
    }
}

/// `overflow: hidden` clips the box's content, and is sent as `Clip`. `overflow: visible`
/// with rounded corners rounds the box and clips nothing, as CSS does, so that box has its
/// corners and no `Clip`.
#[test]
fn fr42_overflow_hidden_sends_clip_visible_does_not() {
    let mut host = Host::html(overflowing, config);
    let first = tree(records(host.rebuild().unwrap()));

    let hidden = node_with(&first, &Modifier::Background(rgb(0, 0, 255)));
    assert!(
        modifiers_of(&first, hidden).contains(&Modifier::Clip(true)),
        "{:?}",
        modifiers_of(&first, hidden)
    );

    let visible = node_with(&first, &Modifier::Background(rgb(0, 128, 0)));
    let modifiers = modifiers_of(&first, visible);
    assert!(
        modifiers
            .iter()
            .any(|modifier| matches!(modifier, Modifier::CornerEach { .. })),
        "{modifiers:?}"
    );
    assert!(
        modifiers
            .iter()
            .all(|modifier| !matches!(modifier, Modifier::Clip(_))),
        "{modifiers:?}"
    );
}

fn rounded_scroller() -> Element {
    rsx! {
        div { style: "width: 120px; overflow-x: auto; border-radius: 8px; background-color: rgb(0, 0, 255)",
            div { style: "width: 300px; height: 40px; background-color: rgb(255, 0, 0)" }
        }
    }
}

/// A scroll container with rounded corners clips its content to them, as `overflow: hidden`
/// does: its scroll viewport is sent with the corners and then `Clip`, so the content's
/// corners are cut where the box's are.
#[test]
fn fr34_scroll_container_clips_to_its_rounded_corners() {
    let mut host = Host::html(rounded_scroller, config);
    let first = tree(records(host.rebuild().unwrap()));

    let rows = created(&first, WidgetKind::ScrollRow);
    assert_eq!(rows.len(), 1, "{first:#?}");
    let modifiers = modifiers_of(&first, rows[0]);
    let corners = modifiers
        .iter()
        .position(|modifier| {
            *modifier
                == Modifier::CornerEach {
                    top_left: 8.0,
                    top_right: 8.0,
                    bottom_right: 8.0,
                    bottom_left: 8.0,
                }
        })
        .unwrap_or_else(|| panic!("the scroll viewport has 8px corners: {modifiers:?}"));
    let clip = modifiers
        .iter()
        .position(|modifier| *modifier == Modifier::Clip(true))
        .unwrap_or_else(|| panic!("the scroll viewport clips: {modifiers:?}"));
    assert!(corners < clip, "{modifiers:?}");
}

fn reordering() -> Element {
    let mut presses = use_signal(|| 0u32);
    let items: Vec<u32> = if presses() > 0 {
        vec![3, 1, 2]
    } else {
        vec![1, 2, 3]
    };
    rsx! {
        div {
            style: "width: 20px; height: 20px; background-color: rgb(0, 0, 0)",
            onclick: move |_| presses += 1,
        }
        div { style: "width: 100px",
            for item in items {
                div {
                    key: "{item}",
                    style: "height: 10px; background-color: rgb({item}, 0, 0)",
                }
            }
        }
    }
}

/// Keyed boxes that change places are moved, not made again: no node is created or
/// removed, and the box that went first is moved to the front of its parent.
#[test]
fn fr34_reordered_boxes_are_moved_not_recreated() {
    let mut host = Host::html(reordering, config);
    let first = tree(records(host.rebuild().unwrap()));
    let third = node_with(&first, &Modifier::Background(rgb(3, 0, 0)));

    let after = click(&mut host, clickable(&first));
    assert!(
        after
            .iter()
            .all(|record| !matches!(record, Record::Create(..) | Record::Remove(_))),
        "{after:#?}"
    );
    assert!(
        after
            .iter()
            .any(|record| matches!(record, Record::Move { node, index: 0, .. } if *node == third)),
        "the third box moves to the front: {after:#?}"
    );
}

thread_local! {
    static TYPED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn field() -> Element {
    rsx! {
        input {
            value: "Ada",
            style: "width: 120px; height: 24px",
            oninput: move |event| TYPED.with(|typed| typed.borrow_mut().push(event.value())),
        }
    }
}

/// Text the user types reaches the field's `oninput`, and is not sent back: the field owns
/// its text while it is being edited.
#[test]
fn fr34_typed_text_reaches_oninput_and_is_not_sent_back() {
    TYPED.with(|typed| typed.borrow_mut().clear());
    let mut host = Host::html(field, config);
    let first = tree(records(host.rebuild().unwrap()));
    let field = created(&first, WidgetKind::TextField)[0];
    let handler = handler_of(&first, field, PropertyKind::OnValueChange);

    let (batch, _) = host
        .dispatch(HostEvent {
            node_id: field,
            handler_id: handler,
            payload: EventPayload::TextChanged("Ada Lovelace"),
        })
        .unwrap();
    let after = tree(records(batch));
    assert_eq!(
        TYPED.with(|typed| typed.borrow().clone()),
        vec!["Ada Lovelace".to_string()]
    );
    assert!(
        after
            .iter()
            .all(|record| !matches!(record, Record::SetText(..))),
        "{after:#?}"
    );
}

thread_local! {
    static HEARD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn heard(line: impl Into<String>) {
    HEARD.with(|heard| heard.borrow_mut().push(line.into()));
}

fn take_heard() -> Vec<String> {
    HEARD.with(|heard| std::mem::take(&mut *heard.borrow_mut()))
}

/// Every node given a handler for `property`, with the handler.
fn handlers_for(records: &[Record], property: PropertyKind) -> Vec<(u32, u64)> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Prop(node, given, Value::Int(handler)) if *given == property => {
                Some((*node, *handler as u64))
            }
            _ => None,
        })
        .collect()
}

fn enter(host: &mut Host, (node_id, handler_id): (u32, u64), ctrl_key: bool) -> i64 {
    let (_, result) = host
        .dispatch(HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::KeyDown {
                key: dioxus_compose::schema::Key::Enter,
                shift_key: true,
                ctrl_key,
                alt_key: false,
                meta_key: false,
            },
        })
        .expect("the key is dispatched");
    result
}

fn keyed() -> Element {
    rsx! {
        div {
            style: "width: 200px; height: 40px",
            onkeydown: move |event| heard(format!("div keydown {}", event.key())),
            input {
                style: "width: 120px; height: 24px",
                onkeydown: move |event| {
                    let modifiers = event.modifiers();
                    heard(format!(
                        "input keydown {} shift={} ctrl={}",
                        event.key(),
                        modifiers.contains(dioxus_elements::Modifiers::SHIFT),
                        modifiers.contains(dioxus_elements::Modifiers::CONTROL),
                    ));
                    if modifiers.contains(dioxus_elements::Modifiers::CONTROL) {
                        event.prevent_default();
                    }
                },
                onkeypress: move |event| heard(format!("input keypress {}", event.key())),
            }
        }
    }
}

/// Enter pressed in a field reaches the field's `onkeydown` with its modifiers, bubbles to
/// the element around it, and is followed by `keypress`. The renderer is told the key was
/// not used, so the field still does what Enter does; when a handler prevents the default
/// it is told the key was used, and `keypress` does not follow. The box of an element that
/// listens for keys gets a handler of its own, for keys pressed in whatever inside it has
/// the focus, and the field's own box does not, so no key reaches the page twice.
#[test]
fn fr34_key_reaches_the_focused_dom_handler() {
    take_heard();
    let mut host = Host::html(keyed, config);
    let first = tree(records(host.rebuild().unwrap()));
    let field = created(&first, WidgetKind::TextField)[0];
    let field_keys = (field, handler_of(&first, field, PropertyKind::OnKeyDown));
    let box_keys: Vec<(u32, u64)> = handlers_for(&first, PropertyKind::OnKeyDown)
        .into_iter()
        .filter(|(node, _)| *node != field)
        .collect();
    assert_eq!(
        box_keys.len(),
        1,
        "only the div's box takes keys besides the field: {first:#?}"
    );
    assert!(created(&first, WidgetKind::AbsoluteBox).contains(&box_keys[0].0));

    assert_eq!(enter(&mut host, field_keys, false), 0);
    assert_eq!(
        take_heard(),
        vec![
            "input keydown Enter shift=true ctrl=false".to_string(),
            "div keydown Enter".to_string(),
            "input keypress Enter".to_string(),
        ]
    );

    assert_eq!(enter(&mut host, field_keys, true), 1);
    assert_eq!(
        take_heard(),
        vec![
            "input keydown Enter shift=true ctrl=true".to_string(),
            "div keydown Enter".to_string(),
        ]
    );

    // From the div's box the key has already been offered to every element around the
    // div, so the renderer is told it was used.
    assert_eq!(enter(&mut host, box_keys[0], false), 1);
    assert_eq!(take_heard(), vec!["div keydown Enter".to_string()]);

    assert!(
        host.dispatch(HostEvent {
            node_id: field + 1000,
            handler_id: field_keys.1,
            payload: EventPayload::KeyDown {
                key: dioxus_compose::schema::Key::Enter,
                shift_key: false,
                ctrl_key: false,
                alt_key: false,
                meta_key: false,
            },
        })
        .is_err(),
        "a key handler is only reached from the node it was set on"
    );
}

fn focusable() -> Element {
    rsx! {
        div {
            style: "width: 200px; height: 40px",
            onfocus: move |_| heard("div focus"),
            onblur: move |_| heard("div blur"),
            onfocusin: move |_| heard("div focusin"),
            onfocusout: move |_| heard("div focusout"),
            input {
                style: "width: 120px; height: 24px",
                onfocus: move |_| heard("input focus"),
                onblur: move |_| heard("input blur"),
                onchange: move |_| heard("input change"),
            }
        }
    }
}

fn type_into(host: &mut Host, (node_id, handler_id): (u32, u64), text: &str) {
    host.dispatch(HostEvent {
        node_id,
        handler_id,
        payload: EventPayload::TextChanged(text),
    })
    .expect("the text is dispatched");
}

/// A field's `focus` reaches its handler when the user first does something in it, with
/// `focusin` bubbling after it; leaving the field delivers `change`, then `blur`, then
/// `focusout` bubbling. `focus` and `blur` do not bubble, so the element around the field
/// hears only `focusin` and `focusout`.
#[test]
fn fr34_focus_and_blur_reach_the_dom_handler() {
    take_heard();
    let mut host = Host::html(focusable, config);
    let first = tree(records(host.rebuild().unwrap()));
    let field = created(&first, WidgetKind::TextField)[0];
    let typing = (
        field,
        handler_of(&first, field, PropertyKind::OnValueChange),
    );
    let leaving = (field, handler_of(&first, field, PropertyKind::OnFocusLost));

    type_into(&mut host, typing, "a");
    assert_eq!(
        take_heard(),
        vec!["input focus".to_string(), "div focusin".to_string()]
    );
    type_into(&mut host, typing, "ab");
    assert_eq!(
        take_heard(),
        Vec::<String>::new(),
        "focus is delivered once"
    );

    host.dispatch(HostEvent {
        node_id: leaving.0,
        handler_id: leaving.1,
        payload: EventPayload::FocusLost,
    })
    .expect("the focus loss is dispatched");
    assert_eq!(
        take_heard(),
        vec![
            "input change".to_string(),
            "input blur".to_string(),
            "div focusout".to_string(),
        ]
    );

    // Back in the field: it has the focus again.
    let keys = (field, handler_of(&first, field, PropertyKind::OnKeyDown));
    enter(&mut host, keys, false);
    assert_eq!(
        take_heard(),
        vec!["input focus".to_string(), "div focusin".to_string()]
    );
}

fn schemed_config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![
            "html, body { margin: 0; padding: 0; } \
             #panel { width: 100px; height: 20px; background-color: rgb(255, 255, 255); } \
             #other { width: 100px; height: 20px; background-color: rgb(0, 128, 0); } \
             #switch { width: 100px; height: 20px; } \
             @media (prefers-color-scheme: dark) { \
                 #panel { background-color: rgb(0, 0, 0); } \
             }"
            .to_string(),
        ],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

fn schemed() -> Element {
    let theme = dioxus_compose::use_theme();
    rsx! {
        div { id: "panel" }
        div { id: "other" }
        div {
            id: "switch",
            onclick: move |_| theme.set_color_scheme(dioxus_compose::schema::ColorScheme::Dark),
        }
    }
}

/// The application turns its theme dark, the page's `prefers-color-scheme: dark` rules
/// apply from the same frame, and the batch carries only the box whose style they changed.
///
/// compose-rust does not tell the Host when the system's scheme changes: the renderer
/// applies it on its own side. So the page follows the scheme the application's theme
/// names, and keeps its configured one while the theme follows the system.
#[test]
fn fr34_prefers_color_scheme_follows_the_theme() {
    let mut host = Host::html(schemed, schemed_config);
    let first = tree(records(host.rebuild().unwrap()));
    let panel = node_with(&first, &Modifier::Background(rgb(255, 255, 255)));
    let index = first
        .iter()
        .find_map(|record| match record {
            Record::Modifier(node, index, Modifier::Background(_)) if *node == panel => {
                Some(*index)
            }
            _ => None,
        })
        .unwrap();

    let after = click(&mut host, clickable(&first));
    assert_eq!(
        after,
        vec![Record::Modifier(
            panel,
            index,
            Modifier::Background(rgb(0, 0, 0))
        )]
    );

    let next = tree(records(host.render_frame(0).unwrap()));
    assert_eq!(next, Vec::new(), "the scheme is applied once");
}

/// The `Text` node drawing `text`. There is exactly one.
#[track_caller]
fn text_node(records: &[Record], text: &str) -> u32 {
    let found: Vec<u32> = records
        .iter()
        .filter_map(|record| match record {
            Record::Prop(node, PropertyKind::Text, Value::Str(given)) if given == text => {
                Some(*node)
            }
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one node draws {text:?}: {records:#?}");
    found[0]
}

/// The spans each `Spans` record in `records` gives `node`, in order.
fn spans_sent(records: &[Record], node: u32) -> Vec<Vec<TextSpan>> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Prop(id, PropertyKind::Spans, Value::Bytes(bytes)) if *id == node => {
                Some(TextSpans::from_bytes(bytes.clone()).spans().collect())
            }
            _ => None,
        })
        .collect()
}

fn slanted() -> Element {
    rsx! {
        p { style: "font-style: italic", "Leaning" }
        p { style: "font-style: oblique", "Tilted" }
        p { em { "stressed" } }
        p { "Upright" }
    }
}

/// Text set in `font-style: italic` or `oblique` is sent with one italic span over all of
/// it, which is how compose-rust's text takes a slant. `<em>` is italic by the browser's
/// own stylesheet, and its run gets the same span. Upright text gets none.
#[test]
fn fr34_italic_text_is_sent_as_a_whole_text_span() {
    let mut host = Host::html(slanted, config);
    let first = tree(records(host.rebuild().unwrap()));

    for text in ["Leaning", "Tilted", "stressed"] {
        let node = text_node(&first, text);
        assert_eq!(
            spans_sent(&first, node),
            vec![vec![TextSpan::new(0, text.len() as u32).italic()]],
            "{text:?} is one italic span: {first:#?}"
        );
    }
    let upright = text_node(&first, "Upright");
    assert_eq!(spans_sent(&first, upright), Vec::<Vec<TextSpan>>::new());
}

fn ruled() -> Element {
    rsx! {
        p { style: "text-decoration-line: underline", "Under" }
        p { style: "text-decoration-line: line-through", "Struck" }
        p { style: "text-decoration-line: underline line-through", "Both" }
        p { style: "font-style: italic; text-decoration-line: underline", "Everything" }
        div { style: "text-decoration-line: line-through",
            p { "Inherited" }
        }
    }
}

/// `text-decoration-line: underline`, `line-through`, and both together are sent as one
/// span over the whole run with those lines, combined with a slant when there is one. A
/// box's lines are drawn across the text of the boxes inside it, though the property is
/// not inherited, so a paragraph inside a struck-through box is struck through too.
#[test]
fn fr34_underline_and_line_through_are_sent_as_a_whole_text_span() {
    let mut host = Host::html(ruled, config);
    let first = tree(records(host.rebuild().unwrap()));

    let expected = [
        ("Under", TextSpan::new(0, 5).underline()),
        ("Struck", TextSpan::new(0, 6).strikethrough()),
        ("Both", TextSpan::new(0, 4).underline().strikethrough()),
        ("Everything", TextSpan::new(0, 10).italic().underline()),
        ("Inherited", TextSpan::new(0, 9).strikethrough()),
    ];
    for (text, span) in expected {
        let node = text_node(&first, text);
        assert_eq!(
            spans_sent(&first, node),
            vec![vec![span]],
            "{text:?} is one span: {first:#?}"
        );
    }
}

fn leaning_on_click() -> Element {
    let mut presses = use_signal(|| 0u32);
    let slant = if presses() % 2 == 1 {
        "font-style: italic"
    } else {
        "font-style: normal"
    };
    rsx! {
        div {
            style: "width: 100px; height: 20px; {slant}",
            onclick: move |_| presses += 1,
            "Plain"
        }
    }
}

/// Text with no slant and no lines sends no `Spans` record at all, so a plain page's
/// records are what they were before spans were sent. A run that turns italic gets its
/// span, and when it turns upright again the span is taken back and nothing else changes.
#[test]
fn fr34_plain_text_sends_no_span() {
    let mut host = Host::html(leaning_on_click, config);
    let first = tree(records(host.rebuild().unwrap()));
    assert!(
        !first
            .iter()
            .any(|record| matches!(record, Record::Prop(_, PropertyKind::Spans, _))),
        "no span is sent for plain text: {first:#?}"
    );
    let text = text_node(&first, "Plain");
    let target = clickable(&first);

    let italic = click(&mut host, target);
    assert_eq!(
        spans_sent(&italic, text),
        vec![vec![TextSpan::new(0, 5).italic()]]
    );
    assert_eq!(italic.len(), 1, "only the span is sent: {italic:#?}");

    let upright = click(&mut host, target);
    assert_eq!(
        upright,
        vec![Record::Prop(text, PropertyKind::Spans, Value::None)]
    );
}

fn transformed() -> Element {
    rsx! {
        div { style: "position: relative; width: 300px; height: 200px",
            div { style: "position: absolute; left: 10px; top: 5px; width: 100px; height: 40px; background-color: rgb(255, 0, 0); transform: translate(20px, 30px) scale(2); transform-origin: 0 0" }
        }
    }
}

/// A 2D transform goes to compose-rust as one `Transform` modifier, with its
/// `transform-origin` as a fraction of the box: `0 0` here, the top left.
#[test]
fn fr34_1_transform_is_sent_as_a_matrix_about_the_top_left() {
    let mut host = Host::html(transformed, config);
    let first = tree(records(host.rebuild().unwrap()));
    let node = node_with(&first, &Modifier::Background(rgb(255, 0, 0)));
    let modifiers = modifiers_of(&first, node);
    let sent = modifiers.iter().find_map(|modifier| match modifier {
        Modifier::Transform {
            a,
            b,
            c,
            d,
            e,
            f,
            origin_x,
            origin_y,
        } => Some([*a, *b, *c, *d, *e, *f, *origin_x, *origin_y]),
        _ => None,
    });
    // `translate(20px, 30px) scale(2)` about the top left: x goes to 2x + 20.
    assert_eq!(
        sent,
        Some([2.0, 0.0, 0.0, 2.0, 20.0, 30.0, 0.0, 0.0]),
        "{modifiers:?}"
    );
}

fn title_bar() -> Element {
    rsx! {
        style { ".bar {{ -webkit-app-region: drag; height: 40px; }} .bar .tool {{ app-region: no-drag; width: 30px; height: 20px; }}" }
        div { class: "bar",
            span { "title" }
            button { class: "tool", "x" }
        }
        div { id: "plain", style: "height: 10px", "page" }
    }
}

#[test]
fn fr34_app_region_drag_and_no_drag_reach_the_renderer_as_window_drag() {
    let mut host = Host::html(title_bar, config);
    let first = tree(records(host.rebuild().unwrap()));
    let dragged = node_with(&first, &Modifier::WindowDrag(true));
    let cut_out = node_with(&first, &Modifier::WindowDrag(false));
    assert_ne!(dragged, cut_out);
    assert!(
        modifiers_of(&first, dragged).contains(&Modifier::WindowDrag(true)),
        "the drag region carries WindowDrag(true): {first:#?}"
    );
    // Only the two elements the page marked carry the modifier.
    let marked = first
        .iter()
        .filter(|record| matches!(record, Record::Modifier(_, _, Modifier::WindowDrag(_))))
        .count();
    assert_eq!(marked, 2, "{first:#?}");
}

#[test]
fn fr34_app_region_unchanged_frame_sends_nothing() {
    let mut host = Host::html(title_bar, config);
    host.rebuild().unwrap();
    assert_eq!(tree(records(host.render_frame(0).unwrap())), Vec::new());
}

fn button_under_block() -> Element {
    rsx! {
        div { style: "display: block;",
            button { style: "display: inline-flex;", "다시 연결" }
        }
    }
}

#[test]
fn fr34_button_under_block_sends_label_and_soft_wrap_false() {
    let mut host = Host::html(button_under_block, config);
    let first = tree(records(host.rebuild().unwrap()));
    let text_nodes = created(&first, WidgetKind::Text);
    assert!(
        !text_nodes.is_empty(),
        "a text widget was created: {first:#?}"
    );
    let text_node = text_nodes[0];
    let props: Vec<(PropertyKind, Value)> = first
        .iter()
        .filter_map(|record| match record {
            Record::Prop(id, prop, val) if *id == text_node => Some((*prop, val.clone())),
            _ => None,
        })
        .collect();
    let text = props
        .iter()
        .find(|(prop, _)| *prop == PropertyKind::Text)
        .and_then(|(_, val)| match val {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        });
    assert_eq!(text, Some("다시 연결"), "button label text: {props:#?}");
    let soft_wrap = props
        .iter()
        .find(|(prop, _)| *prop == PropertyKind::SoftWrap)
        .and_then(|(_, val)| match val {
            Value::Bool(b) => Some(*b),
            _ => None,
        });
    assert_eq!(
        soft_wrap,
        Some(false),
        "soft wrap is false for unconstrained label: {props:#?}"
    );
}

fn ellipsis_boxes() -> Element {
    rsx! {
        div {
            style: "width: 100px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis",
            "abcdefghijklmnop"
        }
        div { style: "width: 100px; overflow: hidden; white-space: nowrap", "qrstuvwxyzabcdef" }
    }
}

/// The value the last record for `property` on `node` gave it.
#[track_caller]
fn prop_of(records: &[Record], node: u32, property: PropertyKind) -> Value {
    records
        .iter()
        .rev()
        .find_map(|record| match record {
            Record::Prop(id, given, value) if *id == node && *given == property => {
                Some(value.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("node {node} has no {property:?}: {records:#?}"))
}

/// A one-line box with `overflow: hidden`, `white-space: nowrap` and
/// `text-overflow: ellipsis` ends its text in an ellipsis, as Chromium does. The 16
/// characters are 160px and the box is 100px, so the text node is sent as one line that
/// does not wrap, cut with an ellipsis, and as wide as the box: the renderer places the
/// ellipsis at the width it is given. The same box without `text-overflow: ellipsis` keeps
/// the whole 160px line, which the box clips.
#[test]
fn fr34_ellipsis_box_sends_an_ellipsis_at_the_box_width() {
    let mut host = Host::html(ellipsis_boxes, config);
    let first = tree(records(host.rebuild().unwrap()));

    let cut = text_node(&first, "abcdefghijklmnop");
    assert_eq!(
        prop_of(&first, cut, PropertyKind::Overflow),
        Value::Int(TextOverflow::Ellipsis as i64)
    );
    assert_eq!(prop_of(&first, cut, PropertyKind::MaxLines), Value::Int(1));
    assert_eq!(
        prop_of(&first, cut, PropertyKind::SoftWrap),
        Value::Bool(false)
    );
    let modifiers = modifiers_of(&first, cut);
    assert!(
        modifiers.contains(&Modifier::Width(100.0)),
        "the text is as wide as the box: {modifiers:?}"
    );

    let clipped = text_node(&first, "qrstuvwxyzabcdef");
    assert_eq!(
        prop_of(&first, clipped, PropertyKind::Overflow),
        Value::Int(TextOverflow::Visible as i64)
    );
    let modifiers = modifiers_of(&first, clipped);
    assert!(
        modifiers.contains(&Modifier::Width(160.0)),
        "the text keeps its whole line: {modifiers:?}"
    );
}

fn density() -> Element {
    let mut chosen = use_signal(|| 0usize);
    rsx! {
        for (index, label) in ["Comfortable", "Compact"].into_iter().enumerate() {
            label { key: "{label}",
                input {
                    r#type: "radio",
                    name: "density",
                    value: "{label}",
                    checked: chosen() == index,
                    onclick: move |_| chosen.set(index),
                }
                "{label}"
            }
        }
        input { r#type: "checkbox", checked: true }
    }
}

/// The checked radio button of a group is sent checked and the other unchecked, so the
/// renderer draws the dot in the one the app chose. Neither it nor the checkbox sits in a
/// box that draws the user-agent `input` border or background around the control.
#[test]
fn fr34_checked_radio_is_sent_checked_and_controls_draw_no_input_box() {
    let mut host = Host::html(density, config);
    let first = tree(records(host.rebuild().unwrap()));

    let radios = created(&first, WidgetKind::RadioButton);
    assert_eq!(radios.len(), 2, "two radio buttons: {first:#?}");
    let checked: Vec<Option<bool>> = radios
        .iter()
        .map(|radio| {
            first.iter().rev().find_map(|record| match record {
                Record::Prop(id, PropertyKind::Checked, Value::Bool(on)) if id == radio => {
                    Some(*on)
                }
                _ => None,
            })
        })
        .collect();
    assert_eq!(
        checked,
        vec![Some(true), Some(false)],
        "Comfortable is sent checked, Compact unchecked: {first:#?}"
    );

    let checkbox = created(&first, WidgetKind::Checkbox);
    assert_eq!(checkbox.len(), 1);
    assert!(first.contains(&Record::Prop(
        checkbox[0],
        PropertyKind::Checked,
        Value::Bool(true)
    )));

    for control in radios.iter().chain(&checkbox) {
        let holder = first
            .iter()
            .find_map(|record| match record {
                Record::Insert { parent, node, .. } if node == control => Some(*parent),
                _ => None,
            })
            .unwrap_or_else(|| panic!("control {control} is inserted: {first:#?}"));
        for modifier in modifiers_of(&first, holder)
            .into_iter()
            .chain(modifiers_of(&first, *control))
        {
            assert!(
                !matches!(
                    modifier,
                    Modifier::Background(_) | Modifier::Border { .. } | Modifier::BorderEach { .. }
                ),
                "control {control} is drawn without the input box, but has {modifier:?}: {first:#?}"
            );
        }
    }
}

fn broken_avatar() -> Element {
    rsx! {
        div { style: "display: flex; align-items: flex-start",
            img { src: "avatars/me.png", alt: "Me" }
            img { src: "cat.png", alt: "Cat" }
        }
    }
}

/// An image the resolver cannot draw reaches the renderer as Chromium draws a broken
/// image: a `Canvas` drawing the 16px broken-image icon, and its alt text as a `Text` beside
/// it, inside a clipping box the size of the image. An image the resolver draws sends its
/// picture and not its alt text.
#[test]
fn fr34_unresolved_img_sends_the_broken_image_icon_and_its_alt_text() {
    let mut host = Host::html(broken_avatar, config_with_cat);
    let first = tree(records(host.rebuild().unwrap()));

    let canvases = created(&first, WidgetKind::Canvas);
    assert_eq!(
        canvases.len(),
        1,
        "one icon, for the image with no asset: {first:#?}"
    );
    let icon = canvases[0];
    let drawing = first
        .iter()
        .find_map(|record| match record {
            Record::Prop(node, PropertyKind::Commands, Value::Bytes(bytes)) if *node == icon => {
                Some(bytes.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("the icon is given its drawing: {first:#?}"));
    let commands = dioxus_compose::drawing::decode_commands(&drawing).expect("the drawing decodes");
    assert!(!commands.is_empty(), "the icon draws something");
    assert_eq!(
        modifiers_of(&first, icon),
        vec![
            Modifier::Offset { x: 0.0, y: 0.0 },
            Modifier::RequiredSize {
                width: 16.0,
                height: 16.0
            },
        ]
    );

    let alt = text_node(&first, "Me");
    assert!(
        modifiers_of(&first, alt).contains(&Modifier::Offset { x: 16.0, y: 0.0 }),
        "the alt text starts after the icon: {first:#?}"
    );
    assert!(
        !texts(&first).iter().any(|text| text == "Cat"),
        "the image the resolver draws shows no alt text"
    );
    assert_eq!(
        created(&first, WidgetKind::Image).len(),
        1,
        "the cat is drawn"
    );

    // The box both sit in clips them to the image: "Me" is 20px, so 36 by 20.
    let holder = first
        .iter()
        .find_map(|record| match record {
            Record::Insert { parent, node, .. } if *node == icon => Some(*parent),
            _ => None,
        })
        .expect("the icon is inserted");
    let holder_modifiers = modifiers_of(&first, holder);
    assert!(
        holder_modifiers.contains(&Modifier::RequiredSize {
            width: 36.0,
            height: 20.0
        }) && holder_modifiers.contains(&Modifier::Clip(true)),
        "{holder_modifiers:#?}"
    );

    let next = tree(records(host.render_frame(0).unwrap()));
    assert_eq!(next, Vec::new(), "an unchanged frame sends nothing");
}
