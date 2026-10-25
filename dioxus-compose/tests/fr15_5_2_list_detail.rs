//! `ListDetail`: a list and a detail on one `SplitPane`, through the public surface and the
//! mock renderer.

use std::cell::{Cell, RefCell};

use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::Host;
use dioxus_compose::schema::{EventPayload, Modifier, PropertyKind, WidgetKind};

thread_local! {
    static SELECTED: RefCell<Option<u32>> = const { RefCell::new(Some(7)) };
    static CHANGES: RefCell<Vec<Option<u32>>> = const { RefCell::new(Vec::new()) };
    static COLLAPSE: Cell<Option<f32>> = const { Cell::new(None) };
}

fn app() -> Element {
    let mut selected = use_signal(|| SELECTED.with(|selected| *selected.borrow()));
    rsx! {
        ListDetail::<u32> {
            selected: selected(),
            collapse_below: COLLAPSE.with(Cell::get),
            on_select_change: move |next: Option<u32>| {
                CHANGES.with(|changes| changes.borrow_mut().push(next));
                selected.set(next);
            },
            list: rsx! {
                Column {
                    Text { text: "the list" }
                    Button { text: "Open", on_click: move |_| selected.set(Some(3)) }
                }
            },
            detail: rsx! { Text { text: "the detail" } },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Record {
    Create(u32, WidgetKind),
    Int(u32, PropertyKind, i64),
    Text(u32, String),
    Insert(u32, u32),
    Observe(u32),
}

fn records(batch: &[u8]) -> Vec<Record> {
    decode_batch(batch)
        .unwrap()
        .into_iter()
        .filter_map(|mutation| match mutation {
            Mutation::Create { node_id, widget } => Some(Record::Create(node_id, widget)),
            Mutation::SetProp {
                node_id,
                property,
                value: PropertyValue::Integer(number),
            } => Some(Record::Int(node_id, property, number)),
            Mutation::SetProp {
                node_id,
                value: PropertyValue::String(text),
                ..
            } => Some(Record::Text(node_id, text.to_string())),
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::ObserveSize { .. },
                ..
            } => Some(Record::Observe(node_id)),
            Mutation::Insert {
                parent_id, node_id, ..
            } => Some(Record::Insert(parent_id, node_id)),
            _ => None,
        })
        .collect()
}

fn node_of(records: &[Record], widget: WidgetKind) -> Option<u32> {
    records.iter().find_map(|record| match record {
        Record::Create(node, kind) if *kind == widget => Some(*node),
        _ => None,
    })
}

fn int(records: &[Record], node: u32, property: PropertyKind) -> Option<i64> {
    records.iter().rev().find_map(|record| match record {
        Record::Int(id, kind, value) if *id == node && *kind == property => Some(*value),
        _ => None,
    })
}

fn has_text(records: &[Record], wanted: &str) -> bool {
    records
        .iter()
        .any(|record| matches!(record, Record::Text(_, text) if text == wanted))
}

fn start(selected: Option<u32>, collapse: Option<f32>) -> Host {
    dioxus_compose::window::reset_window_size();
    SELECTED.with(|slot| *slot.borrow_mut() = selected);
    CHANGES.with(|changes| changes.borrow_mut().clear());
    COLLAPSE.with(|slot| slot.set(collapse));
    Host::new(app)
}

fn dispatch(host: &mut Host, node_id: u32, handler_id: u64, payload: EventPayload<'static>) -> Vec<Record> {
    let (batch, _) = host
        .dispatch(HostEvent {
            node_id,
            handler_id,
            payload,
        })
        .unwrap();
    records(batch)
}

#[test]
fn fr15_5_2_selected_maps_to_pane_index() {
    for (selected, index) in [(Some(7), 1), (None, 0)] {
        let mut host = start(selected, None);
        let first = records(host.rebuild().unwrap());
        let pane = node_of(&first, WidgetKind::SplitPane).expect("a split pane");
        assert_eq!(int(&first, pane, PropertyKind::SelectedIndex), Some(index));
    }
}

#[test]
fn fr15_5_2_back_clears_selection_once() {
    let mut host = start(Some(7), None);
    let first = records(host.rebuild().unwrap());
    let pane = node_of(&first, WidgetKind::SplitPane).unwrap();
    let handler = int(&first, pane, PropertyKind::OnDismiss).expect("a dismiss handler") as u64;
    let after = dispatch(&mut host, pane, handler, EventPayload::Clicked);
    assert_eq!(CHANGES.with(|changes| changes.borrow().clone()), [None]);
    assert_eq!(int(&after, pane, PropertyKind::SelectedIndex), Some(0));
}

#[test]
fn fr15_5_2_selection_keeps_list_nodes() {
    let mut host = start(None, None);
    let first = records(host.rebuild().unwrap());
    let pane = node_of(&first, WidgetKind::SplitPane).unwrap();
    let panes: Vec<u32> = first
        .iter()
        .filter_map(|record| match record {
            Record::Insert(parent, node) if *parent == pane => Some(*node),
            _ => None,
        })
        .collect();
    assert_eq!(panes.len(), 2, "the list and the detail are both children");
    let open = node_of(&first, WidgetKind::Button).unwrap();
    let handler = int(&first, open, PropertyKind::OnClick).unwrap() as u64;
    let mut after = dispatch(&mut host, open, handler, EventPayload::Clicked);
    after.extend(records(host.render_frame(0).unwrap()));
    assert_eq!(int(&after, pane, PropertyKind::SelectedIndex), Some(1));
    assert!(
        !after.iter().any(|record| matches!(record, Record::Create(..))),
        "a selection rebuilt a node: {after:?}"
    );
}

fn report_width(host: &mut Host, node_id: u32, width_dp: f32) -> Vec<Record> {
    dispatch(
        host,
        node_id,
        0,
        EventPayload::WindowSizeChanged {
            width_dp,
            height_dp: 800.0,
            class: WindowSizeClass::from_width_dp(width_dp),
            height_class: WindowHeightClass::from_height_dp(800.0),
        },
    )
}

#[test]
fn fr15_5_2_collapse_below_overrides_design_rule() {
    // Not collapsed without a threshold: a narrow report leaves the split pane alone.
    let mut host = start(Some(7), None);
    let first = records(host.rebuild().unwrap());
    assert!(node_of(&first, WidgetKind::SplitPane).is_some());

    let mut host = start(Some(7), Some(600.0));
    let first = records(host.rebuild().unwrap());
    let pane = node_of(&first, WidgetKind::SplitPane).expect("two panes before any report");
    let observer = first
        .iter()
        .find_map(|record| match record {
            Record::Observe(node) => Some(*node),
            _ => None,
        })
        .expect("the composition measures itself when it has a threshold");
    assert_ne!(observer, pane);

    // Narrower than the threshold: only the selected pane is drawn.
    let narrow = report_width(&mut host, observer, 400.0);
    assert!(has_text(&narrow, "the detail"), "{narrow:?}");

    // Wider again: both panes come back on the split pane.
    let wide = report_width(&mut host, observer, 900.0);
    assert!(node_of(&wide, WidgetKind::SplitPane).is_some(), "{wide:?}");
}
