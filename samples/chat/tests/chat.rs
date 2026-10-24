//! The chat example laid out in a 400 by 600 viewport.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px, so
//! every number below follows from the stylesheet by hand:
//!
//! - The composer is 1px of top border, 12px of padding, a 36px field and 12px of padding:
//!   61px tall, so it starts at y = 600 - 61 = 539, and the message list fills the 539px
//!   above it. The send button is 72px wide at x = 400 - 12 - 72 = 316, y = 539 + 1 + 12.
//! - A message is one line of text (20px) with 8px of padding above and below: 36px tall,
//!   and 10px per character plus 24px of side padding wide. Messages are 8px apart inside
//!   12px of padding, so the nth (from 0) starts at y = 12 + 44n.
//! - The assistant's messages sit at the left padding edge, x = 12; the user's end at the
//!   right one, x = 400 - 12 = 388.

use std::collections::HashSet;

use dioxus_compose::html::{
    DisplayList, HtmlConfig, HtmlDom, NodeEntry, NodeId, Rect, TextMeasureRequest, TextMeasurer,
    TextMetrics, WidthConstraint,
};
use dioxus_core::ScopeId;
use dioxus_signals::ReadableExt;
use sample_html_chat::{self as chat, Chat};

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
        chat::app,
        HtmlConfig {
            stylesheets: vec![chat::STYLE.to_string()],
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

/// The conversation the app provides to its components.
fn conversation(dom: &HtmlDom) -> Chat {
    dom.virtual_dom()
        .runtime()
        .consume_context::<Chat>(ScopeId::APP)
        .expect("the chat app provides its conversation as context")
}

/// Types into the composer: the renderer commits the field's text, and the field's
/// `oninput` copies it into the draft.
#[track_caller]
fn type_draft(dom: &mut HtmlDom, text: &str) {
    let field = node(dom, "draft");
    assert_eq!(
        dom.input(field, text),
        Some(field),
        "the composer listens for input"
    );
    dom.render();
}

/// What the app holds as the draft.
fn draft_text(dom: &HtmlDom) -> String {
    let draft = conversation(dom).draft;
    dom.virtual_dom().in_runtime(|| draft.cloned())
}

/// A piece of the assistant's reply arriving.
fn receive(dom: &mut HtmlDom, chunk: &str) {
    let chat = conversation(dom);
    dom.virtual_dom().in_runtime(|| chat.receive(chunk));
    dom.render();
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

/// Types `text` and presses Send.
fn send(dom: &mut HtmlDom, text: &str) {
    type_draft(dom, text);
    layout(dom);
    click(dom, "send");
}

fn content_height(dom: &HtmlDom, list: &DisplayList) -> f32 {
    entry(dom, list, "messages")
        .scroll
        .expect("the message list scrolls")
        .content_height
}

#[test]
fn fr34_chat_message_list_scrolls_above_a_composer_at_the_bottom() {
    let mut dom = open();
    let list = layout(&mut dom);

    let messages = entry(&dom, &list, "messages");
    assert_rect(messages.rect, 0.0, 0.0, 400.0, 539.0);
    let scroll = messages
        .scroll
        .expect("overflow-y: auto makes the list a scroll container");
    assert!(scroll.vertical);
    assert!(
        !scroll.horizontal,
        "overflow-x: hidden clips without scrolling"
    );
    assert_rect(scroll.viewport, 0.0, 0.0, 400.0, 539.0);

    assert_rect(entry(&dom, &list, "composer").rect, 0.0, 539.0, 400.0, 61.0);
    assert_rect(entry(&dom, &list, "send").rect, 316.0, 552.0, 72.0, 36.0);

    // "Hello! How can I help?" is 22 characters: 220 + 24 = 244 wide, at the left.
    assert_rect(
        entry(&dom, &list, "message-1").rect,
        12.0,
        12.0,
        244.0,
        36.0,
    );
    // "Plan a trip" is 11 characters: 134 wide, ending at 388.
    assert_rect(
        entry(&dom, &list, "message-2").rect,
        254.0,
        56.0,
        134.0,
        36.0,
    );
    // "Sure": 64 wide, at the left.
    assert_rect(
        entry(&dom, &list, "message-3").rect,
        12.0,
        100.0,
        64.0,
        36.0,
    );
    // Every message moves with the list's scroll position.
    let list_node = node(&dom, "messages");
    for id in ["message-1", "message-2", "message-3"] {
        assert_eq!(
            entry(&dom, &list, id).scroll_parent,
            Some(list_node),
            "#{id}"
        );
    }

    let before = content_height(&dom, &list);
    assert!(
        before >= 136.0,
        "the content reaches at least the bottom of the third message (100 + 36): {before}"
    );

    // One more message: "Hello" is 74 wide, so it starts at 388 - 74 = 314, one step of
    // 44 below the third, and the content is 44 taller.
    send(&mut dom, "Hello");
    let list = layout(&mut dom);
    assert_rect(
        entry(&dom, &list, "message-4").rect,
        314.0,
        144.0,
        74.0,
        36.0,
    );
    assert_eq!(content_height(&dom, &list), before + 44.0);

    // Enough messages to overflow the list: the composer does not move, the list keeps its
    // size, and its content outgrows it.
    for index in 0..12 {
        send(&mut dom, &format!("Message {index}"));
    }
    let list = layout(&mut dom);
    assert_eq!(
        content_height(&dom, &list),
        before + 13.0 * 44.0,
        "every message added one step"
    );
    assert!(content_height(&dom, &list) > 539.0);
    assert_rect(entry(&dom, &list, "messages").rect, 0.0, 0.0, 400.0, 539.0);
    assert_rect(entry(&dom, &list, "composer").rect, 0.0, 539.0, 400.0, 61.0);
    assert_rect(entry(&dom, &list, "send").rect, 316.0, 552.0, 72.0, 36.0);
}

#[test]
fn fr34_chat_send_appends_exactly_one_entry() {
    let mut dom = open();
    layout(&mut dom);
    type_draft(&mut dom, "Hello");
    let typed = layout(&mut dom);
    assert_eq!(
        entry(&dom, &typed, "draft")
            .input
            .as_ref()
            .expect("the composer is a text field")
            .value,
        "Hello"
    );
    let before: HashSet<NodeId> = typed.order().into_iter().collect();

    click(&mut dom, "send");
    let diff = dom.layout_diff(WIDTH, HEIGHT, 1.0);

    let added: Vec<&NodeEntry> = diff
        .changed
        .iter()
        .filter(|entry| !before.contains(&entry.node))
        .collect();
    assert_eq!(
        added.len(),
        1,
        "one new entry: {:?}",
        added
            .iter()
            .map(|entry| (entry.node, &entry.tag))
            .collect::<Vec<_>>()
    );
    let message = added[0];
    assert_eq!(message.node, node(&dom, "message-4"));
    assert_eq!(message.tag, "div");
    assert_eq!(message.texts.len(), 1);
    assert_eq!(message.texts[0].text, "Hello");
    assert!(diff.removed.is_empty(), "{:?}", diff.removed);

    // Sending empties the composer.
    let list = dom.display_list().unwrap();
    assert_eq!(entry(&dom, list, "draft").input.as_ref().unwrap().value, "");

    // An empty composer sends nothing.
    let diff = {
        click(&mut dom, "send");
        dom.layout_diff(WIDTH, HEIGHT, 1.0)
    };
    assert!(diff.is_empty(), "{diff:?}");
}

#[test]
fn fr34_chat_growing_reply_changes_only_that_message() {
    let mut dom = open();
    layout(&mut dom);
    let reply = node(&dom, "message-3");

    receive(&mut dom, ", where to?");
    let diff = dom.layout_diff(WIDTH, HEIGHT, 1.0);

    assert!(!diff.changed.is_empty(), "the reply changed");
    let changed: Vec<NodeId> = diff.changed.iter().map(|entry| entry.node).collect();
    assert!(
        changed.iter().all(|&node| node == reply),
        "only the reply's entry changes: {changed:?} (reply is {reply})"
    );
    assert!(diff.removed.is_empty(), "{:?}", diff.removed);
    assert_eq!(diff.order, None);

    // "Sure, where to?" is 15 characters: 174 wide, still one line at the left.
    let list = dom.display_list().unwrap();
    let message = entry(&dom, list, "message-3");
    assert_eq!(message.texts[0].text, "Sure, where to?");
    assert_rect(message.rect, 12.0, 100.0, 174.0, 36.0);
}

/// Text the renderer commits to the composer reaches its `oninput`, which keeps it as the
/// draft, and pressing Enter in the composer sends it as the Send button does.
#[test]
fn fr34_chat_typing_and_enter_send_a_message() {
    let mut dom = open();
    layout(&mut dom);
    let field = node(&dom, "draft");

    assert_eq!(dom.input(field, "Hi there"), Some(field));
    dom.render();
    assert_eq!(draft_text(&dom), "Hi there");

    let composer = node(&dom, "composer");
    assert_eq!(
        dom.submit(field),
        Some(composer),
        "Enter clicks the composer's submit button, which submits it"
    );
    dom.render();
    let list = layout(&mut dom);
    assert_eq!(entry(&dom, &list, "message-4").texts[0].text, "Hi there");
    assert_eq!(draft_text(&dom), "");
    assert_eq!(
        entry(&dom, &list, "draft").input.as_ref().unwrap().value,
        ""
    );
}
