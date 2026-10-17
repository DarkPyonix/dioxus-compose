//! The typography example laid out in an 800 by 4000 viewport, tall enough that nothing
//! scrolls.
//!
//! Every number follows from `typography::STYLE` and the support measurer (an advance of
//! half the font size per character plus any letter spacing, the CSS line height per line,
//! lines broken greedily at spaces and at every `\n`).
//!
//! `.page` has 32px of padding, so everything starts at x = 32 and a full-width block is
//! 800 - 64 = 736px wide. Vertical positions depend on every section above, so the tests
//! compare heights and the distances between neighbours, which the CSS fixes on its own.
//!
//! Some of the features on the page are not laid out by the layout pass at all (the end of
//! this file says which and the tests that would defend them are not here, since a test of
//! "nothing happens" would pin the gap in place).

use dioxus_compose::html::{HtmlDom, TextAlign, TextLineHeight, TextRun};
use sample_html_support::{
    Measurer, assert_close, assert_rect, centre, config, entry, node, text_of,
};
use sample_html_typography as typography;

fn laid_out() -> HtmlDom {
    let mut dom = HtmlDom::with_config(typography::app, config(typography::STYLE, Measurer::new()));
    dom.layout(800.0, 4000.0, 1.0);
    dom
}

/// The one run of text the element with `id` draws.
#[track_caller]
fn run(dom: &HtmlDom, id: &str) -> TextRun {
    let texts = &entry(dom, id).texts;
    assert_eq!(texts.len(), 1, "#{id} draws one run: {texts:?}");
    texts[0].clone()
}

/// "Hamburgefonstiv" is 15 characters, so a run is 15 * size / 2 wide, and `line-height:
/// normal` makes the line 1.25 times the size: 12px gives 90 x 15, 16px 120 x 20, 20px
/// 150 x 25, 24px 180 x 30, 32px 240 x 40 and 40px 300 x 50. The blocks have no margins,
/// so each starts where the one above ends.
#[test]
fn fr34_typography_scale_text_follows_font_size() {
    let dom = laid_out();
    let mut previous_bottom = None;
    for (size, width, height) in [
        (12.0, 90.0, 15.0),
        (16.0, 120.0, 20.0),
        (20.0, 150.0, 25.0),
        (24.0, 180.0, 30.0),
        (32.0, 240.0, 40.0),
        (40.0, 300.0, 50.0),
    ] {
        let id = format!("scale-{size}");
        let block = entry(&dom, &id).rect;
        assert_close(block.x, 32.0, &format!("#{id} x"));
        assert_close(block.width, 736.0, &format!("#{id} width"));
        assert_close(block.height, height, &format!("#{id} height"));
        if let Some(bottom) = previous_bottom {
            assert_close(block.y, bottom, &format!("#{id} starts below the last one"));
        }
        previous_bottom = Some(block.y + block.height);

        let text = run(&dom, &id);
        assert_eq!(text.style.font_size, size, "#{id}");
        assert_close(text.rect.width, width, &format!("#{id} text width"));
        assert_eq!(text.line_count, 1, "#{id}");
    }
}

/// Weight and style do not change the measurer's advance, but they reach the run, which
/// is what the renderer draws with.
#[test]
fn fr34_typography_weight_and_style_reach_the_run() {
    let dom = laid_out();
    assert_eq!(run(&dom, "weight-300").style.font_weight, 300.0);
    assert_eq!(run(&dom, "weight-400").style.font_weight, 400.0);
    assert_eq!(run(&dom, "weight-700").style.font_weight, 700.0);
    assert!(!run(&dom, "weight-700").style.italic);
    assert!(run(&dom, "italic").style.italic);
    // "Bold 700" is 8 characters at 8px, whatever the weight.
    assert_close(run(&dom, "weight-700").rect.width, 64.0, "bold width");
}

/// "aaa bbb ccc ddd eee fff" at 16px is 8px a character in a 120px box, so 15 characters
/// fit: "aaa bbb ccc ddd" and "eee fff", two lines whatever the line height. The box is as
/// tall as two lines: unitless 1.5 is 24px a line (48), `30px` is 60, and `normal` is
/// 1.25 * 16 = 20px a line (40).
#[test]
fn fr34_typography_line_height_sets_the_line_pitch() {
    let dom = laid_out();
    for (id, line, height) in [
        ("lh-unitless", 24.0, 48.0),
        ("lh-px", 30.0, 60.0),
        ("lh-normal", 20.0, 40.0),
    ] {
        let block = entry(&dom, id).rect;
        assert_rect(block, 32.0, block.y, 120.0, height, &format!("#{id}"));
        let text = run(&dom, id);
        assert_eq!(text.line_count, 2, "#{id}");
        assert_close(text.rect.height, height, &format!("#{id} text height"));
        assert_close(text.rect.width, 120.0, &format!("#{id} widest line"));
        if id != "lh-normal" {
            assert_eq!(text.style.line_height, TextLineHeight::Px(line), "#{id}");
        } else {
            assert_eq!(text.style.line_height, TextLineHeight::Normal);
        }
    }
}

/// Letter spacing adds to every character's advance. At 0 the advance is 8px and 15
/// characters fit in 120px: two lines, the first 120px wide. At `2px` it is 10px, so 12
/// characters fit: "aaa bbb ccc" (11 characters, 110px) and "ddd eee fff". At `0.25em`,
/// which is 4px at 16px, it is 12px, so 10 characters fit: "aaa bbb", "ccc ddd" and "eee
/// fff", three lines of 7 characters (84px).
#[test]
fn fr34_typography_letter_spacing_widens_the_advance() {
    let dom = laid_out();
    for (id, spacing, lines, widest) in [
        ("ls-zero", 0.0, 2, 120.0),
        ("ls-px", 2.0, 2, 110.0),
        ("ls-em", 4.0, 3, 84.0),
    ] {
        let text = run(&dom, id);
        assert_eq!(text.style.letter_spacing, spacing, "#{id} spacing");
        assert_eq!(text.line_count, lines, "#{id} lines");
        assert_close(text.rect.width, widest, &format!("#{id} widest line"));
        assert_close(
            text.rect.height,
            lines as f32 * 20.0,
            &format!("#{id} height"),
        );
    }
}

/// "Align me" is 8 characters, 64px, in a 240px box that starts at x = 32. Left keeps the
/// line at 32. Centre moves it by half the 176px that is left over, to 32 + 88 = 120.
/// Right moves it by all of it, to 32 + 176 = 208. The justified paragraph breaks at 30
/// characters into "Justify spreads the words of", "every line across the full" and
/// "width": three 20px lines, the widest 28 characters (224px). The layout pass leaves the
/// stretching of the first two lines to the renderer, so the run stays at x = 32 and says
/// `Justify`.
#[test]
fn fr34_typography_align_places_the_line() {
    let dom = laid_out();
    for (id, x, align) in [
        ("align-left", 32.0, TextAlign::Left),
        ("align-center", 120.0, TextAlign::Center),
        ("align-right", 208.0, TextAlign::Right),
    ] {
        let text = run(&dom, id);
        assert_eq!(text.align, align, "#{id}");
        assert_close(text.rect.x, x, &format!("#{id} text x"));
        assert_close(text.rect.width, 64.0, &format!("#{id} text width"));
        assert_close(entry(&dom, id).rect.width, 240.0, &format!("#{id} box"));
    }

    let justified = run(&dom, "align-justify");
    assert_eq!(justified.align, TextAlign::Justify);
    assert_eq!(justified.line_count, 3);
    assert_eq!(justified.wrap_width, Some(240.0));
    assert_close(justified.rect.x, 32.0, "justified x");
    assert_close(justified.rect.width, 224.0, "justified widest line");
    assert_close(justified.rect.height, 60.0, "justified height");
}

/// The measurer is asked about the text as drawn: "type specimen" is "TYPE SPECIMEN" under
/// `uppercase` and "Type Specimen" under `capitalize`, 13 characters (104px) either way.
#[test]
fn fr34_typography_transform_changes_the_drawn_text() {
    let dom = laid_out();
    let upper = run(&dom, "tt-upper");
    assert_eq!(upper.text, "TYPE SPECIMEN");
    assert_close(upper.rect.width, 104.0, "uppercase width");
    let capitalized = run(&dom, "tt-capitalize");
    assert_eq!(capitalized.text, "Type Specimen");
    assert_close(capitalized.rect.width, 104.0, "capitalize width");
}

/// "alpha beta gamma delta epsilon zeta" is 35 characters, 8px each. In a 160px box 20
/// fit, so `normal` breaks it into "alpha beta gamma" (16) and "delta epsilon zeta" (18):
/// two lines, 144px wide at the widest, wrapped at 160. `nowrap` never breaks: one 20px
/// line 35 * 8 = 280px wide, wider than its 160px box, with no wrap width.
#[test]
fn fr34_typography_nowrap_keeps_one_line() {
    let dom = laid_out();

    let normal = run(&dom, "ws-normal");
    assert_eq!(normal.line_count, 2);
    assert_eq!(normal.wrap_width, Some(160.0));
    assert_close(normal.rect.width, 144.0, "normal widest line");
    assert_close(normal.rect.height, 40.0, "normal height");

    let nowrap = run(&dom, "ws-nowrap");
    assert_eq!(nowrap.line_count, 1);
    assert_eq!(nowrap.wrap_width, None);
    assert_close(nowrap.rect.width, 280.0, "nowrap line");
    assert_close(nowrap.rect.height, 20.0, "nowrap height");
    assert_close(entry(&dom, "ws-nowrap").rect.width, 160.0, "nowrap box");
}

/// `pre` keeps the source's breaks and its spaces: "first line", "    indented line"
/// (4 spaces and 13 characters is 17, 136px) and "last" are three lines, and nothing
/// wraps in a 160px box. `pre-wrap` keeps the break too but also wraps at the box:
/// "alpha beta gamma delta epsilon" breaks into "alpha beta gamma" (16) and "delta
/// epsilon" (13), then "zeta" after the newline: three lines, 128px at the widest.
#[test]
fn fr34_typography_pre_keeps_breaks_and_spaces() {
    let dom = laid_out();

    let pre = run(&dom, "ws-pre");
    assert_eq!(pre.text, "first line\n    indented line\nlast");
    assert_eq!(pre.line_count, 3);
    assert_eq!(pre.wrap_width, None);
    assert_close(pre.rect.width, 136.0, "pre widest line");
    assert_close(pre.rect.height, 60.0, "pre height");

    let wrapped = run(&dom, "ws-pre-wrap");
    assert_eq!(wrapped.line_count, 3);
    assert_eq!(wrapped.wrap_width, Some(160.0));
    assert_close(wrapped.rect.width, 128.0, "pre-wrap widest line");
    assert_close(wrapped.rect.height, 60.0, "pre-wrap height");
}

/// The ellipsis box is 160px wide with `overflow: hidden`, and its 40 characters
/// (320px) stay on one line: the box keeps its width and its one line's height, and the
/// run is the full 320px, which the box clips.
#[test]
fn fr34_typography_ellipsis_box_keeps_its_width() {
    let dom = laid_out();
    let block = entry(&dom, "ellipsis").rect;
    assert_close(block.x, 32.0, "box x");
    assert_close(block.width, 160.0, "box width");
    assert_close(block.height, 20.0, "box height");

    let text = run(&dom, "ellipsis");
    assert_eq!(text.line_count, 1);
    assert_eq!(text.wrap_width, None);
    assert_close(text.rect.x, 32.0, "text x");
    assert_close(text.rect.width, 320.0, "text width");
}

/// `::before` and `::after` put "[" and "]" around "note": the run is "[note]", 6
/// characters, 48px, on one line.
#[test]
fn fr34_typography_generated_content_joins_the_text() {
    let dom = laid_out();
    let tag = entry(&dom, "tag");
    assert_eq!(text_of(tag), "[note]");
    let text = run(&dom, "tag");
    assert_eq!(text.line_count, 1);
    assert_close(text.rect.width, 48.0, "generated text width");
    assert_close(text.rect.height, 20.0, "generated text height");
}

/// The lists are 240px wide with 40px of padding on the left, so their items start at
/// x = 32 + 40 = 72 and are 200px wide. An item is one 20px line with no margin, so the
/// items follow one another every 20px, and a list is as tall as its items: 60px for
/// three, 40px for two. The markers themselves are not part of the layout.
#[test]
fn fr34_typography_list_items_stack() {
    let dom = laid_out();
    for (list, prefix, items) in [
        ("list-square", "square", 3),
        ("list-decimal", "decimal", 3),
        ("list-alpha", "alpha", 2),
        ("list-roman", "roman", 2),
    ] {
        let list_box = entry(&dom, list).rect;
        assert_close(list_box.x, 32.0, &format!("#{list} x"));
        assert_close(list_box.width, 240.0, &format!("#{list} width"));
        assert_close(
            list_box.height,
            items as f32 * 20.0,
            &format!("#{list} height"),
        );
        for index in 1..=items {
            let item = entry(&dom, &format!("{prefix}-{index}")).rect;
            assert_rect(
                item,
                72.0,
                list_box.y + (index - 1) as f32 * 20.0,
                200.0,
                20.0,
                &format!("#{prefix}-{index}"),
            );
        }
    }
}

/// Underline and line-through do not change a box: "Underlined text" is 15 characters
/// (120px) and "Struck text" 11 (88px), each one 20px line.
#[test]
fn fr34_typography_decoration_leaves_the_box_alone() {
    let dom = laid_out();
    let underlined = run(&dom, "deco-underline");
    assert_close(underlined.rect.width, 120.0, "underlined width");
    assert_close(underlined.rect.height, 20.0, "underlined height");
    let struck = run(&dom, "deco-strike");
    assert_close(struck.rect.width, 88.0, "struck width");
    assert_close(struck.rect.height, 20.0, "struck height");
}

/// The specimen is 200px wide with `line-height: 1.5`. "Pack my box with five dozen
/// liquor jugs" is 39 characters, and the size decides how it wraps:
///
/// - medium, 16px: 8px a character, 25 fit. "Pack my box with five" (21, 168px) and
///   "dozen liquor jugs": 2 lines of 24px, 48px.
/// - large, 20px: 10px a character, 20 fit. "Pack my box with" (16), "five dozen
///   liquor" (17, 170px) and "jugs": 3 lines of 30px, 90px.
/// - small, 14px: 7px a character, 28 fit. "Pack my box with five dozen" (27, 189px) and
///   "liquor jugs": 2 lines of 21px, 42px.
///
/// The button starts at medium and each click moves on one step, around to medium again.
#[test]
fn fr34_typography_size_button_cycles_the_specimen() {
    let mut dom = laid_out();
    let button = node(&dom, "size-button");

    let specimen = run(&dom, "specimen");
    assert_eq!(specimen.style.font_size, 16.0);
    assert_eq!(specimen.line_count, 2);
    assert_close(specimen.rect.width, 168.0, "medium widest line");
    assert_close(entry(&dom, "specimen").rect.height, 48.0, "medium height");
    assert!(text_of(entry(&dom, "size-button")).contains("medium"));

    for (name, size, lines, widest, height) in [
        ("large", 20.0, 3, 170.0, 90.0),
        ("small", 14.0, 2, 189.0, 42.0),
        ("medium", 16.0, 2, 168.0, 48.0),
    ] {
        let rect = entry(&dom, "size-button").rect;
        let (x, y) = centre(rect);
        assert_eq!(dom.click(x, y), Some(button));
        dom.render();
        dom.layout(800.0, 4000.0, 1.0);

        let specimen = run(&dom, "specimen");
        assert_eq!(specimen.style.font_size, size, "{name}");
        assert_eq!(specimen.line_count, lines, "{name}");
        assert_close(specimen.rect.width, widest, &format!("{name} widest line"));
        assert_close(
            entry(&dom, "specimen").rect.height,
            height,
            &format!("{name} height"),
        );
        assert!(
            text_of(entry(&dom, "size-button")).contains(name),
            "the label names the size {name}: {:?}",
            entry(&dom, "size-button").texts
        );
    }
}

// Not defended by a test, because the layout pass does not act on them:
//
// - `word-break` and `overflow-wrap`: neither reaches the measurer's request, so a long
//   word in a narrow box is measured as if it could not break.
// - `text-overflow: ellipsis`: the run is the full text, and the display list carries
//   nothing that says to cut it short.
// - `text-decoration`: the display list carries no decoration for a run.
// - list markers: the display list carries no marker for a list item.
