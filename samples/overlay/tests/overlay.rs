//! The overlay example laid out in an 800 by 600 viewport.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px, so
//! every number below follows from the stylesheet by hand:
//!
//! - The toolbar is 48px tall with 16px of side padding and centres its 32px buttons, so
//!   they sit at y = 8. "File" is 40px of text and 24px of padding: 64px wide at x = 16.
//! - The menu hangs from the bottom of the File button's wrapper (`top: 100%`), at
//!   y = 8 + 32 = 40, x = 16. It is 180px wide and 1 + 4 + 3 x 32 + 4 + 1 = 106px tall,
//!   its items 32px apart from y = 40 + 1 + 4 = 45.
//! - The page starts below the toolbar with 16px of padding: the card is at (16, 64),
//!   768 x 120, and the preview 16px below it at (16, 200), 240 x 80.
//! - The dialog is 320px wide and 24 + 20 + 8 + 20 + 16 + 32 + 24 = 144px tall (padding,
//!   heading, gap, paragraph, gap, buttons, padding). Its top-left corner is put at the
//!   middle of the viewport and moved back by half its own size: (400 - 160, 300 - 72).

use dioxus_compose::html::{
    DisplayList, HtmlConfig, HtmlDom, NodeEntry, NodeId, Rect, Rgba, TextMeasureRequest,
    TextMeasurer, TextMetrics, WidthConstraint,
};
use sample_html_overlay as overlay;

const WIDTH: f32 = 800.0;
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
        overlay::app,
        HtmlConfig {
            stylesheets: vec![overlay::STYLE.to_string()],
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

/// Clicks the middle of the element with `id` in the last layout and applies what the
/// handler changed.
#[track_caller]
fn click(dom: &mut HtmlDom, id: &str) -> NodeId {
    let rect = entry(dom, dom.display_list().expect("laid out"), id).rect;
    let target = dom
        .click(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
        .unwrap_or_else(|| panic!("nothing under #{id} listens for clicks"));
    dom.render();
    target
}

#[track_caller]
fn paint_index(dom: &HtmlDom, list: &DisplayList, id: &str) -> usize {
    let node = node(dom, id);
    list.order()
        .iter()
        .position(|&painted| painted == node)
        .unwrap_or_else(|| panic!("#{id} is not painted"))
}

/// Opens the File menu, picks Delete, and lays out the page with the dialog up.
fn open_dialog(dom: &mut HtmlDom) -> DisplayList {
    layout(dom);
    click(dom, "file-button");
    layout(dom);
    click(dom, "menu-delete");
    layout(dom)
}

#[test]
fn fr34_overlay_menu_opens_below_its_button_over_later_content() {
    let mut dom = open();
    let list = layout(&mut dom);
    assert_rect(
        entry(&dom, &list, "file-button").rect,
        16.0,
        8.0,
        64.0,
        32.0,
    );
    assert!(
        dom.element_by_id("menu").is_none(),
        "the menu starts closed"
    );

    click(&mut dom, "file-button");
    let list = layout(&mut dom);

    let menu = entry(&dom, &list, "menu");
    assert_rect(menu.rect, 16.0, 40.0, 180.0, 106.0);
    assert_rect(
        entry(&dom, &list, "menu-rename").rect,
        17.0,
        45.0,
        178.0,
        32.0,
    );
    assert_rect(
        entry(&dom, &list, "menu-duplicate").rect,
        17.0,
        77.0,
        178.0,
        32.0,
    );
    assert_rect(
        entry(&dom, &list, "menu-delete").rect,
        17.0,
        109.0,
        178.0,
        32.0,
    );
    assert_eq!(menu.background, Some(Rgba::new(255, 255, 255, 255)));
    assert_eq!(menu.shadows.len(), 1);

    // The card comes after the toolbar in the document and lies under the menu; the menu,
    // positioned with a z-index, is painted after it all the same.
    assert_rect(entry(&dom, &list, "card").rect, 16.0, 64.0, 768.0, 120.0);
    let menu_index = paint_index(&dom, &list, "menu");
    for later in ["page", "card", "status", "preview", "preview-banner"] {
        assert!(
            menu_index > paint_index(&dom, &list, later),
            "the menu is painted after #{later}"
        );
    }
    // Where the two overlap, a point lands on the menu item, not the card.
    assert_eq!(
        list.hit_test(100.0, 100.0),
        Some(node(&dom, "menu-duplicate"))
    );

    // Choosing an item closes the menu.
    click(&mut dom, "menu-duplicate");
    layout(&mut dom);
    assert!(dom.element_by_id("menu").is_none());
}

// CSS transitions are not carried by the display list yet. Once they are, the dialog's
// fade-in (the backdrop's opacity going from 0 to 0.5) is checked next to this test.
#[test]
fn fr34_overlay_dialog_is_centred_over_a_backdrop_covering_the_viewport() {
    let mut dom = open();
    let list = open_dialog(&mut dom);
    assert!(
        dom.element_by_id("menu").is_none(),
        "picking Delete closes the menu"
    );

    let backdrop = entry(&dom, &list, "backdrop");
    assert_rect(backdrop.rect, 0.0, 0.0, 800.0, 600.0);
    assert_eq!(backdrop.opacity, 0.5);
    assert_eq!(backdrop.background, Some(Rgba::new(15, 23, 42, 255)));

    let dialog = entry(&dom, &list, "dialog");
    assert_rect(dialog.rect, 240.0, 228.0, 320.0, 144.0);
    assert_eq!(dialog.opacity, 1.0, "the backdrop's opacity is its own");

    // The backdrop is painted over everything on the page, and the dialog over it.
    let backdrop_index = paint_index(&dom, &list, "backdrop");
    for below in ["file-button", "card", "preview-banner"] {
        assert!(
            backdrop_index > paint_index(&dom, &list, below),
            "the backdrop covers #{below}"
        );
    }
    assert!(paint_index(&dom, &list, "dialog") > backdrop_index);
    assert!(paint_index(&dom, &list, "confirm") > backdrop_index);

    // Centred in whatever viewport: in 1000 x 700, at (500 - 160, 350 - 72).
    let list = dom.layout(1000.0, 700.0, 1.0).clone();
    assert_rect(entry(&dom, &list, "backdrop").rect, 0.0, 0.0, 1000.0, 700.0);
    assert_rect(
        entry(&dom, &list, "dialog").rect,
        340.0,
        278.0,
        320.0,
        144.0,
    );
}

#[test]
fn fr34_overlay_backdrop_click_reaches_the_backdrop_not_the_page() {
    let mut dom = open();
    let list = open_dialog(&mut dom);
    let backdrop = node(&dom, "backdrop");

    // The File button is under the backdrop. A click there is the backdrop's.
    let file_button = entry(&dom, &list, "file-button").rect;
    let (x, y) = (
        file_button.x + file_button.width / 2.0,
        file_button.y + file_button.height / 2.0,
    );
    assert_eq!(list.hit_test(x, y), Some(backdrop));

    // A click inside the dialog, on its heading, reaches nothing that listens: the
    // backdrop is the dialog's sibling, not its ancestor, and the dialog stays open.
    let heading = entry(&dom, &list, "dialog-title").rect;
    assert_eq!(
        dom.click(heading.x + 4.0, heading.y + heading.height / 2.0),
        None
    );
    dom.render();
    layout(&mut dom);
    assert!(dom.element_by_id("dialog").is_some());

    assert_eq!(dom.click(x, y), Some(backdrop));
    dom.render();
    layout(&mut dom);
    assert!(
        dom.element_by_id("dialog").is_none(),
        "the backdrop's handler closed the dialog"
    );
    assert!(dom.element_by_id("backdrop").is_none());
    assert!(
        dom.element_by_id("menu").is_none(),
        "the File button under the backdrop did not get the click"
    );
}

#[test]
fn fr34_overlay_hidden_overflow_clips_its_child() {
    let mut dom = open();
    let list = layout(&mut dom);

    let preview = entry(&dom, &list, "preview");
    assert_rect(preview.rect, 16.0, 200.0, 240.0, 80.0);
    assert!(preview.clips_children);
    assert!(preview.radii.is_some());

    let banner = entry(&dom, &list, "preview-banner");
    assert_rect(banner.rect, 16.0, 200.0, 400.0, 160.0);
    assert_eq!(banner.clip, Some(Rect::new(16.0, 200.0, 240.0, 80.0)));

    // Inside the banner's box but outside the preview, the banner is not there to hit.
    assert_eq!(
        list.hit_test(100.0, 240.0),
        Some(node(&dom, "preview-banner"))
    );
    assert_ne!(
        list.hit_test(300.0, 250.0),
        Some(node(&dom, "preview-banner"))
    );
}
