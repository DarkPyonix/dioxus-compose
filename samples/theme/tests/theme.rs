//! The theme example laid out in a 400 by 300 viewport.
//!
//! The colours checked are the stylesheet's custom properties as written: light
//! `--bg: #ffffff`, `--text: #111827`, `--surface: #f3f4f6`, `--accent: #2563eb`; dark
//! `--bg: #111827`, `--text: #f9fafb`, `--surface: #1f2937`, `--accent: #60a5fa`.

use dioxus_compose::html::{
    ColorScheme, DisplayList, HtmlConfig, HtmlDom, NodeEntry, NodeId, Rect, Rgba, Sides,
    TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint,
};
use sample_html_theme as theme;

const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 300.0;
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

/// The colours one theme gives the page.
#[derive(Debug, PartialEq)]
struct Palette {
    background: Option<Rgba>,
    text: Rgba,
    surface: Option<Rgba>,
    accent: Option<Rgba>,
}

const LIGHT: Palette = Palette {
    background: Some(Rgba::new(255, 255, 255, 255)),
    text: Rgba::new(17, 24, 39, 255),
    surface: Some(Rgba::new(243, 244, 246, 255)),
    accent: Some(Rgba::new(37, 99, 235, 255)),
};

const DARK: Palette = Palette {
    background: Some(Rgba::new(17, 24, 39, 255)),
    text: Rgba::new(249, 250, 251, 255),
    surface: Some(Rgba::new(31, 41, 55, 255)),
    accent: Some(Rgba::new(96, 165, 250, 255)),
};

fn open(color_scheme: ColorScheme) -> HtmlDom {
    HtmlDom::with_config(
        theme::app,
        HtmlConfig {
            stylesheets: vec![theme::STYLE.to_string()],
            measurer: Some(Box::new(FixedAdvance)),
            color_scheme,
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

/// Clicks the theme button and lays the page out again.
fn toggle(dom: &mut HtmlDom) -> DisplayList {
    let rect = entry(dom, dom.display_list().expect("laid out"), "theme-toggle").rect;
    let target = dom.click(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    assert_eq!(target, Some(node(dom, "theme-toggle")));
    dom.render();
    layout(dom)
}

/// The colour of the text the element with `id` holds.
#[track_caller]
fn text_colour(dom: &HtmlDom, list: &DisplayList, id: &str) -> Rgba {
    let owner = node(dom, id);
    list.entries
        .iter()
        .flat_map(|entry| entry.texts.iter())
        .find(|run| run.owner == owner)
        .unwrap_or_else(|| panic!("#{id} draws no text"))
        .color
}

fn palette(dom: &HtmlDom, list: &DisplayList) -> Palette {
    let text = text_colour(dom, list, "title");
    assert_eq!(
        text_colour(dom, list, "body-text"),
        text,
        "the card's text inherits the page's text colour"
    );
    Palette {
        background: entry(dom, list, "app").background,
        text,
        surface: entry(dom, list, "card").background,
        accent: entry(dom, list, "theme-toggle").background,
    }
}

/// The entry with every colour taken out: what is left is geometry, text and shape.
fn without_colours(entry: &NodeEntry) -> NodeEntry {
    let mut entry = entry.clone();
    entry.background = None;
    if let Some(border) = entry.border.as_mut() {
        border.colors = Sides::default();
    }
    for shadow in &mut entry.shadows {
        shadow.color = Rgba::default();
    }
    for run in &mut entry.texts {
        run.color = Rgba::default();
    }
    if let Some(input) = entry.input.as_mut() {
        input.color = Rgba::default();
    }
    entry
}

#[test]
fn fr34_theme_light_and_dark_colours_resolve_from_custom_properties() {
    let mut dom = open(ColorScheme::Light);
    let list = layout(&mut dom);
    assert_eq!(palette(&dom, &list), LIGHT);
    // The app fills the viewport, whatever the theme.
    assert_rect(entry(&dom, &list, "app").rect, 0.0, 0.0, 400.0, 300.0);

    let list = toggle(&mut dom);
    assert_eq!(palette(&dom, &list), DARK);
    let card = entry(&dom, &list, "card");
    let border = card.border.expect("the card has a 1px border");
    assert_eq!(
        border.colors.top,
        Rgba::new(55, 65, 81, 255),
        "--border, dark"
    );

    let list = toggle(&mut dom);
    assert_eq!(palette(&dom, &list), LIGHT);
    let border = entry(&dom, &list, "card").border.unwrap();
    assert_eq!(
        border.colors.top,
        Rgba::new(209, 213, 219, 255),
        "--border, light"
    );
}

// CSS transitions are not carried by the display list yet. Once they are, a
// `transition: background-color` on the app is checked next to this test: toggling then
// changes the same entries, with the colour reached over the transition's duration.
#[test]
fn fr34_theme_toggle_changes_only_colours() {
    let mut dom = open(ColorScheme::Light);
    let before = layout(&mut dom);

    let rect = entry(&dom, &before, "theme-toggle").rect;
    dom.click(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    dom.render();
    let diff = dom.layout_diff(WIDTH, HEIGHT, 1.0);

    assert!(!diff.changed.is_empty(), "the colours changed");
    assert!(diff.removed.is_empty(), "{:?}", diff.removed);
    assert_eq!(diff.order, None, "nothing was added, removed or reordered");
    for changed in &diff.changed {
        let previous = before
            .get(changed.node)
            .unwrap_or_else(|| panic!("node {} ({:?}) is new", changed.node, changed.tag));
        assert_eq!(
            without_colours(changed),
            without_colours(previous),
            "node {} ({:?}) changed more than its colours",
            changed.node,
            changed.tag
        );
    }
    let changed: Vec<NodeId> = diff.changed.iter().map(|entry| entry.node).collect();
    for id in ["app", "card", "theme-toggle"] {
        assert!(changed.contains(&node(&dom, id)), "#{id} changed colour");
    }
}

#[test]
fn fr34_theme_color_scheme_selects_the_media_query_default() {
    let mut dom = open(ColorScheme::Light);
    let list = layout(&mut dom);
    assert_eq!(palette(&dom, &list), LIGHT);

    let mut dom = open(ColorScheme::Dark);
    let list = layout(&mut dom);
    assert_eq!(
        palette(&dom, &list),
        DARK,
        "with no class on the app, prefers-color-scheme: dark picks the dark set"
    );

    // A class on the app wins over the system: the first click picks dark (no change
    // here), the second light.
    let list = toggle(&mut dom);
    assert_eq!(palette(&dom, &list), DARK);
    let list = toggle(&mut dom);
    assert_eq!(palette(&dom, &list), LIGHT);
}
