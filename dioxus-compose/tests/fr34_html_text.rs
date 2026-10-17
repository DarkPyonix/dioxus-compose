//! Text features of the HTML and CSS path that reach the measurer or the display list:
//! `word-break`, `overflow-wrap`, `text-overflow: ellipsis`, list markers and
//! `text-align: justify`.
//!
//! Every expected number is worked out from the CSS in the fixture. Text is sized by a fake
//! measurer: 10px per character, 20px per line, lines broken greedily at spaces. It breaks
//! inside words the way the CSS asks, so a test can count lines.

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose::html::{
    BaseDocument, DisplayList, NodeEntry, OverflowWrap, ParleyMeasurer, Plan, PlanChange, PlanKey,
    PlanKind, Rect, RunMarker, TextAlign, TextLineHeight, TextMeasureRequest, TextMeasurer,
    TextMetrics, TextStyle, TextWhiteSpace, WidthConstraint, WordBreak, diff, element_by_id,
    layout_document, plan_from,
};

/// Every character is 10px wide and every line 20px tall.
struct FakeMeasurer;

const CHAR: f32 = 10.0;
const LINE: f32 = 20.0;

impl TextMeasurer for FakeMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let max_chars = match request.width {
            WidthConstraint::MaxContent => usize::MAX,
            WidthConstraint::MinContent => 0,
            // The allowance keeps 70 / 10 from coming out as 6.999.
            WidthConstraint::AtMost(width) => (width / CHAR + 1e-3).floor() as usize,
        };
        let break_long_words = match request.style.overflow_wrap {
            OverflowWrap::Anywhere => true,
            OverflowWrap::BreakWord => request.width != WidthConstraint::MinContent,
            OverflowWrap::Normal => false,
        };
        let mut lines: Vec<String> = Vec::new();
        for paragraph in request.text.split('\n') {
            if request.style.word_break == WordBreak::BreakAll {
                let max = max_chars.max(1);
                let mut current = String::new();
                for letter in paragraph.chars() {
                    if current.chars().count() == max {
                        lines.push(std::mem::take(&mut current));
                        if letter == ' ' {
                            continue;
                        }
                    }
                    current.push(letter);
                }
                lines.push(current);
                continue;
            }
            let mut current = String::new();
            let mut started = false;
            for word in paragraph.split(' ') {
                let length = word.chars().count();
                if started && current.chars().count() + 1 + length <= max_chars {
                    current.push(' ');
                    current.push_str(word);
                    continue;
                }
                if started {
                    lines.push(std::mem::take(&mut current));
                }
                started = true;
                if break_long_words && length > max_chars {
                    let letters: Vec<char> = word.chars().collect();
                    let pieces: Vec<&[char]> = letters.chunks(max_chars.max(1)).collect();
                    for (index, piece) in pieces.iter().enumerate() {
                        if index + 1 < pieces.len() {
                            lines.push(piece.iter().collect());
                        } else {
                            current = piece.iter().collect();
                        }
                    }
                } else {
                    current.push_str(word);
                }
            }
            lines.push(current);
        }
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

fn parse(body: &str, extra_css: &str) -> BaseDocument {
    let html = format!(
        "<!DOCTYPE html><html><head><style>html, body {{ margin: 0; }} {extra_css}</style></head><body>{body}</body></html>"
    );
    let config = DocumentConfig {
        viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
        ..DocumentConfig::default()
    };
    HtmlDocument::from_html(&html, config).into_inner()
}

fn laid_out(body: &str, extra_css: &str) -> (BaseDocument, DisplayList) {
    let mut doc = parse(body, extra_css);
    let list = layout_document(&mut doc, &mut FakeMeasurer);
    (doc, list)
}

fn entry<'a>(list: &'a DisplayList, doc: &BaseDocument, id: &str) -> &'a NodeEntry {
    let node = element_by_id(doc, id).unwrap_or_else(|| panic!("no element with id {id}"));
    list.get(node)
        .unwrap_or_else(|| panic!("#{id} (node {node}) has no display list entry"))
}

#[track_caller]
fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: expected {expected}, got {actual}"
    );
}

#[track_caller]
fn assert_rect(actual: Rect, x: f32, y: f32, width: f32, height: f32, what: &str) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(
        close(actual.x, x)
            && close(actual.y, y)
            && close(actual.width, width)
            && close(actual.height, height),
        "{what}: expected ({x}, {y}, {width}, {height}), got ({}, {}, {}, {})",
        actual.x,
        actual.y,
        actual.width,
        actual.height
    );
}

// ------------------------------------------------------------------------------------------
// word-break and overflow-wrap

/// "abcdefghij klm" in a 50px box is 5 characters a line. `word-break: break-all` breaks
/// between any two characters: "abcde", "fghij" (the space that would start the next line
/// hangs) and "klm". That is 3 lines of 20px, 60px tall, the widest 50px. The style the
/// measurer was given says `break-all`.
#[test]
fn fr34_word_break_all_breaks_a_long_word() {
    let (doc, list) = laid_out(
        r#"<div id="box">abcdefghij klm</div>"#,
        "#box { width: 50px; word-break: break-all; }",
    );
    let boxed = entry(&list, &doc, "box");
    assert_rect(boxed.rect, 0.0, 0.0, 50.0, 60.0, "box");
    let run = &boxed.texts[0];
    assert_eq!(run.style.word_break, WordBreak::BreakAll);
    assert_eq!(run.line_count, 3);
    assert_rect(run.rect, 0.0, 0.0, 50.0, 60.0, "run");
}

/// The same text with the default `word-break` is not broken inside a word: "abcdefghij"
/// does not fit 50px, so it keeps its line and overflows to 100px, and "klm" follows on a
/// second line. `keep-all` breaks only at spaces too, for text without CJK.
#[test]
fn fr34_word_break_normal_lets_a_long_word_overflow() {
    for (class, expected) in [("", WordBreak::Normal), ("keep", WordBreak::KeepAll)] {
        let (doc, list) = laid_out(
            &format!(r#"<div id="box" class="{class}">abcdefghij klm</div>"#),
            "#box { width: 50px; } .keep { word-break: keep-all; }",
        );
        let run = &entry(&list, &doc, "box").texts[0];
        assert_eq!(run.style.word_break, expected, "{class}");
        assert_eq!(run.line_count, 2, "{class}");
        assert_rect(run.rect, 0.0, 0.0, 100.0, 40.0, class);
    }
}

/// "aaa bbbbbbbbbb" in a 70px box is 7 characters a line. The long word does not fit a
/// line of its own, so `overflow-wrap: anywhere`, `overflow-wrap: break-word` and its alias
/// `word-wrap: break-word` break it, after the space: "aaa", "bbbbbbb" and "bbb", three
/// lines, 60px tall and 70px wide. With the default `overflow-wrap` it keeps its line, 100px
/// wide, on two lines.
#[test]
fn fr34_overflow_wrap_breaks_a_word_that_cannot_fit() {
    let (doc, list) = laid_out(
        r#"<div id="normal">aaa bbbbbbbbbb</div>
<div id="anywhere">aaa bbbbbbbbbb</div>
<div id="word">aaa bbbbbbbbbb</div>
<div id="alias">aaa bbbbbbbbbb</div>"#,
        "div { width: 70px; }
#anywhere { overflow-wrap: anywhere; }
#word { overflow-wrap: break-word; }
#alias { word-wrap: break-word; }",
    );
    let normal = &entry(&list, &doc, "normal").texts[0];
    assert_eq!(normal.style.overflow_wrap, OverflowWrap::Normal);
    assert_eq!(normal.line_count, 2);
    assert_rect(normal.rect, 0.0, 0.0, 100.0, 40.0, "normal");

    for (id, expected) in [
        ("anywhere", OverflowWrap::Anywhere),
        ("word", OverflowWrap::BreakWord),
        ("alias", OverflowWrap::BreakWord),
    ] {
        let boxed = entry(&list, &doc, id);
        let run = &boxed.texts[0];
        assert_eq!(run.style.overflow_wrap, expected, "#{id}");
        assert_eq!(run.line_count, 3, "#{id}");
        assert_close(run.rect.width, 70.0, &format!("#{id} widest line"));
        assert_close(boxed.rect.height, 60.0, &format!("#{id} height"));
    }
}

/// A flex item's width cannot go below its min-content width. The item holds the 10
/// characters "abcdefghij" in a 30px row. With the default `overflow-wrap` and with
/// `break-word` the min-content width is the whole word, 100px, so the item keeps 100px and
/// overflows the row. With `anywhere` every character is a break point, the min-content
/// width is one character, and the item shrinks to the row's 30px: 3 characters a line,
/// "abc", "def", "ghi", "j", four lines of 20px.
#[test]
fn fr34_overflow_wrap_anywhere_lowers_the_min_content_width() {
    let (doc, list) = laid_out(
        r#"<div class="row"><div id="normal">abcdefghij</div></div>
<div class="row"><div id="word" style="overflow-wrap: break-word">abcdefghij</div></div>
<div class="row"><div id="anywhere" style="overflow-wrap: anywhere">abcdefghij</div></div>"#,
        ".row { display: flex; width: 30px; }",
    );
    assert_close(
        entry(&list, &doc, "normal").rect.width,
        100.0,
        "normal item",
    );
    assert_close(
        entry(&list, &doc, "word").rect.width,
        100.0,
        "break-word item",
    );
    let anywhere = entry(&list, &doc, "anywhere");
    assert_rect(
        anywhere.rect,
        0.0,
        anywhere.rect.y,
        30.0,
        80.0,
        "anywhere item",
    );
    assert_eq!(anywhere.texts[0].line_count, 4);
}

/// The default measurer hands the two properties to Parley. The word has no space, so by
/// default it is one line however narrow the box is; under `break-all`, `anywhere` and
/// `break-word` it takes more than one line when the box is half as wide as the word, and
/// `keep-all` leaves it whole. Widths depend on the font the machine has, so the test
/// takes the word's own width from the measurer and halves it.
#[test]
fn fr34_parley_measurer_honours_word_break_and_overflow_wrap() {
    let mut measurer = ParleyMeasurer::new();
    let style = |word_break, overflow_wrap| TextStyle {
        font_family: vec!["sans-serif".to_string()],
        font_size: 16.0,
        font_weight: 400.0,
        italic: false,
        letter_spacing: 0.0,
        line_height: TextLineHeight::Px(20.0),
        word_break,
        overflow_wrap,
    };
    let text = "Supercalifragilisticexpialidocious";
    let mut lines = |style: &TextStyle, width: WidthConstraint| {
        measurer
            .measure(&TextMeasureRequest {
                text,
                style,
                width,
                white_space: TextWhiteSpace::NORMAL,
            })
            .line_count
    };

    let plain = style(WordBreak::Normal, OverflowWrap::Normal);
    let natural = {
        let metrics = ParleyMeasurer::new().measure(&TextMeasureRequest {
            text,
            style: &plain,
            width: WidthConstraint::MaxContent,
            white_space: TextWhiteSpace::NORMAL,
        });
        metrics.width
    };
    assert!(
        natural > 0.0,
        "the machine has no font for sans-serif, so nothing can be measured"
    );
    let narrow = WidthConstraint::AtMost(natural / 2.0);

    assert_eq!(lines(&plain, narrow), 1, "default");
    assert_eq!(
        lines(&style(WordBreak::KeepAll, OverflowWrap::Normal), narrow),
        1
    );
    assert!(lines(&style(WordBreak::BreakAll, OverflowWrap::Normal), narrow) >= 2);
    assert!(lines(&style(WordBreak::Normal, OverflowWrap::Anywhere), narrow) >= 2);
    assert!(lines(&style(WordBreak::Normal, OverflowWrap::BreakWord), narrow) >= 2);
}

// ------------------------------------------------------------------------------------------
// text-overflow: ellipsis

const ELLIPSIS_CSS: &str =
    "#box { width: 100px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }";

/// 16 characters are 160px on one line, and the box is 100px wide with `overflow: hidden`
/// and `text-overflow: ellipsis`: the run asks for an ellipsis at 100px. Its own rectangle
/// is still the whole 160px, and the box clips it.
#[test]
fn fr34_ellipsis_is_carried_to_the_run() {
    let (doc, list) = laid_out(r#"<div id="box">abcdefghijklmnop</div>"#, ELLIPSIS_CSS);
    let run = &entry(&list, &doc, "box").texts[0];
    assert_eq!(run.ellipsis, Some(100.0));
    assert_rect(run.rect, 0.0, 0.0, 160.0, 20.0, "run");
    assert_eq!(run.line_count, 1);
    assert_eq!(run.wrap_width, None);
}

/// Each of these leaves the text alone: text that fits ("abcde" is 50px in 100px), a box
/// that does not clip (`overflow: visible`), and `text-overflow: clip`.
#[test]
fn fr34_ellipsis_applies_only_when_the_text_is_cut() {
    let (doc, list) = laid_out(
        r#"<div id="fits" class="e">abcde</div>
<div id="visible" class="e">abcdefghijklmnop</div>
<div id="clip" class="e">abcdefghijklmnop</div>"#,
        ".e { width: 100px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
#visible { overflow: visible; }
#clip { text-overflow: clip; }",
    );
    for id in ["fits", "visible", "clip"] {
        assert_eq!(entry(&list, &doc, id).texts[0].ellipsis, None, "#{id}");
    }
}

/// The plan's text node says so, with the width to cut at, and a change of box width that
/// turns the cut on is reported as a change of the text node's kind. Box 200px: the 160px
/// text fits and there is no ellipsis. Box 100px: it is cut at 100px.
#[test]
fn fr34_ellipsis_is_carried_to_the_plan() {
    let css = "#box { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }";
    let plan_for = |width: u32| -> (BaseDocument, Plan) {
        let (doc, list) = laid_out(
            r#"<div id="box">abcdefghijklmnop</div>"#,
            &format!("{css} #box {{ width: {width}px; }}"),
        );
        let plan = plan_from(&list);
        (doc, plan)
    };
    let (doc, wide) = plan_for(200);
    let (_, narrow) = plan_for(100);
    let node = element_by_id(&doc, "box").unwrap();
    let text_key = PlanKey::Text { node, index: 0 };

    let text_of = |plan: &Plan| match &plan.find(text_key).expect("the text node").kind {
        PlanKind::Text(text) => text.clone(),
        other => panic!("a text run is a Text: {other:?}"),
    };
    assert_eq!(text_of(&wide).ellipsis, None);
    assert_eq!(text_of(&narrow).ellipsis, Some(100.0));

    let changes = diff(&wide, &narrow);
    assert!(
        changes.iter().any(|change| matches!(
            change,
            PlanChange::KindChanged { key, kind: PlanKind::Text(text) }
                if *key == text_key && text.ellipsis == Some(100.0)
        )),
        "the cut is reported: {changes:#?}"
    );
}

// ------------------------------------------------------------------------------------------
// list markers

/// The markers are runs of their own, before the item's text. An outside marker ends one
/// space (10px) before the item's border box. The list has 40px of padding, so its items
/// start at x = 40:
///
/// - "3." is 2 characters, 20px, so it starts at 40 - 10 - 20 = 10;
/// - "10." is 3 characters, 30px, so it starts at 0.
///
/// The items count from `start="3"`, and the `value="10"` on the third item restarts the
/// count: 3, 4, 10, 11. Each marker sits on its item's first baseline: the same y, 20px
/// tall.
#[test]
fn fr34_ordered_list_markers_count_from_start() {
    let (doc, list) = laid_out(
        r#"<ol id="list" start="3"><li id="a">x</li><li id="b">y</li><li id="c" value="10">z</li><li id="d">w</li></ol>"#,
        "ol { margin: 0; padding: 0 0 0 40px; }",
    );
    for (id, marker, x, width, y) in [
        ("a", "3.", 10.0, 20.0, 0.0),
        ("b", "4.", 10.0, 20.0, 20.0),
        ("c", "10.", 0.0, 30.0, 40.0),
        ("d", "11.", 0.0, 30.0, 60.0),
    ] {
        let item = entry(&list, &doc, id);
        assert_eq!(item.texts.len(), 2, "#{id}: the marker and the text");
        let run = &item.texts[0];
        assert_eq!(run.marker, RunMarker::Marker, "#{id}");
        assert_eq!(run.text, marker, "#{id}");
        assert_eq!(run.owner, element_by_id(&doc, id).unwrap(), "#{id}");
        assert_rect(run.rect, x, y, width, 20.0, &format!("#{id} marker"));
        assert_eq!(item.texts[1].marker, RunMarker::None);
        assert_close(item.texts[1].rect.x, 40.0, &format!("#{id} text x"));
    }
}

/// `reversed` counts down from the number of items (3, 2, 1), and from `start` when there
/// is one (5, 4, 3).
#[test]
fn fr34_reversed_list_markers_count_down() {
    let (doc, list) = laid_out(
        r#"<ol><li id="a">x</li><li id="b">x</li><li id="c">x</li></ol>
<ol reversed><li id="r1">x</li><li id="r2">x</li><li id="r3">x</li></ol>
<ol reversed start="5"><li id="s1">x</li><li id="s2">x</li><li id="s3">x</li></ol>"#,
        "ol { margin: 0; padding: 0 0 0 40px; }",
    );
    let marker = |id: &str| entry(&list, &doc, id).texts[0].text.clone();
    assert_eq!([marker("a"), marker("b"), marker("c")], ["1.", "2.", "3."]);
    assert_eq!(
        [marker("r1"), marker("r2"), marker("r3")],
        ["3.", "2.", "1."]
    );
    assert_eq!(
        [marker("s1"), marker("s2"), marker("s3")],
        ["5.", "4.", "3."]
    );
}

/// Each `list-style-type` writes its own marker: the bullets as one character, `decimal`
/// as "1.", the alphabetic types as letters. `lower-alpha` from `start="27"` goes past z
/// to "aa." and "ab."; `upper-alpha` from 26 gives "Z." and "AA.". `none` has no marker,
/// so the item draws only its text.
#[test]
fn fr34_list_style_type_chooses_the_marker() {
    let (doc, list) = laid_out(
        r#"<ul class="disc"><li id="disc">x</li></ul>
<ul class="circle"><li id="circle">x</li></ul>
<ul class="square"><li id="square">x</li></ul>
<ol class="decimal"><li id="decimal">x</li></ol>
<ol class="lower" start="27"><li id="lower-1">x</li><li id="lower-2">x</li></ol>
<ol class="upper" start="26"><li id="upper-1">x</li><li id="upper-2">x</li></ol>
<ul class="none"><li id="none">x</li></ul>"#,
        "ul, ol { margin: 0; padding: 0 0 0 40px; }
.disc { list-style-type: disc; }
.circle { list-style-type: circle; }
.square { list-style-type: square; }
.decimal { list-style-type: decimal; }
.lower { list-style-type: lower-alpha; }
.upper { list-style-type: upper-alpha; }
.none { list-style-type: none; }",
    );
    let marker = |id: &str| entry(&list, &doc, id).texts[0].text.clone();
    assert_eq!(marker("disc"), "\u{2022}");
    assert_eq!(marker("circle"), "\u{25e6}");
    assert_eq!(marker("square"), "\u{25aa}");
    assert_eq!(marker("decimal"), "1.");
    assert_eq!(marker("lower-1"), "aa.");
    assert_eq!(marker("lower-2"), "ab.");
    assert_eq!(marker("upper-1"), "Z.");
    assert_eq!(marker("upper-2"), "AA.");

    let none = entry(&list, &doc, "none");
    assert_eq!(none.texts.len(), 1);
    assert_eq!(none.texts[0].text, "x");
    assert_eq!(none.texts[0].marker, RunMarker::None);
}

/// A marker inside the item is the start of its first line. "x" after the bullet and a
/// space is "\u{2022} x", 3 characters, 30px, in one run whose first 4 bytes (the bullet is
/// 3 bytes in UTF-8, then the space) are the marker. The item draws one run, at its own
/// content origin. `list-style-position: outside` draws two.
#[test]
fn fr34_inside_marker_starts_the_text() {
    let (doc, list) = laid_out(
        r#"<ul id="inside"><li id="in">x</li></ul><ul id="outside"><li id="out">x</li></ul>"#,
        "ul { margin: 0; padding: 0 0 0 40px; }
#inside { list-style-position: inside; }",
    );
    let inside = entry(&list, &doc, "in");
    assert_eq!(inside.texts.len(), 1);
    let run = &inside.texts[0];
    assert_eq!(run.text, "\u{2022} x");
    assert_eq!(run.marker, RunMarker::Prefix { len: 4 });
    assert_rect(run.rect, 40.0, 0.0, 30.0, 20.0, "inside run");

    assert_eq!(entry(&list, &doc, "out").texts.len(), 2);
}

/// A `display: list-item` element that is not in an `ol` or `ul` has a marker too, and the
/// items of one parent count together: 1, 2.
#[test]
fn fr34_display_list_item_gets_a_marker() {
    let (doc, list) = laid_out(
        r#"<div id="parent"><div id="a" class="item">x</div><div id="b" class="item">x</div></div>"#,
        "#parent { margin-left: 40px; } .item { display: list-item; list-style-type: decimal; }",
    );
    for (id, marker) in [("a", "1."), ("b", "2.")] {
        let item = entry(&list, &doc, id);
        assert_eq!(item.texts.len(), 2, "#{id}");
        assert_eq!(item.texts[0].text, marker, "#{id}");
        assert_eq!(item.texts[0].marker, RunMarker::Marker, "#{id}");
        // Two characters, one space of gap, ending where the item's box starts (x = 40).
        assert_close(item.texts[0].rect.x, 10.0, &format!("#{id} marker x"));
    }
}

/// The marker is in the plan as a text node before the item's own text, and a plan text
/// says whether it is a marker.
#[test]
fn fr34_marker_is_carried_to_the_plan() {
    let (doc, list) = laid_out(
        r#"<ul id="list"><li id="a">x</li></ul>"#,
        "ul { margin: 0; padding: 0 0 0 40px; }",
    );
    let plan = plan_from(&list);
    let node = element_by_id(&doc, "a").unwrap();
    let kind_of = |index| match &plan
        .find(PlanKey::Text { node, index })
        .expect("the text node")
        .kind
    {
        PlanKind::Text(text) => text.clone(),
        other => panic!("a text run is a Text: {other:?}"),
    };
    assert_eq!(kind_of(0).marker, RunMarker::Marker);
    assert_eq!(kind_of(0).text, "\u{2022}");
    assert_eq!(kind_of(1).marker, RunMarker::None);
    assert_eq!(kind_of(1).text, "x");
}

// ------------------------------------------------------------------------------------------
// text-align: justify

/// "aaa bbb ccc ddd" in a 100px box is 10 characters a line: "aaa bbb" (70px) and "ccc ddd".
/// Left aligned, the widest line is 70px. Justified, the first line is stretched to the
/// box's 100px, so the run is 100px wide, and it says `Justify`; the plan node is 100px
/// wide and wraps. The last line is not stretched, and neither is a paragraph of one line:
/// "aaa bbb" stays 70px.
#[test]
fn fr34_justified_lines_fill_the_width() {
    let (doc, list) = laid_out(
        r#"<div id="left">aaa bbb ccc ddd</div>
<div id="justify">aaa bbb ccc ddd</div>
<div id="single">aaa bbb</div>"#,
        "div { width: 100px; } #justify, #single { text-align: justify; }",
    );
    let left = &entry(&list, &doc, "left").texts[0];
    assert_ne!(left.align, TextAlign::Justify);
    assert_close(left.rect.width, 70.0, "left widest line");

    let justified = &entry(&list, &doc, "justify").texts[0];
    assert_eq!(justified.align, TextAlign::Justify);
    assert_eq!(justified.line_count, 2);
    assert_eq!(justified.wrap_width, Some(100.0));
    assert_rect(
        justified.rect,
        0.0,
        justified.rect.y,
        100.0,
        40.0,
        "justified",
    );

    let single = &entry(&list, &doc, "single").texts[0];
    assert_eq!(single.align, TextAlign::Justify);
    assert_eq!(single.line_count, 1);
    assert_close(single.rect.width, 70.0, "a single line is not stretched");

    let plan = plan_from(&list);
    let node = element_by_id(&doc, "justify").unwrap();
    let text = plan
        .find(PlanKey::Text { node, index: 0 })
        .expect("the text node");
    let PlanKind::Text(plan_text) = &text.kind else {
        panic!("a text run is a Text: {text:#?}");
    };
    assert_eq!(plan_text.align, TextAlign::Justify);
    assert!(plan_text.soft_wrap);
}
