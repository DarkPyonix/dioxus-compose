//! LazyColumn windowing: exactly the requested range is materialised.
//!
//! The read-ahead buffer belongs to the Renderer, because the Renderer is the side that
//! owns the scroll position and so is the only one that knows how far ahead to read. The
//! Host must not widen the range it was asked for: `start` is the global index of the first
//! item the Host materialises, and if the Host added its own buffer the Renderer could no
//! longer tell where the subtree it received belongs in the full list.

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, PropertyKind, WidgetKind};
use std::collections::HashMap;

const ITEM_COUNT: usize = 10_000;

fn app() -> Element {
    rsx! {
        LazyColumn {
            item_count: ITEM_COUNT,
            item: move |index: usize| rsx! { Text { text: "message {index}" } },
        }
    }
}

/// Minimal stand-in for the Compose interpreter: applies a batch to a node table.
#[derive(Default)]
struct MockRenderer {
    widgets: HashMap<u32, WidgetKind>,
    texts: HashMap<u32, String>,
    item_keys: HashMap<u32, String>,
    parents: HashMap<u32, u32>,
    item_count: Option<i64>,
}

impl MockRenderer {
    fn apply(&mut self, batch: &[Mutation<'_>]) {
        for mutation in batch {
            match mutation {
                Mutation::Create { node_id, widget } => {
                    self.widgets.insert(*node_id, *widget);
                }
                Mutation::Insert {
                    parent_id, node_id, ..
                }
                | Mutation::Move {
                    parent_id, node_id, ..
                } => {
                    self.parents.insert(*node_id, *parent_id);
                }
                // Removing a node removes its whole subtree, as Compose does.
                Mutation::Remove { node_id } => self.remove_subtree(*node_id),
                Mutation::SetProp {
                    node_id,
                    property,
                    value,
                } => match (property, value) {
                    (PropertyKind::Text, PropertyValue::String(value)) => {
                        self.texts.insert(*node_id, (*value).to_owned());
                    }
                    (PropertyKind::ItemKey, PropertyValue::String(value)) => {
                        self.item_keys.insert(*node_id, (*value).to_owned());
                    }
                    (PropertyKind::ItemCount, PropertyValue::Integer(value)) => {
                        self.item_count = Some(*value);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    fn remove_subtree(&mut self, node_id: u32) {
        let children: Vec<_> = self
            .parents
            .iter()
            .filter(|(_, parent)| **parent == node_id)
            .map(|(child, _)| *child)
            .collect();
        for child in children {
            self.remove_subtree(child);
        }
        self.widgets.remove(&node_id);
        self.texts.remove(&node_id);
        self.item_keys.remove(&node_id);
        self.parents.remove(&node_id);
    }

    fn live_texts(&self) -> Vec<String> {
        let mut texts: Vec<_> = self
            .texts
            .iter()
            .filter(|(node_id, _)| self.widgets.contains_key(node_id))
            .map(|(_, text)| text.clone())
            .collect();
        texts.sort();
        texts
    }

    fn live_item_keys(&self) -> Vec<String> {
        let mut keys: Vec<_> = self
            .item_keys
            .iter()
            .filter(|(node_id, _)| self.widgets.contains_key(node_id))
            .map(|(_, key)| key.clone())
            .collect();
        keys.sort();
        keys
    }

    fn node_count(&self) -> usize {
        self.widgets.len()
    }
}

fn expected_texts(start: usize, count: usize) -> Vec<String> {
    let first = start.min(ITEM_COUNT);
    let last = (first + count).min(ITEM_COUNT);
    let mut texts: Vec<_> = (first..last)
        .map(|index| format!("message {index}"))
        .collect();
    texts.sort();
    texts
}

struct Fixture {
    host: Host,
    mock: MockRenderer,
    list_node_id: u32,
    handler_id: u64,
}

impl Fixture {
    fn new() -> Self {
        let mut host = Host::new(app);
        let initial = decode_batch(host.rebuild().unwrap()).unwrap();
        let mut mock = MockRenderer::default();
        mock.apply(&initial);
        let (list_node_id, handler_id) = initial
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::SetProp {
                    node_id,
                    property: PropertyKind::OnRangeRequested,
                    value: PropertyValue::Integer(handler),
                } => Some((*node_id, *handler as u64)),
                _ => None,
            })
            .expect("LazyColumn declares a range-request handler");
        drop(initial);
        Self {
            host,
            mock,
            list_node_id,
            handler_id,
        }
    }

    fn request_range(&mut self, start: usize, count: usize) {
        let event = HostEvent {
            node_id: self.list_node_id,
            handler_id: self.handler_id,
            payload: EventPayload::RangeRequested {
                start: start as u32,
                count: count as u32,
            },
        };
        let (batch, _) = self.host.dispatch(event).unwrap();
        let decoded = decode_batch(batch).unwrap();
        self.mock.apply(&decoded);
    }
}

#[test]
fn fr8_only_the_requested_range_is_materialised() {
    let mut fixture = Fixture::new();
    assert_eq!(fixture.mock.item_count, Some(ITEM_COUNT as i64));
    assert!(fixture.mock.live_texts().len() < ITEM_COUNT);

    fixture.request_range(100, 20);

    assert_eq!(fixture.mock.live_texts(), expected_texts(100, 20));
}

#[test]
fn fr8_node_count_is_proportional_to_the_window() {
    let mut fixture = Fixture::new();
    fixture.request_range(5_000, 20);

    let window = 20;
    let nodes = fixture.mock.node_count();
    assert_eq!(fixture.mock.live_texts().len(), window);
    assert!(
        nodes <= 2 * window + 2,
        "{nodes} nodes materialised for a window of {window} out of {ITEM_COUNT} items"
    );
}

#[test]
fn fr8_re_requesting_an_earlier_range_restores_it() {
    let mut fixture = Fixture::new();
    fixture.request_range(0, 10);
    let first_texts = fixture.mock.live_texts();
    let first_keys = fixture.mock.live_item_keys();

    fixture.request_range(5_000, 10);
    assert_eq!(fixture.mock.live_texts(), expected_texts(5_000, 10));

    // Scrolling back must rebuild the identical subtree: the Host owns the data (D5).
    fixture.request_range(0, 10);
    assert_eq!(fixture.mock.live_texts(), first_texts);
    assert_eq!(fixture.mock.live_item_keys(), first_keys);
    assert_eq!(fixture.mock.live_texts(), expected_texts(0, 10));
}
