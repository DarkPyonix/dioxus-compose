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
use dioxus_compose::schema::{Color, EventPayload, Modifier, Paint, PropertyKind, WidgetKind};
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
    Bytes,
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
                    PropertyValue::Bytes(_) => Value::Bytes,
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

    let window = match first[0] {
        Record::Create(node, WidgetKind::ScrollColumn) => node,
        ref other => panic!("the window's scroll comes first: {other:?}"),
    };
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
