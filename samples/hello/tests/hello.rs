//! The hello example laid out in an 800 by 600 viewport.
//!
//! Every number follows from `hello::STYLE` and the support measurer (an advance of half
//! the font size per character, the CSS line height per line):
//!
//! - `.page` has 40px of padding top and bottom and 48px on the sides, so its content
//!   starts at (48, 40) and is 800 - 96 = 704px wide.
//! - `h1` is one 40px line ("Hello, Dioxus", 13 characters at 16px each is 208px) with a
//!   16px bottom margin.
//! - Each `p` has 12px margins and one 24px line (16px at `line-height: 1.5`). Between the
//!   heading and the first paragraph the 16px and 12px margins collapse to 16px; between
//!   two paragraphs 12px and 12px collapse to 12px.
//!
//! So `h1` is at y = 40, `#intro` at 40 + 40 + 16 = 96, `#action` at 96 + 24 + 12 = 132
//! and `#status` at 132 + 24 + 12 = 168.

use dioxus_compose::html::HtmlDom;
use sample_html_hello as hello;
use sample_html_support::{Measurer, assert_rect, centre, config, entry, node, text_of};

fn laid_out() -> HtmlDom {
    let mut dom = HtmlDom::with_config(hello::app, config(hello::STYLE, Measurer::new()));
    dom.layout(800.0, 600.0, 1.0);
    dom
}

#[test]
fn fr34_hello_blocks_flow_with_collapsed_margins() {
    let dom = laid_out();

    assert_rect(entry(&dom, "title").rect, 48.0, 40.0, 704.0, 40.0, "h1");
    assert_rect(entry(&dom, "intro").rect, 48.0, 96.0, 704.0, 24.0, "#intro");
    assert_rect(
        entry(&dom, "action").rect,
        48.0,
        132.0,
        704.0,
        24.0,
        "#action",
    );
    assert_rect(
        entry(&dom, "status").rect,
        48.0,
        168.0,
        704.0,
        24.0,
        "#status",
    );

    let title = &entry(&dom, "title").texts;
    assert_eq!(title.len(), 1, "{title:?}");
    assert_eq!(title[0].text, "Hello, Dioxus");
    assert_eq!(title[0].line_count, 1);
    assert_rect(title[0].rect, 48.0, 40.0, 208.0, 40.0, "the heading's text");
}

/// The paragraph mixes three styles (plain, `strong`, `em`), so it is laid out as one
/// inline formatting context with a run per style. All of them sit on its one 24px line,
/// in source order.
#[test]
fn fr34_hello_inline_runs_share_one_line() {
    let dom = laid_out();
    let intro = entry(&dom, "intro");
    let bold = node(&dom, "bold");
    let italic = node(&dom, "italic");

    let text = text_of(intro);
    for piece in ["This page is", "HTML", "CSS", "laid out by the Host."] {
        assert!(
            text.contains(piece),
            "{piece:?} is drawn: {:?}",
            intro.texts
        );
    }

    let run_of = |owner: usize, word: &str| {
        intro
            .texts
            .iter()
            .find(|run| run.owner == owner && run.text.trim() == word)
            .cloned()
            .unwrap_or_else(|| panic!("a run of {word:?} owned by node {owner}: {:?}", intro.texts))
    };
    let strong = run_of(bold, "HTML");
    let em = run_of(italic, "CSS");
    assert!(
        strong.rect.x < em.rect.x,
        "`strong` comes before `em` on the line: {:?} then {:?}",
        strong.rect,
        em.rect
    );

    for run in &intro.texts {
        assert_eq!(run.line_count, 1, "{run:?}");
        let (_, middle) = centre(run.rect);
        assert!(
            (96.0..120.0).contains(&middle),
            "{:?} is on the paragraph's one line, y 96 to 120: {:?}",
            run.text,
            run.rect
        );
    }
}

/// The link is the only thing in its paragraph, so its text is one measured run owned by
/// the `<a>`: "Say hello" is 9 characters, 72px, at (48, 132). A click there reaches the
/// link's handler, and the status line shows the new count after the next render.
#[test]
fn fr34_hello_clicking_the_link_reaches_its_handler() {
    let mut dom = laid_out();
    let greet = node(&dom, "greet");

    let action = entry(&dom, "action");
    assert_eq!(action.texts.len(), 1, "{:?}", action.texts);
    let run = &action.texts[0];
    assert_eq!(run.owner, greet);
    assert_eq!(run.text, "Say hello");
    assert_rect(run.rect, 48.0, 132.0, 72.0, 24.0, "the link's text");
    assert_eq!(text_of(entry(&dom, "status")), "Nobody has said hello yet");

    let (x, y) = centre(run.rect);
    assert_eq!(dom.click(x, y), Some(greet));
    dom.render();
    dom.layout(800.0, 600.0, 1.0);
    assert_eq!(text_of(entry(&dom, "status")), "You said hello once");

    dom.click(x, y);
    dom.render();
    dom.layout(800.0, 600.0, 1.0);
    assert_eq!(text_of(entry(&dom, "status")), "You said hello 2 times");

    // Nothing else on the page listens: a click on the status line or the heading goes
    // nowhere and changes nothing.
    assert_eq!(dom.click(60.0, 180.0), None);
    assert_eq!(dom.click(60.0, 60.0), None);
    dom.render();
    dom.layout(800.0, 600.0, 1.0);
    assert_eq!(text_of(entry(&dom, "status")), "You said hello 2 times");
}
