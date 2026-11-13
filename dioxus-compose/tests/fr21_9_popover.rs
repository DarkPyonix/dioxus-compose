//! `Popover`: a floating box of any content hung off an anchor, taken through the wire and
//! replayed by a small renderer that keeps each parent's children in order.

use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch, encode_event};
use dioxus_compose::{EventPayload, Host, PropertyKind, WidgetKind};
use std::cell::RefCell;
use std::collections::HashMap;

/// What a renderer keeps: which widget each node is, its children in drawing order, and
/// the integer and boolean properties last set on it.
#[derive(Default)]
struct MockRenderer {
    widgets: HashMap<u32, WidgetKind>,
    children: HashMap<u32, Vec<u32>>,
    parents: HashMap<u32, u32>,
    ints: HashMap<(u32, u16), i64>,
    bools: HashMap<(u32, u16), bool>,
    floats: HashMap<(u32, u16), f64>,
    texts: HashMap<(u32, u16), String>,
}

impl MockRenderer {
    fn detach(&mut self, node: u32) {
        if let Some(parent) = self.parents.remove(&node)
            && let Some(list) = self.children.get_mut(&parent)
        {
            list.retain(|child| *child != node);
        }
    }

    fn attach(&mut self, parent: u32, node: u32, index: u32) {
        // Node 0 is the Host's placeholder for a dynamic slot that produced nothing.
        if node == 0 {
            return;
        }
        self.detach(node);
        let list = self.children.entry(parent).or_default();
        let at = (index as usize).min(list.len());
        list.insert(at, node);
        self.parents.insert(node, parent);
    }

    fn apply(&mut self, batch: &[u8]) {
        for mutation in decode_batch(batch).unwrap() {
            match mutation {
                Mutation::Create { node_id, widget } => {
                    self.widgets.insert(node_id, widget);
                }
                Mutation::Insert {
                    parent_id,
                    node_id,
                    index,
                }
                | Mutation::Move {
                    parent_id,
                    node_id,
                    index,
                } => self.attach(parent_id, node_id, index),
                Mutation::Remove { node_id } => {
                    self.detach(node_id);
                    self.widgets.remove(&node_id);
                }
                Mutation::SetProp {
                    node_id,
                    property,
                    value,
                } => match value {
                    PropertyValue::Integer(value) => {
                        self.ints.insert((node_id, property as u16), value);
                    }
                    PropertyValue::Bool(value) => {
                        self.bools.insert((node_id, property as u16), value);
                    }
                    PropertyValue::Float(value) => {
                        self.floats.insert((node_id, property as u16), value as f64);
                    }
                    PropertyValue::String(text) => {
                        self.texts
                            .insert((node_id, property as u16), text.to_owned());
                    }
                    _ => {}
                },
                Mutation::SetText { node_id, text, .. } => {
                    self.texts
                        .insert((node_id, PropertyKind::Text as u16), text.to_owned());
                }
                _ => {}
            }
        }
    }

    fn only(&self, widget: WidgetKind) -> u32 {
        let found: Vec<u32> = self
            .widgets
            .iter()
            .filter(|(_, kind)| **kind == widget)
            .map(|(node, _)| *node)
            .collect();
        assert_eq!(found.len(), 1, "expected one {widget:?}, found {found:?}");
        found[0]
    }

    fn children_of(&self, node: u32) -> Vec<u32> {
        self.children.get(&node).cloned().unwrap_or_default()
    }

    fn handler(&self, node: u32, property: PropertyKind) -> u64 {
        *self
            .ints
            .get(&(node, property as u16))
            .unwrap_or_else(|| panic!("node {node} has no {property:?}")) as u64
    }

    fn text(&self, node: u32) -> String {
        self.texts
            .get(&(node, PropertyKind::Text as u16))
            .cloned()
            .unwrap_or_default()
    }
}

fn send(
    host: &mut Host,
    mock: &mut MockRenderer,
    node_id: u32,
    handler_id: u64,
    payload: EventPayload<'_>,
) {
    let mut wire = Vec::new();
    encode_event(
        &HostEvent {
            node_id,
            handler_id,
            payload,
        },
        &mut wire,
    )
    .unwrap();
    let (batch, _) = host.dispatch_event(&wire).unwrap();
    let batch = batch.to_vec();
    mock.apply(&batch);
}

fn start(app: fn() -> Element) -> (Host, MockRenderer) {
    let mut host = Host::new(app);
    let mut mock = MockRenderer::default();
    let batch = host.rebuild().unwrap().to_vec();
    mock.apply(&batch);
    (host, mock)
}

fn wire_shape_app() -> Element {
    rsx! {
        Popover {
            open: true,
            placement: PopoverPlacement::Top,
            align: PopoverAlign::End,
            offset: 8.0,
            anchor: rsx! { Button { text: "Model" } },
            Text { text: "first" }
            Text { text: "second" }
        }
    }
}

#[test]
fn fr21_9_popover_wire_shape() {
    let (_host, mock) = start(wire_shape_app);
    let popover = mock.only(WidgetKind::Popover);
    let children = mock.children_of(popover);
    assert_eq!(children.len(), 3, "the anchor and two content children");
    assert_eq!(
        mock.widgets[&children[0]],
        WidgetKind::Button,
        "child 0 is the anchor"
    );
    assert_eq!(mock.text(children[1]), "first");
    assert_eq!(mock.text(children[2]), "second");
    assert_eq!(
        mock.ints.get(&(popover, PropertyKind::Placement as u16)),
        Some(&(PopoverPlacement::Top as i64)),
    );
    assert_eq!(
        mock.ints.get(&(popover, PropertyKind::Align as u16)),
        Some(&(PopoverAlign::End as i64)),
    );
    let offset = mock
        .floats
        .get(&(popover, PropertyKind::OffsetDp as u16))
        .copied()
        .expect("the popover carries no OffsetDp");
    assert!((offset - 8.0).abs() < 1e-6, "offset was {offset}");
    assert_eq!(
        mock.bools.get(&(popover, PropertyKind::Open as u16)),
        Some(&true)
    );
}

fn toggled_app() -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        Popover {
            open: open(),
            on_dismiss: move |()| open.set(false),
            anchor: rsx! { Button { text: "Model", on_click: move |_| open.set(true) } },
            if open() {
                Text { text: "content one" }
                Text { text: "content two" }
            }
        }
    }
}

#[test]
fn fr21_9_popover_content_stays_behind_anchor() {
    let (mut host, mut mock) = start(toggled_app);
    let popover = mock.only(WidgetKind::Popover);
    let anchor = mock.only(WidgetKind::Button);
    assert_eq!(mock.children_of(popover), vec![anchor]);
    let click = mock.handler(anchor, PropertyKind::OnClick);
    let dismiss = mock.handler(popover, PropertyKind::OnDismiss);

    for round in 0..4 {
        send(&mut host, &mut mock, anchor, click, EventPayload::Clicked);
        let children = mock.children_of(popover);
        assert_eq!(children.len(), 3, "round {round}: {children:?}");
        assert_eq!(
            children[0], anchor,
            "round {round}: the anchor must stay child 0"
        );
        assert_eq!(mock.text(children[1]), "content one", "round {round}");
        assert_eq!(mock.text(children[2]), "content two", "round {round}");
        assert_eq!(
            mock.bools.get(&(popover, PropertyKind::Open as u16)),
            Some(&true)
        );

        send(
            &mut host,
            &mut mock,
            popover,
            dismiss,
            EventPayload::Clicked,
        );
        assert_eq!(mock.children_of(popover), vec![anchor], "round {round}");
        assert_eq!(
            mock.bools.get(&(popover, PropertyKind::Open as u16)),
            Some(&false)
        );
    }
}

thread_local! {
    static DISMISSED: RefCell<u32> = const { RefCell::new(0) };
    static TYPED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static PICKED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

fn dismiss_app() -> Element {
    rsx! {
        Popover {
            open: true,
            on_dismiss: move |()| DISMISSED.with(|count| *count.borrow_mut() += 1),
            anchor: rsx! { Button { text: "Model" } },
            Text { text: "content" }
        }
    }
}

#[test]
fn fr21_9_dismiss_fires_once() {
    DISMISSED.with(|count| *count.borrow_mut() = 0);
    let (mut host, mut mock) = start(dismiss_app);
    let popover = mock.only(WidgetKind::Popover);
    let dismiss = mock.handler(popover, PropertyKind::OnDismiss);
    send(
        &mut host,
        &mut mock,
        popover,
        dismiss,
        EventPayload::Clicked,
    );
    assert_eq!(DISMISSED.with(|count| *count.borrow()), 1);
    // The application kept it open, so the box and its content are still there.
    assert_eq!(mock.children_of(popover).len(), 2);
}

fn arbitrary_app() -> Element {
    rsx! {
        Popover {
            open: true,
            anchor: rsx! { Button { text: "Model" } },
            Column {
                TextField {
                    on_value_change: move |text: String| TYPED.with(|typed| typed.borrow_mut().push(text)),
                }
                LazyColumn {
                    item_count: 100,
                    item: move |index: usize| rsx! {
                        Button {
                            text: "model {index}",
                            on_click: move |_| PICKED.with(|picked| picked.borrow_mut().push(index)),
                        }
                    },
                }
            }
        }
    }
}

#[test]
fn fr21_9_popover_hosts_arbitrary_children() {
    TYPED.with(|typed| typed.borrow_mut().clear());
    PICKED.with(|picked| picked.borrow_mut().clear());
    let (mut host, mut mock) = start(arbitrary_app);
    let popover = mock.only(WidgetKind::Popover);
    let children = mock.children_of(popover);
    assert_eq!(children.len(), 2, "the anchor and the content column");
    let column = children[1];
    assert_eq!(mock.widgets[&column], WidgetKind::Column);
    let field = mock.only(WidgetKind::TextField);
    let list = mock.only(WidgetKind::LazyColumn);
    assert_eq!(mock.children_of(column), vec![field, list]);

    let change = mock.handler(field, PropertyKind::OnValueChange);
    send(
        &mut host,
        &mut mock,
        field,
        change,
        EventPayload::TextChanged("gpt"),
    );
    assert_eq!(
        TYPED.with(|typed| typed.borrow().clone()),
        vec!["gpt".to_owned()]
    );

    let range = mock.handler(list, PropertyKind::OnRangeRequested);
    send(
        &mut host,
        &mut mock,
        list,
        range,
        EventPayload::RangeRequested {
            start: 10,
            count: 5,
        },
    );
    assert_eq!(
        mock.children_of(list).len(),
        5,
        "the window the renderer asked for"
    );
    let row = mock
        .widgets
        .iter()
        .find(|(node, kind)| **kind == WidgetKind::Button && mock.text(**node) == "model 12")
        .map(|(node, _)| *node)
        .expect("item 12 was built");
    let click = mock.handler(row, PropertyKind::OnClick);
    send(&mut host, &mut mock, row, click, EventPayload::Clicked);
    assert_eq!(PICKED.with(|picked| picked.borrow().clone()), vec![12]);
}
