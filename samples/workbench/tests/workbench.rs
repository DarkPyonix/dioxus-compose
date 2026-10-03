//! The workbench example laid out in a 1000 by 600 viewport.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px, so
//! every number below follows from the stylesheet by hand:
//!
//! - The status bar is 22px tall at the bottom, y = 578. Above it the main row is 578px
//!   tall: the 48px activity bar, the 240px sidebar at x = 48, and the editor taking the
//!   remaining 1000 - 288 = 712px at x = 288.
//! - In the sidebar a 32px title comes first, then the tree's 22px rows from y = 32. Every
//!   level of nesting moves a row 12px right (the nested list's left padding), so a row at
//!   depth d is at x = 48 + 12d and 240 - 12d wide.
//! - A tab is 120px wide unless its contents need more: 12px of padding each side, a 1px
//!   right border and the name at 10px a character. "layout_engine.rs" needs
//!   24 + 1 + 160 = 185px; "main.rs" (95px) and "README.md" (115px) stay at 120.

use dioxus_compose::html::{
    DisplayList, HtmlConfig, HtmlDom, NodeEntry, NodeId, Rect, Rgba, TextMeasureRequest,
    TextMeasurer, TextMetrics, WidthConstraint,
};
use sample_html_workbench as workbench;

const WIDTH: f32 = 1000.0;
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
        workbench::app,
        HtmlConfig {
            stylesheets: vec![workbench::STYLE.to_string()],
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

/// All the text drawn for the element with `id`, wherever its runs are listed.
fn text_of(dom: &HtmlDom, list: &DisplayList, id: &str) -> String {
    let owner = node(dom, id);
    list.entries
        .iter()
        .flat_map(|entry| entry.texts.iter())
        .filter(|run| run.owner == owner)
        .map(|run| run.text.as_str())
        .collect()
}

/// The text a tree row shows: its twisty, then its name.
fn row_text(dom: &HtmlDom, list: &DisplayList, id: &str) -> String {
    let row = node(dom, id);
    list.entries
        .iter()
        .filter(|entry| entry.parent == Some(row))
        .flat_map(|entry| entry.texts.iter())
        .map(|run| run.text.as_str())
        .collect()
}

/// The tree's rows from the top, with their depth, while `src` and `src/ui` are open.
const OPEN_ROWS: [(&str, f32); 9] = [
    ("src", 0.0),
    ("src/ui", 1.0),
    ("src/ui/mod.rs", 2.0),
    ("src/ui/tabs.rs", 2.0),
    ("src/layout_engine.rs", 1.0),
    ("src/main.rs", 1.0),
    ("tests", 0.0),
    ("Cargo.toml", 0.0),
    ("README.md", 0.0),
];

#[track_caller]
fn assert_row(dom: &HtmlDom, list: &DisplayList, path: &str, index: f32, depth: f32) {
    assert_rect(
        entry(dom, list, &format!("row-{path}")).rect,
        48.0 + 12.0 * depth,
        32.0 + 22.0 * index,
        240.0 - 12.0 * depth,
        22.0,
    );
}

#[test]
fn fr34_workbench_areas_fill_the_viewport() {
    let mut dom = open();
    let list = layout(&mut dom);

    assert_rect(
        entry(&dom, &list, "activity-bar").rect,
        0.0,
        0.0,
        48.0,
        578.0,
    );
    assert_rect(entry(&dom, &list, "sidebar").rect, 48.0, 0.0, 240.0, 578.0);
    assert_rect(entry(&dom, &list, "editor").rect, 288.0, 0.0, 712.0, 578.0);
    assert_rect(
        entry(&dom, &list, "statusbar").rect,
        0.0,
        578.0,
        1000.0,
        22.0,
    );

    // The activity bar's buttons stack from the top, 48px each.
    assert_rect(
        entry(&dom, &list, "view-Explorer").rect,
        0.0,
        0.0,
        48.0,
        48.0,
    );
    assert_rect(
        entry(&dom, &list, "view-Search").rect,
        0.0,
        48.0,
        48.0,
        48.0,
    );
    // The tab strip runs along the editor's top.
    assert_rect(entry(&dom, &list, "tabs").rect, 288.0, 0.0, 712.0, 35.0);

    // The sidebar scrolls its tree vertically.
    let sidebar = entry(&dom, &list, "sidebar");
    let scroll = sidebar.scroll.expect("overflow-y: auto");
    assert!(scroll.vertical && !scroll.horizontal);

    for (index, (path, depth)) in OPEN_ROWS.into_iter().enumerate() {
        assert_row(&dom, &list, path, index as f32, depth);
    }
}

#[test]
fn fr34_workbench_collapsing_a_folder_removes_its_rows() {
    let mut dom = open();
    layout(&mut dom);
    let hidden = [
        node(&dom, "row-src/ui/mod.rs"),
        node(&dom, "row-src/ui/tabs.rs"),
    ];
    assert_eq!(
        row_text(&dom, dom.display_list().unwrap(), "row-src/ui"),
        "\u{25be}ui"
    );

    click(&mut dom, "row-src/ui");
    let diff = dom.layout_diff(WIDTH, HEIGHT, 1.0);

    for row in hidden {
        assert!(
            diff.removed.contains(&row),
            "row {row} is gone: {:?}",
            diff.removed
        );
    }
    assert!(dom.element_by_id("row-src/ui/mod.rs").is_none());
    assert!(dom.element_by_id("row-src/ui/tabs.rs").is_none());

    // Everything after the folder moves up by its two rows, 44px.
    let list = dom.display_list().unwrap();
    let rows = [
        ("src", 0.0),
        ("src/ui", 1.0),
        ("src/layout_engine.rs", 1.0),
        ("src/main.rs", 1.0),
        ("tests", 0.0),
        ("Cargo.toml", 0.0),
        ("README.md", 0.0),
    ];
    for (index, (path, depth)) in rows.into_iter().enumerate() {
        assert_row(&dom, list, path, index as f32, depth);
    }
    assert_eq!(
        row_text(&dom, list, "row-src/ui"),
        "\u{25b8}ui",
        "the folder shows closed"
    );

    // Opening it again brings the rows back where they were.
    click(&mut dom, "row-src/ui");
    let list = layout(&mut dom);
    for (index, (path, depth)) in OPEN_ROWS.into_iter().enumerate() {
        assert_row(&dom, &list, path, index as f32, depth);
    }
}

#[test]
fn fr34_workbench_tab_widths_follow_their_labels() {
    let mut dom = open();
    let list = layout(&mut dom);

    assert_rect(
        entry(&dom, &list, "tab-main.rs").rect,
        288.0,
        0.0,
        120.0,
        35.0,
    );
    assert_rect(
        entry(&dom, &list, "tab-layout_engine.rs").rect,
        408.0,
        0.0,
        185.0,
        35.0,
    );
    assert_rect(
        entry(&dom, &list, "tab-README.md").rect,
        593.0,
        0.0,
        120.0,
        35.0,
    );
}

#[test]
fn fr34_workbench_clicking_a_tab_activates_it() {
    const ACTIVE: Rgba = Rgba::new(30, 30, 30, 255);
    const INACTIVE: Rgba = Rgba::new(45, 45, 45, 255);

    let mut dom = open();
    let list = layout(&mut dom);
    assert_eq!(entry(&dom, &list, "tab-main.rs").background, Some(ACTIVE));
    assert_eq!(
        entry(&dom, &list, "tab-README.md").background,
        Some(INACTIVE)
    );
    assert_eq!(text_of(&dom, &list, "breadcrumbs"), "src \u{203a} main.rs");

    let target = click(&mut dom, "tab-README.md");
    assert_eq!(target, node(&dom, "tab-README.md"));
    let list = layout(&mut dom);

    assert_eq!(entry(&dom, &list, "tab-README.md").background, Some(ACTIVE));
    assert_eq!(entry(&dom, &list, "tab-main.rs").background, Some(INACTIVE));
    assert_eq!(text_of(&dom, &list, "breadcrumbs"), "README.md");
    // Activating a tab moves nothing.
    assert_rect(
        entry(&dom, &list, "tab-main.rs").rect,
        288.0,
        0.0,
        120.0,
        35.0,
    );
    assert_rect(
        entry(&dom, &list, "tab-README.md").rect,
        593.0,
        0.0,
        120.0,
        35.0,
    );
    // The file is selected in the tree too.
    assert_eq!(
        entry(&dom, &list, "row-README.md").background,
        Some(Rgba::new(55, 55, 61, 255))
    );
}
