//! The kanban example laid out in a 600 by 600 viewport.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px, so
//! every number below follows from the stylesheet by hand:
//!
//! - The app has 16px of padding, so the content is 568px wide. The heading is one 20px
//!   line with 12px below it and the new-card row is 32px tall with 12px below it, so the
//!   board starts at y = 16 + 20 + 12 + 32 + 12 = 92.
//! - Three columns share the width after two 12px gaps: (568 - 24) / 3 = 181.33px each, at
//!   x = 16, 209.33 and 402.67.
//! - A column has 8px of padding, a 20px heading and 8px below it, so its first card is at
//!   y = 92 + 8 + 20 + 8 = 128, 8px in from the column's left edge and 165.33px wide.
//!   Cards are 76px tall with 8px between them, so card n (from 0) is at y = 128 + 84n.
//! - A card has a 1px border and 8px of padding. Its title is one 20px line and its buttons
//!   are 28px squares 4px apart, 8px below the title: y = card + 1 + 8 + 20 + 8 = card + 37,
//!   x = card + 9, then every 32px.

use dioxus_compose::html::{
    DisplayList, HtmlConfig, HtmlDom, NodeEntry, NodeId, Rect, TextMeasureRequest, TextMeasurer,
    TextMetrics, WidthConstraint,
};
use dioxus_core::ScopeId;
use dioxus_signals::ReadableExt;
use sample_html_kanban::{self as kanban, Board};

const WIDTH: f32 = 600.0;
const HEIGHT: f32 = 600.0;
const ADVANCE: f32 = 10.0;
const LINE: f32 = 20.0;

const COLUMN_WIDTH: f32 = 544.0 / 3.0;
const TODO_X: f32 = 16.0;
const DOING_X: f32 = 16.0 + COLUMN_WIDTH + 12.0;
const DONE_X: f32 = 16.0 + 2.0 * (COLUMN_WIDTH + 12.0);
const CARD_WIDTH: f32 = COLUMN_WIDTH - 16.0;

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
        kanban::app,
        HtmlConfig {
            stylesheets: vec![kanban::STYLE.to_string()],
            measurer: Some(Box::new(FixedAdvance)),
            ..HtmlConfig::default()
        },
    )
}

fn layout(dom: &mut HtmlDom) -> DisplayList {
    dom.layout(WIDTH, HEIGHT, 1.0).clone()
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

/// Clicks the middle of the element with `id` in a fresh layout and applies what the
/// handler changed. Returns whether anything under the point handled the click.
#[track_caller]
fn click(dom: &mut HtmlDom, id: &str) -> bool {
    let list = layout(dom);
    let rect = entry(dom, &list, id).rect;
    let handled = dom
        .click(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
        .is_some();
    dom.render();
    handled
}

/// Types into the new-card field: the renderer commits the field's text, and the field's
/// `oninput` copies it into the draft.
#[track_caller]
fn type_draft(dom: &mut HtmlDom, text: &str) {
    let field = node(dom, "new-card");
    assert_eq!(
        dom.input(field, text),
        Some(field),
        "the new-card field listens for input"
    );
    dom.render();
}

/// What the app holds as the draft.
fn draft_text(dom: &HtmlDom) -> String {
    let draft = dom
        .virtual_dom()
        .runtime()
        .consume_context::<Board>(ScopeId::APP)
        .expect("the board app provides its cards as context")
        .draft;
    dom.virtual_dom().in_runtime(|| draft.cloned())
}

/// The text drawn directly inside the element with `id`.
fn text_of(dom: &HtmlDom, list: &DisplayList, id: &str) -> String {
    entry(dom, list, id)
        .texts
        .iter()
        .map(|run| run.text.clone())
        .collect()
}

/// The text drawn for the card's title.
fn title(dom: &HtmlDom, list: &DisplayList, card: &str) -> String {
    let card = node(dom, card);
    list.entries
        .iter()
        .filter(|entry| entry.parent == Some(card) && entry.tag == "span")
        .flat_map(|entry| entry.texts.iter())
        .map(|run| run.text.clone())
        .collect()
}

#[test]
fn fr34_kanban_columns_share_the_width_and_cards_stack() {
    let mut dom = open();
    let list = layout(&mut dom);

    assert_rect(entry(&dom, &list, "new-card").rect, 16.0, 48.0, 496.0, 32.0);
    assert_rect(entry(&dom, &list, "add").rect, 520.0, 48.0, 64.0, 32.0);
    for (id, x) in [
        ("column-todo", TODO_X),
        ("column-doing", DOING_X),
        ("column-done", DONE_X),
    ] {
        let rect = entry(&dom, &list, id).rect;
        assert_rect(rect, x, 92.0, COLUMN_WIDTH, rect.height);
    }

    assert_rect(
        entry(&dom, &list, "card-1").rect,
        TODO_X + 8.0,
        128.0,
        CARD_WIDTH,
        76.0,
    );
    assert_rect(
        entry(&dom, &list, "card-2").rect,
        TODO_X + 8.0,
        212.0,
        CARD_WIDTH,
        76.0,
    );
    assert_rect(
        entry(&dom, &list, "card-3").rect,
        DOING_X + 8.0,
        128.0,
        CARD_WIDTH,
        76.0,
    );
    assert_rect(
        entry(&dom, &list, "card-4").rect,
        DONE_X + 8.0,
        128.0,
        CARD_WIDTH,
        76.0,
    );

    // The buttons of card 1: 28px squares 32px apart, 37px below the card's top.
    let card = TODO_X + 8.0;
    assert_rect(
        entry(&dom, &list, "left-1").rect,
        card + 9.0,
        165.0,
        28.0,
        28.0,
    );
    assert_rect(
        entry(&dom, &list, "right-1").rect,
        card + 41.0,
        165.0,
        28.0,
        28.0,
    );
    assert_rect(
        entry(&dom, &list, "delete-1").rect,
        card + 73.0,
        165.0,
        28.0,
        28.0,
    );

    assert_eq!(title(&dom, &list, "card-2"), "Fix layout");
    assert_eq!(text_of(&dom, &list, "count-todo"), "2");
    assert_eq!(text_of(&dom, &list, "count-doing"), "1");
    assert_eq!(text_of(&dom, &list, "count-done"), "1");
}

#[test]
fn fr34_kanban_move_right_appends_to_the_next_column() {
    let mut dom = open();
    layout(&mut dom);

    assert!(click(&mut dom, "right-1"));
    let list = layout(&mut dom);

    // Card 3 was alone in the middle column, so card 1 lands under it.
    assert_rect(
        entry(&dom, &list, "card-1").rect,
        DOING_X + 8.0,
        212.0,
        CARD_WIDTH,
        76.0,
    );
    assert_rect(
        entry(&dom, &list, "card-3").rect,
        DOING_X + 8.0,
        128.0,
        CARD_WIDTH,
        76.0,
    );
    // Card 2 moves up to the top of the first column.
    assert_rect(
        entry(&dom, &list, "card-2").rect,
        TODO_X + 8.0,
        128.0,
        CARD_WIDTH,
        76.0,
    );
    assert_eq!(title(&dom, &list, "card-1"), "Write docs");
    assert_eq!(text_of(&dom, &list, "count-todo"), "1");
    assert_eq!(text_of(&dom, &list, "count-doing"), "2");

    // And back: the card returns to the end of the first column.
    assert!(click(&mut dom, "left-1"));
    let list = layout(&mut dom);
    assert_rect(
        entry(&dom, &list, "card-1").rect,
        TODO_X + 8.0,
        212.0,
        CARD_WIDTH,
        76.0,
    );
    assert_eq!(text_of(&dom, &list, "count-doing"), "1");
}

#[test]
fn fr34_kanban_a_card_walks_across_all_three_columns() {
    let mut dom = open();
    layout(&mut dom);

    click(&mut dom, "right-2");
    click(&mut dom, "right-2");
    let list = layout(&mut dom);

    assert_rect(
        entry(&dom, &list, "card-2").rect,
        DONE_X + 8.0,
        212.0,
        CARD_WIDTH,
        76.0,
    );
    assert_eq!(text_of(&dom, &list, "count-todo"), "1");
    assert_eq!(text_of(&dom, &list, "count-done"), "2");
}

/// The first column has nowhere to the left and the last nowhere to the right, so those
/// buttons are disabled and a click on them changes nothing.
#[test]
fn fr34_kanban_edge_buttons_do_not_move_cards() {
    let mut dom = open();
    let before = layout(&mut dom);

    click(&mut dom, "left-1");
    click(&mut dom, "right-4");
    let after = layout(&mut dom);

    for id in ["card-1", "card-2", "card-3", "card-4"] {
        let was = entry(&dom, &before, id).rect;
        assert_rect(
            entry(&dom, &after, id).rect,
            was.x,
            was.y,
            was.width,
            was.height,
        );
    }
    assert_eq!(text_of(&dom, &after, "count-todo"), "2");
    assert_eq!(text_of(&dom, &after, "count-done"), "1");
}

#[test]
fn fr34_kanban_delete_removes_the_card_and_closes_the_gap() {
    let mut dom = open();
    layout(&mut dom);

    assert!(click(&mut dom, "delete-1"));
    let list = layout(&mut dom);

    assert!(dom.element_by_id("card-1").is_none());
    assert_rect(
        entry(&dom, &list, "card-2").rect,
        TODO_X + 8.0,
        128.0,
        CARD_WIDTH,
        76.0,
    );
    assert_eq!(text_of(&dom, &list, "count-todo"), "1");
}

#[test]
fn fr34_kanban_an_emptied_column_says_so() {
    let mut dom = open();
    layout(&mut dom);

    click(&mut dom, "delete-4");
    let list = layout(&mut dom);

    assert_eq!(text_of(&dom, &list, "count-done"), "0");
    assert!(
        list.entries
            .iter()
            .any(|entry| entry.tag == "p" && entry.texts.iter().any(|run| run.text == "No cards")),
        "the empty column shows its placeholder"
    );
}

/// Text the renderer commits to the new-card field reaches its `oninput`, and pressing
/// Enter adds the card to the first column. White space alone adds nothing.
#[test]
fn fr34_kanban_typing_and_enter_add_a_card_to_the_first_column() {
    let mut dom = open();
    layout(&mut dom);
    let field = node(&dom, "new-card");
    let form = node(&dom, "new-card-form");

    type_draft(&mut dom, "Ship it");
    assert_eq!(draft_text(&dom), "Ship it");
    assert_eq!(dom.submit(field), Some(form));
    dom.render();
    let list = layout(&mut dom);

    assert_eq!(title(&dom, &list, "card-5"), "Ship it");
    assert_rect(
        entry(&dom, &list, "card-5").rect,
        TODO_X + 8.0,
        296.0,
        CARD_WIDTH,
        76.0,
    );
    assert_eq!(text_of(&dom, &list, "count-todo"), "3");
    assert_eq!(draft_text(&dom), "");
    assert_eq!(
        entry(&dom, &list, "new-card").input.as_ref().unwrap().value,
        ""
    );

    type_draft(&mut dom, "   ");
    dom.submit(field);
    dom.render();
    layout(&mut dom);
    assert!(dom.element_by_id("card-6").is_none());
}
