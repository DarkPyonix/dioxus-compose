//! The to-do example laid out in a 400 by 600 viewport.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px, so
//! every number below follows from the stylesheet by hand:
//!
//! - The app has 16px of padding. The heading is one 20px line with 12px below it, and the
//!   new-item row is 32px tall with 12px below it, so the list starts at
//!   y = 16 + 20 + 12 + 32 + 12 = 92, 400 - 32 = 368px wide.
//! - Every row is 40px tall (border-box, its 1px bottom border included), so row n (from 0)
//!   is at y = 92 + 40n, x = 16.
//! - In a row: the 16px checkbox, the title, and three 64px buttons, 8px apart. The title
//!   takes what is left: 368 - 16 - 3 x 64 - 4 x 8 = 128px.

use dioxus_compose::html::{
    DisplayList, HtmlConfig, HtmlDom, InputKind, ModifierSlot, NodeEntry, NodeId, Plan, PlanChange,
    PlanKey, PlanModifier, Rect, TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint,
    diff, plan_from,
};
use dioxus_core::ScopeId;
use dioxus_signals::ReadableExt;
use sample_html_todo::{self as todo, Todos};

const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 600.0;
const ADVANCE: f32 = 10.0;
const LINE: f32 = 20.0;

/// Every character is `ADVANCE` wide and every line `LINE` tall; lines break greedily at
/// spaces.
struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let max_chars = match request.width {
            WidthConstraint::MaxContent => usize::MAX,
            WidthConstraint::MinContent => 0,
            WidthConstraint::AtMost(width) => (width / ADVANCE).floor() as usize,
        };
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in request.text.split(' ') {
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        lines.push(current);
        let width = lines
            .iter()
            .map(|line| line.chars().count() as f32 * ADVANCE)
            .fold(0.0, f32::max);
        TextMetrics {
            width,
            height: lines.len() as f32 * LINE,
            first_baseline: LINE * 0.75,
            line_count: lines.len() as u32,
        }
    }
}

fn open() -> HtmlDom {
    HtmlDom::with_config(
        todo::app,
        HtmlConfig {
            stylesheets: vec![todo::STYLE.to_string()],
            measurer: Some(Box::new(FixedAdvance)),
            ..HtmlConfig::default()
        },
    )
}

fn layout(dom: &mut HtmlDom) -> DisplayList {
    dom.layout(WIDTH, HEIGHT, 1.0).clone()
}

fn plan(dom: &mut HtmlDom) -> Plan {
    plan_from(dom.layout(WIDTH, HEIGHT, 1.0))
}

#[track_caller]
fn node(dom: &HtmlDom, id: &str) -> NodeId {
    dom.element_by_id(id)
        .unwrap_or_else(|| panic!("no element with id {id}"))
}

#[track_caller]
fn entry<'a>(dom: &HtmlDom, list: &'a DisplayList, id: &str) -> &'a NodeEntry {
    let node = node(dom, id);
    list.get(node)
        .unwrap_or_else(|| panic!("#{id} (node {node}) has no display list entry"))
}

#[track_caller]
fn assert_rect(actual: Rect, x: f32, y: f32, width: f32, height: f32) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(
        close(actual.x, x)
            && close(actual.y, y)
            && close(actual.width, width)
            && close(actual.height, height),
        "expected ({x}, {y}, {width}, {height}), got ({}, {}, {}, {})",
        actual.x,
        actual.y,
        actual.width,
        actual.height
    );
}

/// Clicks the middle of the element with `id` in the last layout and applies what the
/// handler changed.
#[track_caller]
fn click(dom: &mut HtmlDom, id: &str) -> NodeId {
    let rect = entry(dom, dom.display_list().expect("laid out"), id).rect;
    let target = dom
        .click(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
        .unwrap_or_else(|| panic!("nothing under #{id} handled the click"));
    dom.render();
    target
}

/// Types into the new-item field: the renderer commits the field's text, and the field's
/// `oninput` copies it into the draft.
#[track_caller]
fn type_draft(dom: &mut HtmlDom, text: &str) {
    let field = node(dom, "new-todo");
    assert_eq!(
        dom.input(field, text),
        Some(field),
        "the new-item field listens for input"
    );
    dom.render();
}

/// What the app holds as the draft.
fn draft_text(dom: &HtmlDom) -> String {
    let draft = dom
        .virtual_dom()
        .runtime()
        .consume_context::<Todos>(ScopeId::APP)
        .expect("the to-do app provides its list as context")
        .draft;
    dom.virtual_dom().in_runtime(|| draft.cloned())
}

/// The text drawn for the row's title.
fn title(dom: &HtmlDom, list: &DisplayList, row: &str) -> String {
    let row = node(dom, row);
    list.entries
        .iter()
        .filter(|entry| entry.parent == Some(row) && entry.tag == "span")
        .flat_map(|entry| entry.texts.iter())
        .map(|run| run.text.clone())
        .collect()
}

/// Whether `node` is `ancestor` or sits inside it, by the display list's parent links.
fn within(list: &DisplayList, node: NodeId, ancestor: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        current = list.get(id).and_then(|entry| entry.parent);
    }
    false
}

fn removed(changes: &[PlanChange]) -> Vec<PlanKey> {
    changes
        .iter()
        .filter_map(|change| match change {
            PlanChange::Removed { key } => Some(*key),
            _ => None,
        })
        .collect()
}

fn inserted(changes: &[PlanChange]) -> Vec<(PlanKey, usize, PlanKey)> {
    changes
        .iter()
        .filter_map(|change| match change {
            PlanChange::Inserted {
                parent,
                index,
                node,
            } => Some((*parent, *index, node.key)),
            _ => None,
        })
        .collect()
}

#[test]
fn fr34_todo_rows_are_laid_out_from_the_stylesheet() {
    let mut dom = open();
    let list = layout(&mut dom);

    assert_rect(entry(&dom, &list, "new-todo").rect, 16.0, 48.0, 296.0, 32.0);
    assert_rect(entry(&dom, &list, "add").rect, 320.0, 48.0, 64.0, 32.0);
    assert_rect(entry(&dom, &list, "todos").rect, 16.0, 92.0, 368.0, 120.0);
    for (index, id) in ["todo-1", "todo-2", "todo-3"].into_iter().enumerate() {
        assert_rect(
            entry(&dom, &list, id).rect,
            16.0,
            92.0 + 40.0 * index as f32,
            368.0,
            40.0,
        );
    }
    // The buttons follow the 128px title: 16 + 16 + 8 + 128 + 8 = 176, then every 72px.
    // Each is 28px tall, centred in the 39px above the row's bottom border.
    assert_rect(entry(&dom, &list, "up-1").rect, 176.0, 97.5, 64.0, 28.0);
    assert_rect(entry(&dom, &list, "down-1").rect, 248.0, 97.5, 64.0, 28.0);
    assert_rect(entry(&dom, &list, "delete-1").rect, 320.0, 97.5, 64.0, 28.0);
    assert_rect(entry(&dom, &list, "toggle-1").rect, 16.0, 103.5, 16.0, 16.0);
    assert_eq!(title(&dom, &list, "todo-2"), "Write tests");
}

#[test]
fn fr34_todo_add_inserts_one_row() {
    let mut dom = open();
    layout(&mut dom);
    type_draft(&mut dom, "Read a book");
    let typed = plan(&mut dom);

    click(&mut dom, "add");
    let after = plan(&mut dom);
    let changes = diff(&typed, &after);

    let todos = PlanKey::Node(node(&dom, "todos"));
    let row = PlanKey::Node(node(&dom, "todo-4"));
    assert_eq!(inserted(&changes), vec![(todos, 3, row)], "{changes:#?}");
    assert!(removed(&changes).is_empty(), "{changes:#?}");
    assert!(
        !changes
            .iter()
            .any(|change| matches!(change, PlanChange::Reordered { .. })),
        "{changes:#?}"
    );

    let list = dom.display_list().unwrap();
    assert_rect(entry(&dom, list, "todo-4").rect, 16.0, 212.0, 368.0, 40.0);
    assert_eq!(title(&dom, list, "todo-4"), "Read a book");
    assert_eq!(
        entry(&dom, list, "new-todo").input.as_ref().unwrap().value,
        "",
        "adding empties the field"
    );
}

#[test]
fn fr34_todo_delete_removes_its_row() {
    let mut dom = open();
    let before = plan(&mut dom);
    let todos = PlanKey::Node(node(&dom, "todos"));
    let second = PlanKey::Node(node(&dom, "todo-2"));
    let third = PlanKey::Node(node(&dom, "todo-3"));

    click(&mut dom, "delete-2");
    let after = plan(&mut dom);
    let changes = diff(&before, &after);

    assert_eq!(removed(&changes), vec![second], "{changes:#?}");
    assert!(inserted(&changes).is_empty(), "{changes:#?}");
    // The third row moves up into the second's place: 40px below the top of the list.
    assert!(
        changes.contains(&PlanChange::ModifierSet {
            key: third,
            slot: ModifierSlot::Offset,
            modifier: PlanModifier::Offset { x: 0.0, y: 40.0 },
        }),
        "{changes:#?}"
    );
    assert!(dom.element_by_id("todo-2").is_none());
    let rows: Vec<PlanKey> = after
        .find(todos)
        .expect("the list is in the plan")
        .children
        .iter()
        .map(|child| child.key)
        .collect();
    assert_eq!(rows, vec![PlanKey::Node(node(&dom, "todo-1")), third]);
}

/// A keyed reorder within one parent. blitz-dom 0.2.4 drops a node that is moved to
/// another place under the same parent unless it is detached first, so this is also the
/// check that every row survives the move.
#[test]
fn fr34_todo_move_up_reorders_rows() {
    let mut dom = open();
    let before = plan(&mut dom);
    let todos = PlanKey::Node(node(&dom, "todos"));
    let first = PlanKey::Node(node(&dom, "todo-1"));
    let second = PlanKey::Node(node(&dom, "todo-2"));
    let third = PlanKey::Node(node(&dom, "todo-3"));

    click(&mut dom, "up-2");
    let after = plan(&mut dom);
    let changes = diff(&before, &after);

    assert!(
        changes.contains(&PlanChange::Reordered {
            parent: todos,
            order: vec![second, first, third],
        }),
        "{changes:#?}"
    );
    assert!(
        removed(&changes).is_empty(),
        "a reorder keeps every row: {changes:#?}"
    );
    assert!(inserted(&changes).is_empty(), "{changes:#?}");

    let list = dom.display_list().unwrap();
    for id in ["todo-1", "todo-2", "todo-3"] {
        assert!(
            dom.element_by_id(id)
                .is_some_and(|row| list.get(row).is_some()),
            "#{id} is still in the document and drawn"
        );
    }
    assert_rect(entry(&dom, list, "todo-2").rect, 16.0, 92.0, 368.0, 40.0);
    assert_rect(entry(&dom, list, "todo-1").rect, 16.0, 132.0, 368.0, 40.0);
    assert_rect(entry(&dom, list, "todo-3").rect, 16.0, 172.0, 368.0, 40.0);
    assert_eq!(title(&dom, list, "todo-2"), "Write tests");
    assert_eq!(title(&dom, list, "todo-1"), "Buy milk");

    // And back down again: the original order.
    click(&mut dom, "down-2");
    let back = plan(&mut dom);
    let changes = diff(&after, &back);
    assert!(
        changes.contains(&PlanChange::Reordered {
            parent: todos,
            order: vec![first, second, third],
        }),
        "{changes:#?}"
    );
    assert!(removed(&changes).is_empty(), "{changes:#?}");
}

#[test]
fn fr34_todo_toggle_changes_only_its_row() {
    let mut dom = open();
    let list = layout(&mut dom);
    let row = node(&dom, "todo-1");
    assert_eq!(
        entry(&dom, &list, "toggle-1").input.as_ref().unwrap().kind,
        InputKind::Checkbox { checked: false }
    );

    click(&mut dom, "toggle-1");
    let diff = dom.layout_diff(WIDTH, HEIGHT, 1.0);

    assert!(!diff.changed.is_empty());
    let list = dom.display_list().unwrap();
    for changed in &diff.changed {
        assert!(
            within(list, changed.node, row),
            "node {} ({:?}) changed outside the toggled row",
            changed.node,
            changed.tag
        );
    }
    assert!(diff.removed.is_empty(), "{:?}", diff.removed);
    assert_eq!(diff.order, None);

    let toggled = entry(&dom, list, "toggle-1");
    assert_eq!(
        toggled.input.as_ref().unwrap().kind,
        InputKind::Checkbox { checked: true }
    );
    assert_rect(entry(&dom, list, "todo-1").rect, 16.0, 92.0, 368.0, 40.0);
}

/// Unticking an item clears the box again. Dioxus removes a `checked` attribute that turns
/// false, and the box must follow the attribute rather than keep the state it had.
#[test]
fn fr34_todo_toggling_twice_clears_the_checkbox() {
    let mut dom = open();
    layout(&mut dom);
    click(&mut dom, "toggle-1");
    layout(&mut dom);
    click(&mut dom, "toggle-1");
    let list = layout(&mut dom);

    assert_eq!(
        entry(&dom, &list, "toggle-1").input.as_ref().unwrap().kind,
        InputKind::Checkbox { checked: false }
    );
}

/// Text the renderer commits to the new-item field reaches its `oninput`, and pressing
/// Enter in the field adds the item as the Add button does. White space alone adds
/// nothing.
#[test]
fn fr34_todo_typing_and_enter_add_an_item() {
    let mut dom = open();
    layout(&mut dom);
    let field = node(&dom, "new-todo");
    let form = node(&dom, "new-todo-form");

    type_draft(&mut dom, "Water the plants");
    assert_eq!(draft_text(&dom), "Water the plants");
    assert_eq!(dom.submit(field), Some(form));
    dom.render();
    let list = layout(&mut dom);
    assert_eq!(title(&dom, &list, "todo-4"), "Water the plants");
    assert_rect(entry(&dom, &list, "todo-4").rect, 16.0, 212.0, 368.0, 40.0);
    assert_eq!(draft_text(&dom), "");
    assert_eq!(
        entry(&dom, &list, "new-todo").input.as_ref().unwrap().value,
        ""
    );

    type_draft(&mut dom, "   ");
    dom.submit(field);
    dom.render();
    layout(&mut dom);
    assert!(dom.element_by_id("todo-5").is_none());
}
