//! HTML and CSS laid out inside the Host.
//!
//! Every expected number in these tests is worked out by hand from the CSS in the fixture,
//! never read back from blitz-dom: fixed widths, padding, borders and flex rows and columns
//! whose geometry follows from the CSS box model alone. Text is sized by a fake measurer
//! with a fixed advance per character, so its numbers are known too.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose_html::prelude::*;
use dioxus_compose_html::{
    BaseDocument, BorderLine, DisplayList, HtmlConfig, HtmlDom, InputKind, NodeEntry, Rect, Rgba,
    TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint, element_by_id, layout_document,
};
use dioxus_core::ScopeId;

/// A measurer that gives every character the same advance and every line the same height,
/// and breaks lines greedily at spaces.
struct FakeMeasurer {
    char_width: f32,
    line_height: f32,
    calls: Rc<RefCell<Vec<(String, f32, WidthConstraint)>>>,
}

impl FakeMeasurer {
    fn new(char_width: f32, line_height: f32) -> Self {
        Self {
            char_width,
            line_height,
            calls: Rc::default(),
        }
    }
}

impl TextMeasurer for FakeMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        self.calls.borrow_mut().push((
            request.text.to_string(),
            request.style.font_size,
            request.width,
        ));
        let max_chars = match request.width {
            WidthConstraint::MaxContent => usize::MAX,
            WidthConstraint::MinContent => 0,
            WidthConstraint::AtMost(width) => (width / self.char_width).floor() as usize,
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
            .map(|line| line.chars().count() as f32 * self.char_width)
            .fold(0.0, f32::max);
        TextMetrics {
            width,
            height: lines.len() as f32 * self.line_height,
            first_baseline: self.line_height * 0.75,
            line_count: lines.len() as u32,
        }
    }
}

fn parse(html: &str, width: u32, height: u32) -> BaseDocument {
    let config = DocumentConfig {
        viewport: Some(Viewport::new(width, height, 1.0, ColorScheme::Light)),
        ..DocumentConfig::default()
    };
    HtmlDocument::from_html(html, config).into_inner()
}

fn entry<'a>(list: &'a DisplayList, doc: &BaseDocument, id: &str) -> &'a NodeEntry {
    let node = element_by_id(doc, id).unwrap_or_else(|| panic!("no element with id {id}"));
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

const REFERENCE_FIXTURE: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
html, body { margin: 0; padding: 0; }
#root { display: flex; flex-direction: column; width: 300px; padding: 10px; border: 5px solid rgb(0, 0, 0); }
#a { height: 40px; margin-bottom: 10px; background-color: rgb(255, 0, 0); }
#row { display: flex; flex-direction: row; height: 50px; }
#b { width: 100px; padding: 5px; border: 2px solid rgb(0, 0, 255); border-radius: 4px 8px; box-shadow: 1px 2px 3px 4px rgba(0, 0, 0, 0.4); }
#c { flex-grow: 1; }
#block { width: 200px; }
#d { width: 100px; height: 20px; margin-left: auto; margin-right: auto; }
</style>
</head><body><div id="root"><div id="a"></div><div id="row"><div id="b"></div><div id="c"></div></div></div><div id="block"><div id="d"></div></div></body></html>"#;

/// #root is a flex column: 300px content, 10px padding and a 5px border make it 330px
/// wide, its content starting at (15, 15). #a fills the column's width, 40px tall, then a
/// 10px margin. #row follows at y = 15 + 40 + 10 = 65. In it #b is 100px of content plus
/// 2 x 5px padding plus 2 x 2px border = 114px, stretched to the row's 50px; #c grows into
/// the remaining 300 - 114 = 186px. #root is 5 + 10 + 40 + 10 + 50 + 10 + 5 = 130px tall,
/// so #block starts at y = 130, and #d, 100px wide with auto side margins in a 200px
/// block, is centred at x = 50.
#[test]
fn fr34_reference_fixture_boxes_match_hand_computed_geometry() {
    let mut doc = parse(REFERENCE_FIXTURE, 800, 600);
    let mut measurer = FakeMeasurer::new(10.0, 20.0);
    let list = layout_document(&mut doc, &mut measurer);

    assert_rect(entry(&list, &doc, "root").rect, 0.0, 0.0, 330.0, 130.0);
    assert_rect(entry(&list, &doc, "a").rect, 15.0, 15.0, 300.0, 40.0);
    assert_rect(entry(&list, &doc, "row").rect, 15.0, 65.0, 300.0, 50.0);
    assert_rect(entry(&list, &doc, "b").rect, 15.0, 65.0, 114.0, 50.0);
    assert_rect(entry(&list, &doc, "c").rect, 129.0, 65.0, 186.0, 50.0);
    assert_rect(entry(&list, &doc, "block").rect, 0.0, 130.0, 200.0, 20.0);
    assert_rect(entry(&list, &doc, "d").rect, 50.0, 130.0, 100.0, 20.0);
}

#[test]
fn fr34_reference_fixture_decorations_are_resolved_values() {
    let mut doc = parse(REFERENCE_FIXTURE, 800, 600);
    let mut measurer = FakeMeasurer::new(10.0, 20.0);
    let list = layout_document(&mut doc, &mut measurer);

    let root = entry(&list, &doc, "root");
    let border = root.border.expect("#root has a 5px border");
    assert_eq!(border.widths.top, 5.0);
    assert_eq!(border.widths.left, 5.0);
    assert_eq!(border.colors.bottom, Rgba::new(0, 0, 0, 255));
    assert_eq!(border.lines.right, BorderLine::Solid);
    assert_eq!(
        root.background, None,
        "a transparent background is no background"
    );

    assert_eq!(
        entry(&list, &doc, "a").background,
        Some(Rgba::new(255, 0, 0, 255))
    );

    let b = entry(&list, &doc, "b");
    let border = b.border.expect("#b has a 2px border");
    assert_eq!(
        (
            border.widths.top,
            border.widths.right,
            border.widths.bottom,
            border.widths.left
        ),
        (2.0, 2.0, 2.0, 2.0)
    );
    assert_eq!(border.colors.left, Rgba::new(0, 0, 255, 255));
    // `border-radius: 4px 8px`: top-left and bottom-right 4px, the other two 8px.
    let radii = b.radii.expect("#b has rounded corners");
    assert_eq!((radii.top_left.x, radii.top_left.y), (4.0, 4.0));
    assert_eq!((radii.top_right.x, radii.top_right.y), (8.0, 8.0));
    assert_eq!((radii.bottom_right.x, radii.bottom_right.y), (4.0, 4.0));
    assert_eq!((radii.bottom_left.x, radii.bottom_left.y), (8.0, 8.0));
    assert_eq!(b.shadows.len(), 1);
    let shadow = b.shadows[0];
    assert_eq!(
        (
            shadow.offset_x,
            shadow.offset_y,
            shadow.blur,
            shadow.spread,
            shadow.inset
        ),
        (1.0, 2.0, 3.0, 4.0, false)
    );
    // 0.4 x 255 = 102.
    assert_eq!(shadow.color, Rgba::new(0, 0, 0, 102));
    assert_eq!(b.opacity, 1.0);
    assert_eq!(b.clip, None);

    // Paint order is tree order for boxes in normal flow.
    let ids = ["root", "a", "row", "b", "c", "block", "d"];
    let wanted: Vec<usize> = ids
        .iter()
        .map(|id| element_by_id(&doc, id).unwrap())
        .collect();
    let order: Vec<usize> = list
        .order()
        .into_iter()
        .filter(|node| wanted.contains(node))
        .collect();
    assert_eq!(order, wanted);
}

const TEXT_FIXTURE: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
html, body { margin: 0; }
#row { display: flex; flex-direction: row; align-items: flex-start; }
#label { font-size: 16px; color: rgb(10, 20, 30); }
#box { width: 30px; height: 30px; }
</style>
</head><body><div id="row"><div id="label">Hello</div><div id="box"></div></div></body></html>"#;

/// The label is a flex item sized by its text, so the measurer's answer moves the box
/// after it: five characters at 10px put the box at x = 50, at 7px at x = 35.
#[test]
fn fr34_text_measurer_sizes_text_leaves() {
    for (char_width, expected_width) in [(10.0, 50.0), (7.0, 35.0)] {
        let mut doc = parse(TEXT_FIXTURE, 800, 600);
        let mut measurer = FakeMeasurer::new(char_width, 20.0);
        let calls = measurer.calls.clone();
        let list = layout_document(&mut doc, &mut measurer);

        assert!(
            calls
                .borrow()
                .iter()
                .any(|(text, size, _)| text == "Hello" && *size == 16.0),
            "the measurer was asked about the label's text at its font size: {:?}",
            calls.borrow()
        );

        let label = entry(&list, &doc, "label");
        assert_rect(label.rect, 0.0, 0.0, expected_width, 20.0);
        assert_rect(
            entry(&list, &doc, "box").rect,
            expected_width,
            0.0,
            30.0,
            30.0,
        );

        assert_eq!(label.texts.len(), 1);
        let run = &label.texts[0];
        assert_eq!(run.text, "Hello");
        assert_eq!(run.owner, element_by_id(&doc, "label").unwrap());
        assert_rect(run.rect, 0.0, 0.0, expected_width, 20.0);
        assert_eq!(run.style.font_size, 16.0);
        assert_eq!(run.color, Rgba::new(10, 20, 30, 255));
        assert_eq!(run.line_count, 1);
    }
}

/// A fixed-width paragraph wraps at its width: "aaa bbb ccc" at 10px per character in a
/// 70px box is "aaa bbb" and "ccc", two 20px lines.
#[test]
fn fr34_text_measurer_receives_the_wrap_width() {
    let html = r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#para { width: 70px; margin: 0; }
</style></head><body><p id="para">aaa bbb ccc</p></body></html>"#;
    let mut doc = parse(html, 800, 600);
    let mut measurer = FakeMeasurer::new(10.0, 20.0);
    let list = layout_document(&mut doc, &mut measurer);

    let para = entry(&list, &doc, "para");
    assert_rect(para.rect, 0.0, 0.0, 70.0, 40.0);
    let run = &para.texts[0];
    assert_eq!(run.wrap_width, Some(70.0));
    assert_eq!(run.line_count, 2);
    assert_rect(run.rect, 0.0, 0.0, 70.0, 40.0);
}

#[test]
fn fr34_paint_order_and_hit_test_follow_z_index() {
    let html = r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#high { position: absolute; left: 50px; top: 50px; width: 100px; height: 100px; z-index: 2; background-color: rgb(0, 0, 255); }
#low { position: absolute; left: 0px; top: 0px; width: 100px; height: 100px; z-index: 1; background-color: rgb(255, 0, 0); }
#flow { height: 10px; }
</style></head><body><div id="high"></div><div id="low"></div><div id="flow"></div></body></html>"#;
    let mut doc = parse(html, 800, 600);
    let mut measurer = FakeMeasurer::new(10.0, 20.0);
    let list = layout_document(&mut doc, &mut measurer);

    let high = element_by_id(&doc, "high").unwrap();
    let low = element_by_id(&doc, "low").unwrap();
    let flow = element_by_id(&doc, "flow").unwrap();
    let order: Vec<usize> = list
        .order()
        .into_iter()
        .filter(|node| [high, low, flow].contains(node))
        .collect();
    // Normal flow first, then z-index 1, then z-index 2, whatever the tree order.
    assert_eq!(order, vec![flow, low, high]);

    assert_rect(entry(&list, &doc, "high").rect, 50.0, 50.0, 100.0, 100.0);
    // Where the two overlap, the higher z-index is on top.
    assert_eq!(list.hit_test(75.0, 75.0), Some(high));
    assert_eq!(list.hit_test(25.0, 25.0), Some(low));
    assert_eq!(list.hit_test(125.0, 125.0), Some(high));
}

#[test]
fn fr34_clip_opacity_and_scroll_containers() {
    let html = r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#clipper { width: 50px; height: 40px; overflow: hidden; opacity: 0.5; }
#big { width: 100px; height: 100px; opacity: 0.5; }
#scroller { width: 60px; height: 60px; overflow: auto; }
#tall { width: 60px; height: 200px; }
</style></head><body><div id="clipper"><div id="big"></div></div><div id="scroller"><div id="tall"></div></div></body></html>"#;
    let mut doc = parse(html, 800, 600);
    let mut measurer = FakeMeasurer::new(10.0, 20.0);
    let list = layout_document(&mut doc, &mut measurer);

    let clipper = entry(&list, &doc, "clipper");
    assert!(clipper.clips_children);
    assert_eq!(clipper.clip, None);
    assert_eq!(clipper.opacity, 0.5);
    let big = entry(&list, &doc, "big");
    assert_eq!(big.clip, Some(Rect::new(0.0, 0.0, 50.0, 40.0)));
    assert_eq!(big.opacity, 0.25);

    let scroller_id = element_by_id(&doc, "scroller").unwrap();
    let scroller = entry(&list, &doc, "scroller");
    assert_rect(scroller.rect, 0.0, 40.0, 60.0, 60.0);
    let scroll = scroller.scroll.expect("overflow: auto scrolls");
    assert_rect(scroll.viewport, 0.0, 40.0, 60.0, 60.0);
    assert_eq!(scroll.content_height, 200.0);
    assert!(scroll.vertical);

    let tall_id = element_by_id(&doc, "tall").unwrap();
    let tall = entry(&list, &doc, "tall");
    assert_eq!(tall.scroll_parent, Some(scroller_id));
    assert_eq!(tall.clip, Some(Rect::new(0.0, 40.0, 60.0, 60.0)));

    // Unscrolled, the bottom of the scroller shows #tall. Scrolled down by 150px, the same
    // point is 245px into the content, past #tall's 200px, so it lands on the scroller.
    assert_eq!(list.hit_test(10.0, 95.0), Some(tall_id));
    let scrolled = |node: usize| {
        if node == scroller_id {
            (0.0, 150.0)
        } else {
            (0.0, 0.0)
        }
    };
    assert_eq!(
        list.hit_test_scrolled(10.0, 95.0, scrolled),
        Some(scroller_id)
    );
    // Outside the scroller's clip nothing inside it is hit.
    assert_ne!(list.hit_test(10.0, 150.0), Some(tall_id));
}

thread_local! {
    static TARGET_IS_RED: Cell<bool> = const { Cell::new(false) };
    static CLICKS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn margin_free() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        measurer: Some(Box::new(FakeMeasurer::new(10.0, 20.0))),
        ..HtmlConfig::default()
    }
}

fn three_bars() -> Element {
    let color = if TARGET_IS_RED.with(Cell::get) {
        "rgb(255, 0, 0)"
    } else {
        "rgb(0, 0, 255)"
    };
    rsx! {
        div { id: "list", style: "display: flex; flex-direction: column; width: 100px",
            div { id: "first", style: "height: 20px; background-color: rgb(0, 128, 0)" }
            div { id: "target", style: "height: 20px; background-color: {color}" }
            div { id: "third", style: "height: 20px; background-color: rgb(0, 128, 0)" }
        }
    }
}

#[test]
fn fr34_background_change_yields_one_diff_entry() {
    TARGET_IS_RED.with(|red| red.set(false));
    let mut dom = HtmlDom::with_config(three_bars, margin_free());
    let first = dom.layout_diff(400.0, 300.0, 1.0);
    let target = dom
        .element_by_id("target")
        .expect("#target is in the document");
    assert!(
        first.changed.iter().any(|entry| entry.node == target),
        "the first layout reports every entry"
    );
    assert_eq!(
        dom.display_list().unwrap().get(target).unwrap().background,
        Some(Rgba::new(0, 0, 255, 255))
    );

    // Nothing changed: nothing to send.
    assert!(dom.layout_diff(400.0, 300.0, 1.0).is_empty());

    TARGET_IS_RED.with(|red| red.set(true));
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    dom.render();
    let diff = dom.layout_diff(400.0, 300.0, 1.0);

    assert_eq!(
        diff.changed.len(),
        1,
        "only #target's entry changes: {:?}",
        diff.changed
            .iter()
            .map(|entry| (entry.node, &entry.tag))
            .collect::<Vec<_>>()
    );
    assert_eq!(diff.changed[0].node, target);
    assert_eq!(diff.changed[0].background, Some(Rgba::new(255, 0, 0, 255)));
    assert!(diff.removed.is_empty());
    assert_eq!(diff.order, None);
}

fn nested_listeners() -> Element {
    rsx! {
        div {
            id: "outer",
            style: "width: 200px; height: 200px",
            onclick: move |_| CLICKS.with(|clicks| clicks.borrow_mut().push("outer")),
            div { id: "middle", style: "width: 100px; height: 100px",
                div {
                    id: "inner",
                    style: "width: 50px; height: 50px",
                    onclick: move |_| CLICKS.with(|clicks| clicks.borrow_mut().push("inner")),
                }
            }
        }
        p { id: "para", style: "margin: 0; font-size: 10px",
            span {
                id: "word",
                onclick: move |_| CLICKS.with(|clicks| clicks.borrow_mut().push("word")),
                "Tap"
            }
        }
    }
}

fn take_clicks() -> Vec<&'static str> {
    CLICKS.with(|clicks| std::mem::take(&mut *clicks.borrow_mut()))
}

#[test]
fn fr34_hit_test_routes_to_innermost_listener() {
    take_clicks();
    let mut dom = HtmlDom::with_config(nested_listeners, margin_free());
    dom.layout(400.0, 400.0, 1.0);
    let outer = dom.element_by_id("outer").unwrap();
    let middle = dom.element_by_id("middle").unwrap();
    let inner = dom.element_by_id("inner").unwrap();
    let word = dom.element_by_id("word").unwrap();

    assert_eq!(dom.listeners(inner), &["click"]);
    assert!(dom.listeners(middle).is_empty());

    // Over #inner: the innermost box, and it has a listener. The event then bubbles to
    // #outer, as it would in a browser.
    assert_eq!(dom.hit_test(10.0, 10.0), Some(inner));
    assert_eq!(
        dom.listener_target(inner, "click").map(|(node, _)| node),
        Some(inner)
    );
    assert_eq!(dom.click(10.0, 10.0), Some(inner));
    assert_eq!(take_clicks(), vec!["inner", "outer"]);

    // Over #middle, which has no listener: delivered to #outer.
    assert_eq!(dom.hit_test(75.0, 75.0), Some(middle));
    assert_eq!(dom.click(75.0, 75.0), Some(outer));
    assert_eq!(take_clicks(), vec!["outer"]);

    // Over the paragraph's text, which belongs to the <span>: "Tap" is 30px wide at y 200.
    assert_eq!(dom.hit_test(5.0, 205.0), Some(word));
    assert_eq!(dom.click(5.0, 205.0), Some(word));
    assert_eq!(take_clicks(), vec!["word"]);

    // Empty canvas: nothing listens.
    assert_eq!(dom.click(390.0, 390.0), None);
    assert!(take_clicks().is_empty());
}

fn page_with_script() -> Element {
    rsx! {
        div { id: "page",
            script { "globalThis.ran = true; while (true) ;" }
            div { id: "after", style: "height: 10px" }
        }
    }
}

#[test]
fn fr34_script_is_never_executed() {
    let mut dom = HtmlDom::with_config(page_with_script, margin_free());
    let list = dom.layout(400.0, 300.0, 1.0).clone();

    assert!(
        list.entries.iter().all(|entry| entry.tag != "script"),
        "a <script> draws nothing"
    );
    assert!(
        list.entries
            .iter()
            .flat_map(|entry| entry.texts.iter())
            .all(|run| !run.text.contains("while")),
        "the script's source is not text on the page"
    );
    let after = dom.element_by_id("after").unwrap();
    assert_rect(list.get(after).unwrap().rect, 0.0, 0.0, 400.0, 10.0);
}

fn page_with_injected_script() -> Element {
    rsx! {
        div {
            id: "injected",
            dangerous_inner_html: "<script>while (true) ;</script><b id=\"ok\">ok</b>",
        }
    }
}

#[test]
fn fr34_script_in_inner_html_is_inert() {
    let mut dom = HtmlDom::with_config(page_with_injected_script, margin_free());
    let list = dom.layout(400.0, 300.0, 1.0).clone();

    assert!(list.entries.iter().all(|entry| entry.tag != "script"));
    let texts: Vec<&str> = list
        .entries
        .iter()
        .flat_map(|entry| entry.texts.iter())
        .map(|run| run.text.as_str())
        .collect();
    assert!(
        !texts.iter().any(|text| text.contains("while")),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"ok"),
        "the markup after the script is parsed: {texts:?}"
    );
}

fn form() -> Element {
    rsx! {
        input { id: "name", value: "Ada", placeholder: "Name", style: "width: 120px; height: 24px" }
    }
}

#[test]
fn fr34_input_carries_its_value_and_rect() {
    let mut dom = HtmlDom::with_config(form, margin_free());
    dom.layout(400.0, 300.0, 1.0);
    let name = dom.element_by_id("name").unwrap();
    let list = dom.display_list().unwrap();
    let entry = list.get(name).expect("the input has an entry");
    let input = entry.input.as_ref().expect("an <input> is a form field");
    assert_eq!(input.kind, InputKind::Text);
    assert_eq!(input.value, "Ada");
    assert_eq!(input.placeholder.as_deref(), Some("Name"));
    assert!(entry.rect.width > 0.0 && entry.rect.height > 0.0);
}
