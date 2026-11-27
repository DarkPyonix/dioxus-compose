//! A request to go back and a backdrop written with `rsx!`: which handler a request to go
//! back reaches and what consuming it reports, and what a backdrop puts on the wire.

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, Modifier, PropertyKind, WidgetKind};

/// One decoded record with its borrowed parts made owned, so the batch can be dropped.
#[derive(Clone, Debug, PartialEq)]
enum Record {
    Create(u32, WidgetKind),
    Prop(u32, PropertyKind, Option<i64>),
    Modifier(u32, Modifier),
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
                PropertyValue::Integer(number) => Record::Prop(node_id, property, Some(number)),
                _ => Record::Prop(node_id, property, None),
            },
            Mutation::SetModifier {
                node_id, modifier, ..
            } => Record::Modifier(node_id, modifier),
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
fn handler_of(records: &[Record], node: u32, property: PropertyKind) -> u64 {
    records
        .iter()
        .find_map(|record| match record {
            Record::Prop(id, kind, Some(handler)) if *id == node && *kind == property => {
                Some(*handler as u64)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("node {node} has no {property:?} handler: {records:#?}"))
}

fn modifiers_of(records: &[Record], node: u32) -> Vec<Modifier> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Modifier(id, modifier) if *id == node => Some(modifier.clone()),
            _ => None,
        })
        .collect()
}

thread_local! {
    /// Which handler heard a request to go back, and the source it heard.
    static HEARD: RefCell<Vec<(&'static str, BackSource)>> = const { RefCell::new(Vec::new()) };
}

fn heard() -> Vec<(&'static str, BackSource)> {
    HEARD.with_borrow_mut(std::mem::take)
}

/// An outer column that only listens, and an inner one that acts on the request.
fn nested() -> Element {
    rsx! {
        Column {
            on_back_requested: move |request: BackRequest| {
                HEARD.with_borrow_mut(|seen| seen.push(("outer", request.source())));
            },
            Column {
                on_back_requested: move |request: BackRequest| {
                    HEARD.with_borrow_mut(|seen| seen.push(("inner", request.source())));
                    request.consume();
                },
                Text { text: "page" }
            }
        }
    }
}

fn back(source: BackSource) -> EventPayload<'static> {
    EventPayload::BackRequested { source }
}

/// The Renderer offers a request to go back to the innermost container with a handler,
/// then to each one further out until a handler consumes it. What the Host owes it is that
/// the handler it named is the only one called, and that the call's result says whether
/// that handler consumed the request: 1 stops the request there, 0 lets the next one out
/// hear it.
#[test]
fn fr15_back_request_reaches_the_innermost_handler_and_stops_when_consumed() {
    heard();
    let mut host = Host::new(nested);
    let batch = records(host.rebuild().unwrap());
    let columns = nodes_of(&batch, WidgetKind::Column);
    assert_eq!(columns.len(), 2, "{batch:#?}");
    let (outer, inner) = (columns[0], columns[1]);
    let inner_handler = handler_of(&batch, inner, PropertyKind::OnBackRequested);
    let outer_handler = handler_of(&batch, outer, PropertyKind::OnBackRequested);
    assert_ne!(inner_handler, outer_handler);

    let (_, consumed) = host
        .dispatch(HostEvent::new(
            inner,
            inner_handler,
            back(BackSource::Escape),
        ))
        .expect("the inner handler takes a request to go back");
    assert_eq!(
        consumed, 1,
        "the inner handler consumed the request, so the call reports it consumed"
    );
    assert_eq!(
        heard(),
        [("inner", BackSource::Escape)],
        "only the handler the Renderer named is called; the outer one does not hear a \
         request the inner one consumed"
    );

    let (_, consumed) = host
        .dispatch(HostEvent::new(
            outer,
            outer_handler,
            back(BackSource::SystemBack),
        ))
        .expect("the outer handler takes a request to go back");
    assert_eq!(
        consumed, 0,
        "the outer handler did not consume the request, so it goes on to the platform"
    );
    assert_eq!(heard(), [("outer", BackSource::SystemBack)]);
}

fn backdropped() -> Element {
    rsx! {
        Column {
            Card {
                backdrop: Backdrop::new(20.0, 1.6, Paint::Role(ColorRole::Surface)),
                Text { text: "glass" }
            }
            Card { Text { text: "plain" } }
        }
    }
}

/// `backdrop` sends one `Backdrop` modifier with the blur, the saturation and the tint as
/// written, and a widget without one sends none.
#[test]
fn fr15_backdrop_sends_its_blur_saturation_and_tint() {
    let mut host = Host::new(backdropped);
    let batch = records(host.rebuild().unwrap());
    let cards = nodes_of(&batch, WidgetKind::Card);
    assert_eq!(cards.len(), 2, "{batch:#?}");

    let backdrops = |node: u32| -> Vec<Modifier> {
        modifiers_of(&batch, node)
            .into_iter()
            .filter(|modifier| matches!(modifier, Modifier::Backdrop { .. }))
            .collect()
    };
    assert_eq!(
        backdrops(cards[0]),
        [Modifier::Backdrop {
            blur_dp: 20.0,
            saturation: 1.6,
            tint: Paint::Role(ColorRole::Surface),
        }],
        "{:?}",
        modifiers_of(&batch, cards[0])
    );
    assert!(
        backdrops(cards[1]).is_empty(),
        "a card without a backdrop sent {:?}",
        backdrops(cards[1])
    );
    let column = nodes_of(&batch, WidgetKind::Column)[0];
    assert!(backdrops(column).is_empty());
}
