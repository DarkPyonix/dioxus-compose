//! `Table`: a header and keyed rows over a `LazyColumn`, through the public surface and
//! the mock renderer.

use std::cell::Cell;
use std::collections::HashMap;

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, Modifier, PropertyKind, WidgetKind};

thread_local! {
    static ROWS: Cell<usize> = const { Cell::new(30) };
}

#[derive(Clone, PartialEq)]
struct Line {
    id: String,
    name: String,
}

fn lines(prefix: &str, count: usize) -> Vec<Line> {
    (0..count)
        .map(|index| Line {
            id: format!("{prefix}{index}"),
            name: format!("{prefix} name {index}"),
        })
        .collect()
}

fn app() -> Element {
    let mut data = use_signal(|| lines("a", ROWS.with(Cell::get)));
    let mut source = use_signal(|| 0_u64);
    let rows = data();
    let keys = rows.clone();
    rsx! {
        Column {
            Button { text: "bump", on_click: move |_| data.write()[0].name = "changed".into() }
            Button { text: "reverse", on_click: move |_| data.write().reverse() }
            Button {
                text: "replace",
                on_click: move |_| {
                    data.set(lines("b", 30));
                    source += 1;
                },
            }
            Table {
                columns: vec![
                    ColumnSpec::content("name"),
                    ColumnSpec::weight("status", 1.0),
                    ColumnSpec::fixed("score", 72.0),
                ],
                row_count: rows.len(),
                source: source(),
                row_key: move |i: usize| keys[i].id.clone(),
                cell: move |(row, col): (usize, usize)| {
                    let text = match col {
                        0 => rows[row].name.clone(),
                        1 => "ok".to_string(),
                        _ => format!("{row}"),
                    };
                    rsx! { Text { text } }
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Record {
    Create(u32, WidgetKind),
    Int(u32, PropertyKind, i64),
    Text(u32, String),
    Insert(u32, u32, u32),
    Move(u32, u32, u32),
    Width(u32, f32),
    Weight(u32, f32),
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
            Mutation::SetText { node_id, text, .. } => {
                Some(Record::Text(node_id, text.to_string()))
            }
            Mutation::SetModifier {
                node_id, modifier, ..
            } => match modifier {
                Modifier::ObserveSize { .. } => Some(Record::Observe(node_id)),
                Modifier::Width(dp) => Some(Record::Width(node_id, dp)),
                Modifier::Weight(share) => Some(Record::Weight(node_id, share)),
                _ => None,
            },
            Mutation::Insert {
                parent_id,
                node_id,
                index,
            } => Some(Record::Insert(parent_id, node_id, index)),
            Mutation::Move {
                parent_id,
                node_id,
                index,
            } => Some(Record::Move(parent_id, node_id, index)),
            _ => None,
        })
        .collect()
}

/// What the screen holds so far: each node's kind, parent, children in order, latest
/// text and latest width or weight.
#[derive(Default)]
struct Tree {
    kinds: HashMap<u32, WidgetKind>,
    parent: HashMap<u32, u32>,
    children: HashMap<u32, Vec<u32>>,
    text: HashMap<u32, String>,
    width: HashMap<u32, f32>,
    weight: HashMap<u32, f32>,
    observed: Vec<u32>,
}

impl Tree {
    fn apply(&mut self, records: &[Record]) {
        for record in records {
            match record {
                Record::Create(node, kind) => {
                    self.kinds.insert(*node, *kind);
                }
                Record::Insert(parent, node, index) | Record::Move(parent, node, index) => {
                    if let Some(old) = self.parent.insert(*node, *parent) {
                        self.children.entry(old).or_default().retain(|n| n != node);
                    }
                    let children = self.children.entry(*parent).or_default();
                    let at = (*index as usize).min(children.len());
                    children.insert(at, *node);
                }
                Record::Text(node, text) => {
                    self.text.insert(*node, text.clone());
                }
                Record::Width(node, dp) => {
                    self.width.insert(*node, *dp);
                }
                Record::Weight(node, share) => {
                    self.weight.insert(*node, *share);
                }
                Record::Observe(node) => self.observed.push(*node),
                Record::Int(..) => {}
            }
        }
    }

    fn of_kind(&self, kind: WidgetKind) -> Vec<u32> {
        let mut nodes: Vec<u32> = self
            .kinds
            .iter()
            .filter(|(_, k)| **k == kind)
            .map(|(n, _)| *n)
            .collect();
        nodes.sort();
        nodes
    }

    fn node_with_text(&self, wanted: &str) -> Option<u32> {
        self.text
            .iter()
            .find(|(_, text)| text.as_str() == wanted)
            .map(|(node, _)| *node)
    }

    fn ancestor_of_kind(&self, mut node: u32, kind: WidgetKind) -> Option<u32> {
        while let Some(parent) = self.parent.get(&node) {
            if self.kinds.get(parent) == Some(&kind) {
                return Some(*parent);
            }
            node = *parent;
        }
        None
    }

    /// The row holding the text `wanted`.
    fn row_of(&self, wanted: &str) -> u32 {
        let text = self
            .node_with_text(wanted)
            .unwrap_or_else(|| panic!("no node says {wanted:?}"));
        self.ancestor_of_kind(text, WidgetKind::Row).unwrap()
    }

    /// The width or weight the cell at `index` of `row` carries.
    fn cell_width(&self, row: u32, index: usize) -> (Option<f32>, Option<f32>) {
        let cell = self.children[&row][index];
        (
            self.width.get(&cell).copied(),
            self.weight.get(&cell).copied(),
        )
    }
}

fn int(records: &[Record], node: u32, property: PropertyKind) -> Option<i64> {
    records.iter().rev().find_map(|record| match record {
        Record::Int(id, kind, value) if *id == node && *kind == property => Some(*value),
        _ => None,
    })
}

struct Screen {
    host: Host,
    tree: Tree,
    first: Vec<Record>,
}

fn start(rows: usize) -> Screen {
    dioxus_compose::window::reset_window_size();
    ROWS.with(|slot| slot.set(rows));
    let mut host = Host::new(app);
    let first = records(host.rebuild().unwrap());
    let mut tree = Tree::default();
    tree.apply(&first);
    Screen { host, tree, first }
}

impl Screen {
    fn dispatch(
        &mut self,
        node_id: u32,
        handler_id: u64,
        payload: EventPayload<'static>,
    ) -> Vec<Record> {
        let (batch, _) = self
            .host
            .dispatch(HostEvent {
                node_id,
                handler_id,
                payload,
            })
            .unwrap();
        let mut out = records(batch);
        out.extend(records(self.host.render_frame(0).unwrap()));
        self.tree.apply(&out);
        out
    }

    fn list(&self) -> u32 {
        self.tree.of_kind(WidgetKind::LazyColumn)[0]
    }

    fn window(&mut self, start: u32, count: u32) -> Vec<Record> {
        let list = self.list();
        let handler = int(&self.first, list, PropertyKind::OnRangeRequested).unwrap() as u64;
        self.dispatch(list, handler, EventPayload::RangeRequested { start, count })
    }

    fn press(&mut self, label: &str) -> Vec<Record> {
        let button = self
            .tree
            .of_kind(WidgetKind::Button)
            .into_iter()
            .find(|node| self.tree.text.get(node).map(String::as_str) == Some(label))
            .unwrap();
        let handler = int(&self.first, button, PropertyKind::OnClick).unwrap() as u64;
        self.dispatch(button, handler, EventPayload::Clicked)
    }

    /// The measured node inside the cell holding `wanted`.
    fn observed_holding(&self, wanted: &str) -> u32 {
        let mut node = self.tree.node_with_text(wanted).unwrap();
        loop {
            if self.tree.observed.contains(&node) {
                return node;
            }
            node = self.tree.parent[&node];
        }
    }

    fn report_width(&mut self, node_id: u32, width_dp: f32) -> Vec<Record> {
        self.dispatch(
            node_id,
            0,
            EventPayload::WindowSizeChanged {
                width_dp,
                height_dp: 20.0,
                class: WindowSizeClass::from_width_dp(width_dp),
                height_class: WindowHeightClass::from_height_dp(20.0),
                safe_area_leading: 0.0,
                safe_area_top: 0.0,
                safe_area_trailing: 0.0,
            },
        )
    }
}

#[test]
fn fr15_5_3_cells_share_column_widths() {
    let mut screen = start(30);
    screen.window(0, 5);
    let header = screen.tree.row_of("name");
    let row = screen.tree.row_of("a name 3");
    // Measure the content column so it has a width to share.
    let cell = screen.observed_holding("a name 3");
    screen.report_width(cell, 80.0);
    for index in 0..3 {
        assert_eq!(
            screen.tree.cell_width(header, index),
            screen.tree.cell_width(row, index),
            "column {index}"
        );
    }
    assert_eq!(screen.tree.cell_width(row, 0), (Some(80.0), None));
    assert_eq!(screen.tree.cell_width(row, 1), (None, Some(1.0)));
    assert_eq!(screen.tree.cell_width(row, 2), (Some(72.0), None));
}

#[test]
fn fr15_5_3_only_window_rows_exist() {
    let mut screen = start(1000);
    screen.window(0, 10);
    let rows = screen.tree.of_kind(WidgetKind::Row);
    assert_eq!(rows.len(), 11, "the header and the ten rows asked for");
    assert!(screen.tree.node_with_text("a name 9").is_some());
    assert!(screen.tree.node_with_text("a name 10").is_none());
    assert!(screen.tree.node_with_text("a name 999").is_none());
}

#[test]
fn fr15_5_3_row_update_is_one_record() {
    let mut screen = start(30);
    screen.window(0, 5);
    let after = screen.press("bump");
    let texts: Vec<&Record> = after
        .iter()
        .filter(|record| matches!(record, Record::Text(..)))
        .collect();
    assert_eq!(texts.len(), 1, "{after:?}");
    assert!(matches!(texts[0], Record::Text(_, text) if text == "changed"));
    assert!(
        !after
            .iter()
            .any(|record| matches!(record, Record::Create(..) | Record::Insert(..))),
        "{after:?}"
    );
}

#[test]
fn fr15_5_3_reorder_moves_rows_by_key() {
    let mut screen = start(4);
    screen.window(0, 4);
    let before = screen.tree.row_of("a name 0");
    let after = screen.press("reverse");
    assert!(
        after
            .iter()
            .any(|record| matches!(record, Record::Move(..))),
        "{after:?}"
    );
    assert!(
        !after
            .iter()
            .any(|record| matches!(record, Record::Create(..))),
        "{after:?}"
    );
    assert_eq!(screen.tree.row_of("a name 0"), before);
    let list = screen.list();
    let order = &screen.tree.children[&list];
    let last = *order.last().unwrap();
    assert_eq!(
        screen.tree.ancestor_of_kind(before, WidgetKind::LazyColumn),
        Some(list)
    );
    let mut node = before;
    while screen.tree.parent[&node] != list {
        node = screen.tree.parent[&node];
    }
    assert_eq!(node, last, "the first row moved to the end");
}

#[test]
fn fr15_5_3_header_is_outside_the_scrolling_list() {
    let mut screen = start(30);
    screen.window(0, 5);
    let header = screen.tree.row_of("name");
    let list = screen.list();
    assert_eq!(
        screen.tree.ancestor_of_kind(header, WidgetKind::LazyColumn),
        None
    );
    assert_eq!(screen.tree.parent[&header], screen.tree.parent[&list]);
}

#[test]
fn fr15_5_3_content_width_fits_widest_drawn_cell() {
    let mut screen = start(30);
    screen.window(0, 3);
    let header = screen.observed_holding("name");
    screen.report_width(header, 30.0);
    for (index, width) in [(0, 40.0), (1, 90.0), (2, 60.0)] {
        let cell = screen.observed_holding(&format!("a name {index}"));
        screen.report_width(cell, width);
    }
    for index in 0..3 {
        let row = screen.tree.row_of(&format!("a name {index}"));
        assert_eq!(screen.tree.cell_width(row, 0).0, Some(90.0), "row {index}");
    }
    let header = screen.tree.row_of("name");
    assert_eq!(screen.tree.cell_width(header, 0).0, Some(90.0));
}

#[test]
fn fr15_5_3_content_width_keeps_largest_seen() {
    let mut screen = start(30);
    screen.window(0, 3);
    let wide = screen.observed_holding("a name 1");
    screen.report_width(wide, 90.0);
    screen.window(10, 3);
    assert!(screen.tree.node_with_text("a name 10").is_some());
    let cell = screen.observed_holding("a name 10");
    screen.report_width(cell, 40.0);
    let row = screen.tree.row_of("a name 10");
    assert_eq!(screen.tree.cell_width(row, 0).0, Some(90.0));
    let header = screen.tree.row_of("name");
    assert_eq!(screen.tree.cell_width(header, 0).0, Some(90.0));
}

#[test]
fn fr15_5_3_content_width_recomputes_on_data_replace() {
    let mut screen = start(30);
    screen.window(0, 3);
    let wide = screen.observed_holding("a name 1");
    screen.report_width(wide, 90.0);
    screen.press("replace");
    let cell = screen.observed_holding("b name 0");
    screen.report_width(cell, 50.0);
    let row = screen.tree.row_of("b name 0");
    assert_eq!(screen.tree.cell_width(row, 0).0, Some(50.0));
    let header = screen.tree.row_of("name");
    assert_eq!(screen.tree.cell_width(header, 0).0, Some(50.0));
}
