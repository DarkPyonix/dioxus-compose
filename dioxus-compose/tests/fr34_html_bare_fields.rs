//! An `<input>`, a `<textarea>` and a `<select>` are drawn the way a browser draws them:
//! the CSS border and background are the Host's own boxes, and the renderer's `TextField`
//! or `Dropdown` inside the content box draws nothing of its own around the text.
//!
//! The renderer paints the design system's field frame (a fill, a line, a pill shape, room
//! inside, and a wheel for a dropdown on Apple systems) unless the node names its own. Each
//! test runs an app through a Host and reads what the renderer would receive: the field
//! names a transparent fill, a line of no width, square corners and no room inside, and
//! sets its text in the CSS colour and font rather than a type role of the design system.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px.

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    HtmlConfig, TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint,
};
use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{Color, Modifier, Paint, PropertyKind, WidgetKind};

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
        stylesheets: vec![
            "html, body { margin: 0; padding: 0; }
             input, textarea, select {
                 display: block;
                 width: 200px;
                 color: rgb(10, 20, 30);
                 background-color: rgb(255, 255, 255);
                 border: 1px solid rgb(200, 200, 200);
                 padding: 4px;
                 font-family: Menlo;
                 font-size: 18px;
                 line-height: 24px;
                 font-weight: 700;
             }"
            .to_string(),
        ],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

fn fields() -> Element {
    rsx! {
        input { value: "Ada Lovelace" }
        textarea { "Notes" }
        select {
            option { "one" }
            option { "two" }
        }
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

#[derive(Clone, Debug, PartialEq)]
enum Record {
    Create(u32, WidgetKind),
    Prop(u32, PropertyKind, Value),
    Modifier(u32, u16, Modifier),
    Insert { parent: u32, node: u32 },
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
                parent_id, node_id, ..
            } => Record::Insert {
                parent: parent_id,
                node: node_id,
            },
            _ => Record::Other,
        })
        .collect()
}

fn created(records: &[Record], widget: WidgetKind) -> Vec<u32> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Create(node, kind) if *kind == widget => Some(*node),
            _ => None,
        })
        .collect()
}

/// The modifier list a node ends up with after `records`, by index.
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

/// The last value `records` give `node` for `property`.
fn prop(records: &[Record], node: u32, property: PropertyKind) -> Option<Value> {
    records.iter().rev().find_map(|record| match record {
        Record::Prop(id, given, value) if *id == node && *given == property => Some(value.clone()),
        _ => None,
    })
}

fn transparent() -> Paint {
    Paint::Literal(Color::argb(0))
}

fn text_colour() -> i64 {
    Paint::Literal(Color::argb(0xff0a_141e)).to_bits() as i64
}

/// The modifiers that tell the renderer's field to draw no frame of its own: a fill it
/// cannot see, a line of no width, square corners and no room inside.
#[track_caller]
fn assert_bare(records: &[Record], node: u32, what: &str) {
    let modifiers = modifiers_of(records, node);
    let bare = [
        Modifier::Background(transparent()),
        Modifier::Border {
            width: 0.0,
            paint: transparent(),
        },
        Modifier::Shape {
            top_start: 0.0,
            top_end: 0.0,
            bottom_end: 0.0,
            bottom_start: 0.0,
        },
        Modifier::Padding(0.0),
    ];
    for modifier in bare {
        assert!(
            modifiers.contains(&modifier),
            "the {what} names {modifier:?} so the design system draws no frame: {modifiers:#?}"
        );
    }
    assert!(
        matches!(modifiers.first(), Some(Modifier::Offset { .. }))
            && matches!(modifiers.get(1), Some(Modifier::RequiredSize { .. })),
        "the {what} is still placed and sized first: {modifiers:#?}"
    );
}

/// The CSS text style on a node: its colour, size, line height and weight.
#[track_caller]
fn assert_css_text(records: &[Record], node: u32, what: &str) {
    assert_eq!(
        prop(records, node, PropertyKind::Color),
        Some(Value::Int(text_colour())),
        "the {what} is set in the CSS colour"
    );
    assert_eq!(
        prop(records, node, PropertyKind::FontSize),
        Some(Value::Float(18.0)),
        "the {what} is set at the CSS font size"
    );
    assert_eq!(
        prop(records, node, PropertyKind::LineHeight),
        Some(Value::Float(24.0)),
        "the {what} has the CSS line height"
    );
    assert_eq!(
        prop(records, node, PropertyKind::FontWeight),
        Some(Value::Int(700)),
        "the {what} has the CSS weight"
    );
}

/// An `<input>` and a `<textarea>` are TextFields with no frame of their own, set in the
/// CSS text style with no type role, so the renderer uses the CSS font they name.
#[test]
fn fr34_input_and_textarea_draw_no_field_frame_of_their_own() {
    let mut host = Host::html(fields, config);
    let first = records(host.rebuild().unwrap());

    let text_fields = created(&first, WidgetKind::TextField);
    assert_eq!(
        text_fields.len(),
        2,
        "the input and the textarea: {first:#?}"
    );
    for (node, what) in text_fields.into_iter().zip(["input", "textarea"]) {
        assert_bare(&first, node, what);
        assert_css_text(&first, node, what);
        assert_eq!(
            prop(&first, node, PropertyKind::TypeRole),
            Some(Value::Int(0)),
            "the {what} takes no type role from the design system"
        );
        assert!(
            matches!(prop(&first, node, PropertyKind::Font), Some(Value::Bytes(bytes)) if !bytes.is_empty()),
            "the {what} names the CSS font family"
        );
    }
}

/// A `<select>` is a Dropdown with no frame of its own that rests as a closed field on
/// every design system, so no wheel is drawn, and its options are set in the CSS style.
#[test]
fn fr34_select_draws_no_field_frame_and_no_wheel() {
    let mut host = Host::html(fields, config);
    let first = records(host.rebuild().unwrap());

    let dropdowns = created(&first, WidgetKind::Dropdown);
    assert_eq!(dropdowns.len(), 1, "the select: {first:#?}");
    let dropdown = dropdowns[0];
    assert_bare(&first, dropdown, "select");
    assert_eq!(
        prop(&first, dropdown, PropertyKind::Collapsible),
        Some(Value::Bool(true)),
        "the select rests as a closed field, never a wheel"
    );

    let options: Vec<u32> = first
        .iter()
        .filter_map(|record| match record {
            Record::Insert { parent, node } if *parent == dropdown => Some(*node),
            _ => None,
        })
        .collect();
    assert_eq!(options.len(), 2, "the select's two options: {first:#?}");
    for option in options {
        assert_css_text(&first, option, "option");
    }
}

/// A frame in which nothing changed sends nothing more for the fields.
#[test]
fn fr34_bare_fields_are_sent_once() {
    let mut host = Host::html(fields, config);
    host.rebuild().unwrap();
    let again = records(host.render_frame(0).unwrap());
    assert!(
        again.iter().all(|record| *record == Record::Other),
        "an unchanged frame sends nothing: {again:#?}"
    );
}
