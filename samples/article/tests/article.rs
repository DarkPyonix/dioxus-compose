//! The article example laid out at two widths.
//!
//! Every number follows from `article::STYLE` and the support measurer (an advance of half
//! the font size per character, the CSS line height per line, greedy breaks at spaces).
//!
//! At 480px the article is narrower than its 640px `max-width`, so it fills the viewport:
//! with `box-sizing: border-box` and 24px side padding its text column starts at x = 24
//! and is 432px wide. At 800px it is 640px wide, centred at x = 80, and its column starts
//! at x = 104 and is 592px wide.
//!
//! Down the 480px page: 32px of padding, the kicker (one 16px line) and 8px, the headline
//! (two 40px lines, below) and 8px, the byline (one 20px line). The byline's 32px bottom
//! margin leaves the `header` and collapses with the `h2`'s 32px top margin, so the `h2`
//! is at 32 + 16 + 8 + 80 + 8 + 20 + 32 = 196. It is one 32px line with a 12px margin, so
//! the first paragraph starts at y = 240.

use dioxus_compose::html::{HtmlDom, TextWhiteSpace, WidthConstraint};
use sample_html_article as article;
use sample_html_support::{Measurer, Request, assert_rect, config, entry, node};

const FIRST: &str = "A Dioxus app written with HTML tags and CSS does not need a browser to be \
                     laid out. The Host runs the same style and layout engines a browser \
                     would, and hands the renderer nothing but rectangles, colours and runs of \
                     text.";

fn laid_out(width: f32) -> (HtmlDom, std::rc::Rc<std::cell::RefCell<Vec<Request>>>) {
    let measurer = Measurer::new();
    let requests = measurer.requests();
    let mut dom = HtmlDom::with_config(article::app, config(article::STYLE, measurer));
    dom.layout(width, 2000.0, 1.0);
    (dom, requests)
}

/// The first paragraph at 16px is 8px a character. In the 432px column that is 54
/// characters a line, and the greedy breaks are
///
/// ```text
/// A Dioxus app written with HTML tags and CSS does not   (52)
/// need a browser to be laid out. The Host runs the same  (53)
/// style and layout engines a browser would, and hands    (51)
/// the renderer nothing but rectangles, colours and runs  (53)
/// of text.
/// ```
///
/// five 24px lines, 120px. In the 592px column (74 characters) it is three lines, 72px.
/// The headline at 32px is 16px a character, 27 to a 432px line: "Laying out a page
/// without a" and "browser", two lines.
#[test]
fn fr34_article_text_wraps_at_the_column_width() {
    let (mut dom, requests) = laid_out(480.0);

    assert_rect(entry(&dom, "headline").rect, 24.0, 56.0, 432.0, 80.0, "h1");
    assert_eq!(entry(&dom, "headline").texts[0].line_count, 2);
    assert_rect(entry(&dom, "why").rect, 24.0, 196.0, 432.0, 32.0, "h2");

    let first = entry(&dom, "first");
    assert_rect(first.rect, 24.0, 240.0, 432.0, 120.0, "the first paragraph");
    assert_eq!(first.texts.len(), 1, "{:?}", first.texts);
    let run = &first.texts[0];
    assert_eq!(run.text, FIRST);
    assert_eq!(run.wrap_width, Some(432.0));
    assert_eq!(run.line_count, 5);
    // The widest line, "need a browser to be laid out. The Host runs the same", is 53
    // characters.
    assert_rect(
        run.rect,
        24.0,
        240.0,
        424.0,
        120.0,
        "the first paragraph's text",
    );
    assert!(
        requests.borrow().iter().any(|request| request.text == FIRST
            && request.width == WidthConstraint::AtMost(432.0)
            && request.white_space == TextWhiteSpace::NORMAL),
        "the measurer was asked to fit the paragraph into the column: {:?}",
        requests.borrow()
    );

    // The second paragraph is five lines as well, after a 16px margin.
    assert_rect(
        entry(&dom, "second").rect,
        24.0,
        376.0,
        432.0,
        120.0,
        "the second paragraph",
    );

    // Inside the quotation the column loses the 4px border and 16px padding: 412px, 51
    // characters, which breaks the quotation's 86 characters into two lines.
    let quote = entry(&dom, "quote");
    assert_eq!(quote.rect.x, 44.0);
    assert_eq!(quote.rect.width, 412.0);
    assert_eq!(quote.texts[0].wrap_width, Some(412.0));
    assert_eq!(quote.texts[0].line_count, 2);
    assert_eq!(quote.rect.height, 48.0);

    // Wider, the same paragraph takes fewer lines. So does the headline, one 40px line
    // now, which moves the paragraph up by 40px.
    dom.layout(800.0, 2000.0, 1.0);
    let first = entry(&dom, "first");
    assert_rect(
        first.rect,
        104.0,
        200.0,
        592.0,
        72.0,
        "the first paragraph at 800px",
    );
    assert_eq!(first.texts[0].wrap_width, Some(592.0));
    assert_eq!(first.texts[0].line_count, 3);
    assert_eq!(entry(&dom, "headline").texts[0].line_count, 1);
}

/// List items are 24px lines, indented by the list's 24px padding, and stack with their
/// 4px margins collapsed between them: one every 28px.
#[test]
fn fr34_article_list_items_stack() {
    let (dom, _) = laid_out(480.0);

    for list in ["crosses", "stays"] {
        let items: Vec<_> = (1..=3)
            .map(|index| entry(&dom, &format!("{list}-{index}")).rect)
            .collect();
        for (index, item) in items.iter().enumerate() {
            assert_eq!(
                item.x,
                48.0,
                "#{list}-{} starts after the list's padding",
                index + 1
            );
            assert_eq!(item.width, 408.0, "#{list}-{}", index + 1);
            assert_eq!(item.height, 24.0, "#{list}-{} is one line", index + 1);
        }
        assert_eq!(items[1].y - items[0].y, 28.0, "#{list}: {items:?}");
        assert_eq!(items[2].y - items[1].y, 28.0, "#{list}: {items:?}");

        let list_box = entry(&dom, list).rect;
        assert_eq!(
            list_box.y, items[0].y,
            "the first item's margin collapses through"
        );
        assert_eq!(list_box.height, 3.0 * 24.0 + 2.0 * 4.0);
    }

    // The ordered list follows the unordered one.
    assert!(entry(&dom, "stays").rect.y > entry(&dom, "crosses").rect.bottom());
}

/// `pre` keeps its line breaks and never wraps: the measurer is asked about the sample as
/// written, with `white-space: pre` and no width limit, and the widest line (67
/// characters at 7px, 469px) is wider than the 400px inside the block's padding.
#[test]
fn fr34_article_pre_does_not_wrap() {
    let (dom, requests) = laid_out(480.0);

    assert!(
        requests
            .borrow()
            .iter()
            .filter(|request| request.text == article::SAMPLE)
            .all(|request| request.white_space == TextWhiteSpace::PRE
                && request.width == WidthConstraint::MaxContent),
        "{:?}",
        requests.borrow()
    );
    assert!(
        requests
            .borrow()
            .iter()
            .any(|request| request.text == article::SAMPLE),
        "the sample is measured as one run: {:?}",
        requests.borrow()
    );

    let sample = entry(&dom, "sample");
    assert_eq!(sample.texts.len(), 1, "{:?}", sample.texts);
    let run = &sample.texts[0];
    assert_eq!(run.text, article::SAMPLE);
    assert_eq!(run.wrap_width, None);
    assert_eq!(run.line_count, 5);
    assert_rect(
        run.rect,
        40.0,
        sample.rect.y + 12.0,
        469.0,
        100.0,
        "the sample's text",
    );
    // The block keeps the column's width and grows to five 20px lines and its padding.
    assert_eq!(sample.rect.width, 432.0);
    assert_eq!(sample.rect.height, 124.0);
    // The text belongs to the `code` element inside the `pre`.
    let code = dom
        .document()
        .get_node(node(&dom, "sample"))
        .and_then(|pre| pre.children.first().copied())
        .expect("the pre holds its code element");
    assert_eq!(run.owner, code);
}

/// `text-transform: uppercase` is applied before measuring: the measurer is asked about
/// "FIELD NOTES", never "Field notes", and the run is 11 characters at 6px, 66px.
#[test]
fn fr34_article_text_transform_reaches_the_measurer() {
    let (dom, requests) = laid_out(480.0);

    assert!(
        requests
            .borrow()
            .iter()
            .any(|request| request.text == "FIELD NOTES"),
        "{:?}",
        requests.borrow()
    );
    assert!(
        requests
            .borrow()
            .iter()
            .all(|request| request.text != "Field notes"),
        "the source text is never measured: {:?}",
        requests.borrow()
    );

    let kicker = entry(&dom, "kicker");
    assert_eq!(kicker.texts[0].text, "FIELD NOTES");
    assert_rect(
        kicker.texts[0].rect,
        24.0,
        32.0,
        66.0,
        16.0,
        "the kicker's text",
    );
}
