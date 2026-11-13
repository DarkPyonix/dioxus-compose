//! Boxes sized to their contents: `width: max-content`, `min-content` and `fit-content`,
//! and an absolutely positioned box with no width, which CSS sizes shrink-to-fit.
//!
//! Every expected number is worked out by hand from the CSS in the fixture and the rules
//! Chromium follows, never read back from blitz-dom. Text is sized by a fake measurer that
//! gives every character a 10px advance and every line 20px, and breaks lines greedily at
//! spaces, so a text's min-content width is its longest word and its max-content width is
//! the whole text on one line.

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose::html::{
    BaseDocument, DisplayList, NodeEntry, Rect, TextMeasureRequest, TextMeasurer, TextMetrics,
    WidthConstraint, element_by_id, layout_document,
};

const CHAR: f32 = 10.0;
const LINE: f32 = 20.0;

struct FakeMeasurer;

impl TextMeasurer for FakeMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let max_chars = match request.width {
            WidthConstraint::MaxContent => usize::MAX,
            WidthConstraint::MinContent => 0,
            WidthConstraint::AtMost(width) => (width / CHAR + 0.001).floor() as usize,
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
            .map(|line| line.chars().count() as f32 * CHAR)
            .fold(0.0, f32::max);
        TextMetrics {
            width,
            height: lines.len() as f32 * LINE,
            first_baseline: LINE * 0.75,
            line_count: lines.len() as u32,
        }
    }
}

fn lay_out(body: &str, css: &str) -> (BaseDocument, DisplayList) {
    let html = format!(
        "<!DOCTYPE html><html><head><style>html, body {{ margin: 0; }} {css}</style></head>\
         <body>{body}</body></html>"
    );
    let config = DocumentConfig {
        viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
        ..DocumentConfig::default()
    };
    let mut doc = HtmlDocument::from_html(&html, config).into_inner();
    let list = layout_document(&mut doc, &mut FakeMeasurer);
    (doc, list)
}

fn entry<'a>(list: &'a DisplayList, doc: &BaseDocument, id: &str) -> &'a NodeEntry {
    let node = element_by_id(doc, id).unwrap_or_else(|| panic!("no element with id {id}"));
    list.get(node)
        .unwrap_or_else(|| panic!("#{id} (node {node}) has no display list entry"))
}

/// Records a mismatch instead of panicking, so one run reports every case that fails.
fn check_size(failures: &mut Vec<String>, case: &str, actual: Rect, width: f32, height: f32) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    if !(close(actual.width, width) && close(actual.height, height)) {
        failures.push(format!(
            "{case}: expected {width} x {height}, got {} x {} at ({}, {})",
            actual.width, actual.height, actual.x, actual.y
        ));
    }
}

#[track_caller]
fn assert_no_failures(failures: Vec<String>) {
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[track_caller]
fn assert_rect(case: &str, actual: Rect, x: f32, y: f32, width: f32, height: f32) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(
        close(actual.x, x)
            && close(actual.y, y)
            && close(actual.width, width)
            && close(actual.height, height),
        "{case}: expected ({x}, {y}, {width}, {height}), got ({}, {}, {}, {})",
        actual.x,
        actual.y,
        actual.width,
        actual.height
    );
}

/// `foo(alpha, beta)` is 16 characters, 160px on one line. Chromium gives a box with
/// `width: max-content` that width wherever it is: in a block narrower than the text (it
/// overflows), in a flex row, and absolutely positioned near the right edge of a narrow
/// positioned ancestor, which is the signature help hint of an editor. Padding and a border
/// go around it, and `box-sizing: border-box` does not change what the content needs.
#[test]
fn fr34_max_content_width_sizes_to_the_text() {
    let text = "foo(alpha, beta)";
    let cases = [
        (
            "in a narrow block",
            format!(r#"<div style="width: 50px"><div id="h">{text}</div></div>"#),
            "#h { width: max-content; }",
            160.0,
            LINE,
        ),
        (
            "in a wide block",
            format!(r#"<div id="h">{text}</div>"#),
            "#h { width: max-content; }",
            160.0,
            LINE,
        ),
        (
            "with padding and a border",
            format!(r#"<div style="width: 50px"><div id="h">{text}</div></div>"#),
            "#h { width: max-content; padding: 0 4px; border: 1px solid black; }",
            170.0,
            LINE + 2.0,
        ),
        (
            "border-box with padding",
            format!(r#"<div style="width: 50px"><div id="h">{text}</div></div>"#),
            "#h { width: max-content; padding: 0 4px; box-sizing: border-box; }",
            168.0,
            LINE,
        ),
        (
            "absolute near the right edge",
            format!(
                r#"<div style="position: relative; width: 200px; height: 100px"><div id="h">{text}</div></div>"#
            ),
            "#h { position: absolute; left: 150px; top: 10px; width: max-content; }",
            160.0,
            LINE,
        ),
        (
            "absolute in a zero-width positioned parent",
            format!(
                r#"<div style="position: relative; width: 0; height: 0"><div id="h">{text}</div></div>"#
            ),
            "#h { position: absolute; left: 0; top: 0; width: max-content; }",
            160.0,
            LINE,
        ),
        (
            "absolute, nested in a static wrapper",
            format!(
                r#"<div style="position: relative; width: 100px; height: 100px"><div style="width: 20px"><div id="h">{text}</div></div></div>"#
            ),
            "#h { position: absolute; left: 60px; top: 0; width: max-content; }",
            160.0,
            LINE,
        ),
        (
            "fixed",
            format!(r#"<div style="width: 30px"><div id="h">{text}</div></div>"#),
            "#h { position: fixed; left: 700px; top: 0; width: max-content; }",
            160.0,
            LINE,
        ),
        (
            "max-content capped by max-width",
            format!(r#"<div style="width: 50px"><div id="h">{text}</div></div>"#),
            "#h { width: max-content; max-width: 100px; }",
            100.0,
            2.0 * LINE,
        ),
    ];
    let mut failures = Vec::new();
    for (case, body, css, width, height) in cases {
        let (doc, list) = lay_out(&body, css);
        check_size(
            &mut failures,
            case,
            entry(&list, &doc, "h").rect,
            width,
            height,
        );
    }
    assert_no_failures(failures);

    // A flex item: max-content is its flex basis and it does not shrink below it.
    let (doc, list) = lay_out(
        &format!(r#"<div id="row"><div id="h">{text}</div><div id="next"></div></div>"#),
        "#row { display: flex; width: 100px; align-items: flex-start; }\
         #h { width: max-content; flex-shrink: 0; } #next { width: 10px; height: 10px; flex-shrink: 0; }",
    );
    assert_rect(
        "flex item",
        entry(&list, &doc, "h").rect,
        0.0,
        0.0,
        160.0,
        LINE,
    );
    assert_rect(
        "after the flex item",
        entry(&list, &doc, "next").rect,
        160.0,
        0.0,
        10.0,
        10.0,
    );

    // The hint is placed where its insets say.
    let (doc, list) = lay_out(
        &format!(
            r#"<div style="position: relative; width: 200px; height: 100px; margin-left: 20px"><div id="h">{text}</div></div>"#
        ),
        "#h { position: absolute; left: 150px; top: 10px; width: max-content; }",
    );
    assert_rect(
        "hint position",
        entry(&list, &doc, "h").rect,
        170.0,
        10.0,
        160.0,
        LINE,
    );
}

/// `aaa bbbbbbb cc`: the longest word is 7 characters, so `width: min-content` is 70px and
/// the text takes three lines, however wide the containing block is. A min-content box
/// with no break opportunities is the whole text.
#[test]
fn fr34_min_content_width_is_the_longest_word() {
    let cases = [
        (
            "in a wide block",
            r#"<div id="h">aaa bbbbbbb cc</div>"#,
            "#h { width: min-content; }",
            70.0,
            3.0 * LINE,
        ),
        (
            "absolute",
            r#"<div style="position: relative; width: 400px; height: 100px"><div id="h">aaa bbbbbbb cc</div></div>"#,
            "#h { position: absolute; left: 10px; top: 0; width: min-content; }",
            70.0,
            3.0 * LINE,
        ),
        (
            "one word",
            r#"<div style="width: 20px"><div id="h">abcdefgh</div></div>"#,
            "#h { width: min-content; }",
            80.0,
            LINE,
        ),
        (
            "with padding",
            r#"<div id="h">aaa bbbbbbb cc</div>"#,
            "#h { width: min-content; padding: 0 5px; }",
            80.0,
            3.0 * LINE,
        ),
    ];
    let mut failures = Vec::new();
    for (case, body, css, width, height) in cases {
        let (doc, list) = lay_out(body, css);
        check_size(
            &mut failures,
            case,
            entry(&list, &doc, "h").rect,
            width,
            height,
        );
    }
    assert_no_failures(failures);
}

/// `fit-content` is `min(max-content, max(min-content, available))`. In a 100px block,
/// `aaa bbbbb cc dd` (150px on one line, longest word 50px) is 100px wide and wraps after
/// `bbbbb`; `hi` is 20px; a word longer than the block keeps its own width. Absolutely
/// positioned at `left: 150px` in a 200px positioned box, 50px are available, so `aaa bb`
/// (60px, longest word 30px) is 50px wide and wraps to two lines; at `left: 10px` it fits.
#[test]
fn fr34_fit_content_width_clamps_to_the_available_width() {
    let cases = [
        (
            "longer than the block",
            r#"<div style="width: 100px"><div id="h">aaa bbbbb cc dd</div></div>"#,
            "#h { width: fit-content; }",
            100.0,
            2.0 * LINE,
        ),
        (
            "shorter than the block",
            r#"<div style="width: 100px"><div id="h">hi</div></div>"#,
            "#h { width: fit-content; }",
            20.0,
            LINE,
        ),
        (
            "a word longer than the block",
            r#"<div style="width: 100px"><div id="h">abcdefghijkl x</div></div>"#,
            "#h { width: fit-content; }",
            120.0,
            2.0 * LINE,
        ),
        (
            "fit-content() limit",
            r#"<div style="width: 300px"><div id="h">aaa bbbbb cc dd</div></div>"#,
            "#h { width: fit-content(90px); }",
            90.0,
            2.0 * LINE,
        ),
        (
            "absolute near the right edge",
            r#"<div style="position: relative; width: 200px; height: 100px"><div id="h">aaa bb</div></div>"#,
            "#h { position: absolute; left: 150px; top: 0; width: fit-content; }",
            50.0,
            2.0 * LINE,
        ),
        (
            "absolute with room",
            r#"<div style="position: relative; width: 200px; height: 100px"><div id="h">aaa bb</div></div>"#,
            "#h { position: absolute; left: 10px; top: 0; width: fit-content; }",
            60.0,
            LINE,
        ),
    ];
    let mut failures = Vec::new();
    for (case, body, css, width, height) in cases {
        let (doc, list) = lay_out(body, css);
        check_size(
            &mut failures,
            case,
            entry(&list, &doc, "h").rect,
            width,
            height,
        );
    }
    assert_no_failures(failures);
}

/// An absolutely positioned box with `width: auto` and only one horizontal inset is
/// shrink-to-fit: as wide as its text, up to the room between its inset and the far edge
/// of the containing block. `aaa bb` is 60px. At `left: 10px` in a 200px box there are
/// 190px, so it is 60px on one line; at `left: 150px` there are 50px, so the box is 50px
/// wide and the text wraps to `aaa` and `bb`. With both insets the width is what is left
/// between them. Block children make it as wide as the widest child's text. A fixed box
/// does the same against the 800px viewport: at `left: 770px` 30px are left, the longest
/// word, so it is 30px. `right: 10px` puts a 60px box at x = 200 - 10 - 60 = 130.
#[test]
fn fr34_an_absolute_box_without_a_width_shrinks_to_fit() {
    let positioned = |inner: &str| {
        format!(r#"<div style="position: relative; width: 200px; height: 100px">{inner}</div>"#)
    };
    let cases = [
        (
            "left inset with room",
            positioned(r#"<div id="h">aaa bb</div>"#),
            "#h { position: absolute; left: 10px; top: 0; }",
            60.0,
            LINE,
        ),
        (
            "right inset with room",
            positioned(r#"<div id="h">aaa bb</div>"#),
            "#h { position: absolute; right: 10px; top: 0; }",
            60.0,
            LINE,
        ),
        (
            "no insets",
            positioned(r#"<div id="h">aaa bb</div>"#),
            "#h { position: absolute; }",
            60.0,
            LINE,
        ),
        (
            "near the right edge",
            positioned(r#"<div id="h">aaa bb</div>"#),
            "#h { position: absolute; left: 150px; top: 0; }",
            50.0,
            2.0 * LINE,
        ),
        (
            "both insets",
            positioned(r#"<div id="h">aaa bb</div>"#),
            "#h { position: absolute; left: 10px; right: 20px; top: 0; }",
            170.0,
            LINE,
        ),
        (
            "with padding",
            positioned(r#"<div id="h">aaa bb</div>"#),
            "#h { position: absolute; left: 10px; top: 0; padding: 2px 6px; }",
            72.0,
            LINE + 4.0,
        ),
        (
            "block children",
            positioned(r#"<div id="h"><div>aaa bb</div><div>cccc</div></div>"#),
            "#h { position: absolute; left: 10px; top: 0; }",
            60.0,
            2.0 * LINE,
        ),
        (
            "a flex column",
            positioned(r#"<div id="h"><div>aaa bb</div><div>cccc</div></div>"#),
            "#h { position: absolute; left: 10px; top: 0; display: flex; flex-direction: column; }",
            60.0,
            2.0 * LINE,
        ),
        (
            "a flex row",
            positioned(r#"<div id="h"><span>aaa</span><span>bb</span></div>"#),
            "#h { position: absolute; left: 10px; top: 0; display: flex; }",
            50.0,
            LINE,
        ),
    ];
    let mut failures = Vec::new();
    for (case, body, css, width, height) in cases {
        let (doc, list) = lay_out(&body, css);
        check_size(
            &mut failures,
            case,
            entry(&list, &doc, "h").rect,
            width,
            height,
        );
    }
    assert_no_failures(failures);

    let (doc, list) = lay_out(
        r#"<div style="width: 100px"><div id="h">aaa bb</div></div>"#,
        "#h { position: fixed; left: 770px; top: 0; }",
    );
    assert_rect(
        "fixed",
        entry(&list, &doc, "h").rect,
        770.0,
        0.0,
        30.0,
        2.0 * LINE,
    );

    let (doc, list) = lay_out(
        &positioned(r#"<div id="h">aaa bb</div>"#),
        "#h { position: absolute; right: 10px; top: 5px; }",
    );
    assert_rect(
        "placed from the right",
        entry(&list, &doc, "h").rect,
        130.0,
        5.0,
        60.0,
        LINE,
    );
}
