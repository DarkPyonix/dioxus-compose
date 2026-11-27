//! A widget's unfilled weight, and a rich text field written with `rsx!`: what the field
//! and its handle put on the wire, and how the field's answers reach the application.

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, FormatOp, Modifier, PropertyKind, WidgetKind};

/// One decoded record with its borrowed parts made owned, so the batch can be dropped.
#[derive(Clone, Debug, PartialEq)]
enum Record {
    Create(u32, WidgetKind),
    Prop(u32, PropertyKind, Option<i64>, Option<bool>, Option<String>),
    Modifier(u32, Modifier),
    FormatText(u32, FormatOp, u16),
    SetRichText(u32, String),
    Other,
}

fn records(batch: &[u8]) -> Vec<Record> {
    decode_batch(batch)
        .expect("the batch decodes")
        .into_iter()
        .map(|mutation| match mutation {
            Mutation::Create { node_id, widget } => Record::Create(node_id, widget),
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => match value {
                PropertyValue::Integer(number) => {
                    Record::Prop(node_id, property, Some(number), None, None)
                }
                PropertyValue::Bool(flag) => {
                    Record::Prop(node_id, property, None, Some(flag), None)
                }
                PropertyValue::String(text) => {
                    Record::Prop(node_id, property, None, None, Some(text.to_string()))
                }
                _ => Record::Prop(node_id, property, None, None, None),
            },
            Mutation::SetModifier {
                node_id, modifier, ..
            } => Record::Modifier(node_id, modifier),
            Mutation::FormatText {
                node_id, op, value, ..
            } => Record::FormatText(node_id, op, value),
            Mutation::SetRichText { node_id, text, .. } => {
                Record::SetRichText(node_id, text.to_string())
            }
            _ => Record::Other,
        })
        .collect()
}

fn nodes_of(records: &[Record], widget: WidgetKind) -> Vec<u32> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Create(node_id, kind) if *kind == widget => Some(*node_id),
            _ => None,
        })
        .collect()
}

#[track_caller]
fn node_of(records: &[Record], widget: WidgetKind) -> u32 {
    nodes_of(records, widget)
        .first()
        .copied()
        .unwrap_or_else(|| panic!("no {widget:?} was created: {records:#?}"))
}

/// The props one node was sent.
fn props_of(records: &[Record], node: u32) -> Vec<&Record> {
    records
        .iter()
        .filter(|record| matches!(record, Record::Prop(id, ..) if *id == node))
        .collect()
}

#[track_caller]
fn handler_of(records: &[Record], node: u32, property: PropertyKind) -> u64 {
    records
        .iter()
        .find_map(|record| match record {
            Record::Prop(id, kind, Some(handler), ..) if *id == node && *kind == property => {
                Some(*handler as u64)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("node {node} has no {property:?} handler: {records:#?}"))
}

fn weighted() -> Element {
    rsx! {
        Row {
            Spacer { weight: 1.0 }
            Spacer { weight: 2.0, weight_fill: false }
        }
    }
}

/// `weight_fill: false` sends the weight as `WeightUnfilled`, and a weight left filled is
/// still sent as `Weight`.
#[test]
fn fr15_weight_fill_false_sends_weight_unfilled() {
    let mut host = Host::new(weighted);
    let batch = records(host.rebuild().unwrap());
    let spacers = nodes_of(&batch, WidgetKind::Spacer);
    assert_eq!(spacers.len(), 2, "{batch:#?}");
    let modifiers = |node: u32| -> Vec<Modifier> {
        batch
            .iter()
            .filter_map(|record| match record {
                Record::Modifier(id, modifier) if *id == node => Some(modifier.clone()),
                _ => None,
            })
            .collect()
    };
    assert!(
        modifiers(spacers[0]).contains(&Modifier::Weight(1.0)),
        "{:?}",
        modifiers(spacers[0])
    );
    assert!(
        !modifiers(spacers[0])
            .iter()
            .any(|modifier| matches!(modifier, Modifier::WeightUnfilled(_))),
        "{:?}",
        modifiers(spacers[0])
    );
    assert!(
        modifiers(spacers[1]).contains(&Modifier::WeightUnfilled(2.0)),
        "{:?}",
        modifiers(spacers[1])
    );
    assert!(
        !modifiers(spacers[1])
            .iter()
            .any(|modifier| matches!(modifier, Modifier::Weight(_))),
        "{:?}",
        modifiers(spacers[1])
    );
}

fn rich_field() -> Element {
    let rich = use_rich_text();
    rsx! {
        Column {
            TextField { rich_text: rich }
            TextField { placeholder: "plain" }
        }
    }
}

/// A field given a rich text handle is sent `RichText` and both of its handlers, so its
/// handle hears the field's answers. A plain field is sent none of them.
#[test]
fn fr15_rich_text_field_sends_rich_text_and_its_handlers() {
    let mut host = Host::new(rich_field);
    let batch = records(host.rebuild().unwrap());
    let fields = nodes_of(&batch, WidgetKind::TextField);
    assert_eq!(fields.len(), 2, "{batch:#?}");
    let (rich, plain) = (fields[0], fields[1]);
    assert!(
        props_of(&batch, rich).iter().any(|record| matches!(
            record,
            Record::Prop(_, PropertyKind::RichText, None, Some(true), None)
        )),
        "{batch:#?}"
    );
    handler_of(&batch, rich, PropertyKind::OnTextFormat);
    handler_of(&batch, rich, PropertyKind::OnRichTextChanged);
    for property in [
        PropertyKind::RichText,
        PropertyKind::OnTextFormat,
        PropertyKind::OnRichTextChanged,
    ] {
        assert!(
            !props_of(&batch, plain)
                .iter()
                .any(|record| matches!(record, Record::Prop(_, kind, ..) if *kind == property)),
            "a plain field was sent {property:?}"
        );
    }
}

fn formatting() -> Element {
    let rich = use_rich_text();
    dioxus_core::use_hook(|| {
        rich.format(Format::Bold(Toggle::On));
        rich.load(RichDocument::plain("hello"));
    });
    rsx! {
        TextField { rich_text: rich }
    }
}

fn unattached() -> Element {
    let rich = use_rich_text();
    dioxus_core::use_hook(|| rich.format(Format::Italic(Toggle::Flip)));
    rsx! {
        Text { text: "no field" }
    }
}

/// What the handle is asked goes to the field that carries it, in the order it was asked,
/// in the batch that drew the field. A handle no field carries sends nothing.
#[test]
fn fr15_rich_text_handle_sends_format_and_load() {
    let mut host = Host::new(formatting);
    let batch = records(host.rebuild().unwrap());
    let field = node_of(&batch, WidgetKind::TextField);
    let asked: Vec<&Record> = batch
        .iter()
        .filter(|record| matches!(record, Record::FormatText(..) | Record::SetRichText(..)))
        .collect();
    assert_eq!(
        asked,
        [
            &Record::FormatText(field, FormatOp::Bold, 1),
            &Record::SetRichText(field, "hello".to_owned()),
        ],
        "{batch:#?}"
    );

    let mut host = Host::new(unattached);
    let batch = records(host.rebuild().unwrap());
    assert!(
        !batch
            .iter()
            .any(|record| matches!(record, Record::FormatText(..) | Record::SetRichText(..))),
        "a handle no field carries sent a command: {batch:#?}"
    );
}

thread_local! {
    static HEARD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn heard(line: String) {
    HEARD.with_borrow_mut(|lines| lines.push(line));
}

fn listening() -> Element {
    let rich = use_rich_text();
    rsx! {
        Column {
            TextField {
                rich_text: rich,
                on_text_format: move |format: TextFormat| heard(format!("format {:?}", format.bold)),
                on_rich_text_change: move |document: RichDocument| heard(format!("document {}", document.text)),
            }
            Text { text: format!("bold {:?}", rich.format_at_selection().bold) }
            Text { text: format!("text {}", rich.document().text) }
        }
    }
}

/// The field's answers reach the application's handlers, and the handle keeps them: a
/// component that reads the handle draws again with the new formatting and document.
#[test]
fn fr15_rich_text_events_reach_their_handlers() {
    let mut host = Host::new(listening);
    let first = records(host.rebuild().unwrap());
    let field = node_of(&first, WidgetKind::TextField);
    let on_format = handler_of(&first, field, PropertyKind::OnTextFormat);
    let on_document = handler_of(&first, field, PropertyKind::OnRichTextChanged);

    let format = TextFormat {
        bold: Tri::All,
        ..TextFormat::default()
    };
    let (batch, _) = host
        .dispatch(HostEvent::new(
            field,
            on_format,
            EventPayload::TextFormatAtSelection(format),
        ))
        .expect("the formatting is delivered");
    let redrawn = records(batch);
    assert!(
        redrawn.iter().any(|record| matches!(
            record,
            Record::Prop(_, PropertyKind::Text, _, _, Some(text)) if text == "bold All"
        )),
        "{redrawn:#?}"
    );

    let (batch, _) = host
        .dispatch(HostEvent::new(
            field,
            on_document,
            EventPayload::RichTextChanged {
                text: "typed",
                characters: &[],
                paragraphs: &[],
            },
        ))
        .expect("the document is delivered");
    let redrawn = records(batch);
    assert!(
        redrawn.iter().any(|record| matches!(
            record,
            Record::Prop(_, PropertyKind::Text, _, _, Some(text)) if text == "text typed"
        )),
        "{redrawn:#?}"
    );
    assert_eq!(
        HEARD.with_borrow_mut(std::mem::take),
        ["format All", "document typed"]
    );
}
