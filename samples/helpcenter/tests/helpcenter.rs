//! The help centre example laid out in a 600 by 800 viewport.
//!
//! Every number follows from `helpcenter::STYLE` and the support measurer (an advance of
//! half the font size per character, the CSS line height per line). The page is 14px, so a
//! character is 7px wide, and its line height is 20px.
//!
//! - `.help` has 16px of padding, so the content starts at (16, 16) and is 568px wide.
//! - The heading is one 24px line (`line-height: 24px`) with 12px below it, so the search
//!   field is at y = 16 + 24 + 12 = 52. It is 32px tall, the full 568px wide, with 8px below
//!   it, so the summary is at y = 52 + 32 + 8 = 92. That is one 20px line with 12px below
//!   it, so the list of topics starts at y = 92 + 20 + 12 = 124.
//! - A closed topic is its 40px heading inside a 1px border: 42px tall. Topics are 8px
//!   apart, so the closed topics are at y = 124, 174, 224 and 274.
//! - A heading is a flex row with 12px of padding on each side, so its content is
//!   566 - 24 = 542px wide and starts at x = 16 + 1 + 12 = 29. The chevron is one character
//!   (7px) and does not shrink, the gap is 8px, so the name gets 542 - 7 - 8 = 527px. Both
//!   are 20px tall and centred in the 40px heading: y = topic + 1 + 10.
//! - An open topic adds its body under the heading: 12px of padding below the answer, whose
//!   text is 542px wide, so 542 / 7 = 77 characters to a line.

use dioxus_compose::html::{
    AnimatedProperty, Easing, HtmlConfig, HtmlDom, Rgba, RunMarker, SpanRun, TextRun,
};
use sample_html_helpcenter as help;
use sample_html_support::{Measurer, assert_close, assert_rect, centre, config, entry, node};

const WIDTH: f32 = 600.0;
const HEIGHT: f32 = 800.0;

fn open_with(config: HtmlConfig) -> HtmlDom {
    let mut dom = HtmlDom::with_config(help::app, config);
    dom.set_time(0.0);
    dom.layout(WIDTH, HEIGHT, 1.0);
    dom
}

fn open() -> HtmlDom {
    open_with(config(help::STYLE, Measurer::new()))
}

/// Applies what the app changed and lays out at `seconds`.
fn relayout(dom: &mut HtmlDom, seconds: f64) {
    dom.render();
    dom.set_time(seconds);
    dom.layout(WIDTH, HEIGHT, 1.0);
}

/// Clicks the middle of the element with `id`, then lays out at `seconds`.
#[track_caller]
fn click(dom: &mut HtmlDom, id: &str, seconds: f64) {
    let (x, y) = centre(entry(dom, id).rect);
    assert!(
        dom.click(x, y).is_some(),
        "nothing at or around #{id} listens for clicks"
    );
    relayout(dom, seconds);
}

/// Types into the search field, as the renderer commits text, then lays out.
#[track_caller]
fn search(dom: &mut HtmlDom, text: &str) {
    let field = node(dom, "search");
    assert_eq!(dom.input(field, text), Some(field));
    relayout(dom, 0.0);
}

fn summary(dom: &HtmlDom) -> String {
    entry(dom, "summary").texts[0].text.clone()
}

/// The text of the one run an answer paragraph draws.
#[track_caller]
fn answer<'a>(dom: &'a HtmlDom, id: &str) -> &'a TextRun {
    let texts = &entry(dom, id).texts;
    assert_eq!(texts.len(), 1, "#{id} draws one run: {texts:?}");
    &texts[0]
}

/// The words a span covers.
fn words<'a>(run: &'a TextRun, span: &SpanRun) -> &'a str {
    &run.text[span.start..span.end]
}

#[test]
fn fr34_helpcenter_closed_topics_stack_under_the_search_field() {
    let dom = open();

    assert_rect(entry(&dom, "search").rect, 16.0, 52.0, 568.0, 32.0, "search");
    assert_rect(entry(&dom, "summary").rect, 16.0, 92.0, 568.0, 20.0, "summary");
    assert_eq!(summary(&dom), "4 of 4 topics");

    for (index, id) in ["reset", "export", "billing", "shortcuts"].iter().enumerate() {
        let top = 124.0 + 50.0 * index as f32;
        assert_rect(
            entry(&dom, &format!("topic-{id}")).rect,
            16.0,
            top,
            568.0,
            42.0,
            id,
        );
        // A closed topic has no body.
        assert!(dom.element_by_id(&format!("body-{id}")).is_none(), "#{id}");
    }
}

/// The name and the chevron are flex items: the chevron is 7px at the end of the row, and
/// the name takes the other 527px. "Reset your password" is 19 characters (133px) and fits.
/// The export topic's name is 112 characters, 784px, on one line, so the box keeps its
/// 527px and its one line, the run is the whole 784px that the box clips, and the run asks
/// for an ellipsis at the box's width.
#[test]
fn fr34_helpcenter_a_long_name_is_cut_with_an_ellipsis() {
    let dom = open();

    assert_rect(entry(&dom, "head-reset").rect, 17.0, 125.0, 566.0, 40.0, "head");
    assert_rect(entry(&dom, "name-reset").rect, 29.0, 135.0, 527.0, 20.0, "name");
    assert_rect(entry(&dom, "chevron-reset").rect, 564.0, 135.0, 7.0, 20.0, "chevron");
    assert_close(entry(&dom, "name-reset").texts[0].rect.width, 133.0, "short name");

    // The export heading is 50px below the first: 175, so its name is at y = 185.
    assert_rect(entry(&dom, "name-export").rect, 29.0, 185.0, 527.0, 20.0, "long name");
    let run = &entry(&dom, "name-export").texts[0];
    assert_eq!(run.line_count, 1);
    assert_close(run.rect.width, 784.0, "long name run");
    assert_eq!(run.ellipsis, Some(527.0));
}

/// Opening "Reset your password" puts its body under the heading, at y = 124 + 1 + 40 =
/// 165, 566px wide and 12px of padding below the answer. The answer is 85 characters at
/// 77 to a line, so two 20px lines: the paragraph is 542 x 40 at x = 29, the body 40 + 12
/// = 52px tall and the topic 1 + 40 + 52 + 1 = 94px. The next
/// topics move down by 94 - 42 = 52px, so the export topic is at y = 124 + 94 + 8 = 226.
///
/// The paragraph is one text, with a bold span over "Settings" and an italic one over
/// "Security", in source order. Clicking the heading again closes the topic.
#[test]
fn fr34_helpcenter_opening_a_topic_shows_its_answer_and_pushes_the_rest_down() {
    let mut dom = open();
    click(&mut dom, "head-reset", 1.0);

    assert_rect(entry(&dom, "topic-reset").rect, 16.0, 124.0, 568.0, 94.0, "topic");
    assert_rect(entry(&dom, "body-reset").rect, 17.0, 165.0, 566.0, 52.0, "body");
    assert_rect(entry(&dom, "answer-reset").rect, 29.0, 165.0, 542.0, 40.0, "answer");
    assert_rect(entry(&dom, "topic-export").rect, 16.0, 226.0, 568.0, 42.0, "export");

    let run = answer(&dom, "answer-reset");
    assert_eq!(
        run.text,
        "Open Settings, choose Security and press Reset. The link stays valid for ten minutes."
    );
    assert_eq!(run.line_count, 2);
    let bold: Vec<_> = run.spans.iter().filter(|span| span.bold).collect();
    let italic: Vec<_> = run.spans.iter().filter(|span| span.italic).collect();
    assert_eq!((bold.len(), italic.len()), (1, 1), "{:?}", run.spans);
    assert_eq!(words(run, bold[0]), "Settings");
    assert_eq!(words(run, italic[0]), "Security");
    assert!(bold[0].start < italic[0].start);

    click(&mut dom, "head-reset", 2.0);
    assert!(dom.element_by_id("body-reset").is_none());
    assert_rect(entry(&dom, "topic-reset").rect, 16.0, 124.0, 568.0, 42.0, "closed again");
    assert_rect(entry(&dom, "topic-export").rect, 16.0, 174.0, 568.0, 42.0, "export back");
}

/// The chevron has no transform while its topic is closed. Opening the topic gives it a
/// quarter turn about its centre and starts a 0.2 second `ease` transition of `transform`
/// for the renderer to play; the entry holds the end value.
///
/// The chevron box is 7 x 20 at (564, 135), centre (567.5, 145). Turned 90 degrees
/// clockwise, a corner at (dx, dy) from the centre goes to (-dy, dx), so the box covers
/// 20 x 7: x from 567.5 - 10 = 557.5 to 577.5, y from 145 - 3.5 = 141.5 to 148.5.
#[test]
fn fr34_helpcenter_the_chevron_turns_by_a_transition_when_a_topic_opens() {
    let mut dom = open();
    assert!(entry(&dom, "chevron-reset").transform.is_none());
    assert!(dom.display_list().unwrap().transitions.is_empty());

    click(&mut dom, "head-reset", 1.0);

    let chevron = node(&dom, "chevron-reset");
    let list = dom.display_list().unwrap();
    let started: Vec<_> = list
        .transitions
        .iter()
        .filter(|start| start.node == chevron)
        .map(|start| &start.animation)
        .collect();
    assert_eq!(started.len(), 1, "{:#?}", list.transitions);
    assert_eq!(started[0].property, AnimatedProperty::Transform);
    assert_eq!((started[0].duration, started[0].delay), (0.2, 0.0));
    assert_eq!(started[0].easing, Easing::Ease);

    let entry = entry(&dom, "chevron-reset");
    let transform = entry.transform.as_ref().expect("the open chevron is turned");
    assert_rect(transform.bounds(&entry.rect), 557.5, 141.5, 20.0, 7.0, "turned chevron");
}

/// The billing answer is 164 characters. At 77 to a line it needs three lines (two hold
/// 154), so the paragraph is 542 x 60 at (29, 224 + 1 + 40 = 265), and the topic is
/// 1 + 40 + 60 + 12 + 1 = 114px tall.
///
/// It is one text. The red `span` is a span over "Cards that fail twice are paused" with
/// its own colour, and the link is a span over "billing page" that is blue and underlined.
/// Neither is a box: no `a` and no inline `span` has an entry in the display list, so the
/// link is not a separate target. (The spans that do have entries are the names and the
/// chevrons, which are flex items.)
#[test]
fn fr34_helpcenter_colour_and_a_link_are_spans_over_one_text() {
    let mut dom = open();
    click(&mut dom, "head-billing", 1.0);

    assert_rect(entry(&dom, "topic-billing").rect, 16.0, 224.0, 568.0, 114.0, "topic");
    assert_rect(entry(&dom, "answer-billing").rect, 29.0, 265.0, 542.0, 60.0, "answer");

    let run = answer(&dom, "answer-billing");
    assert_eq!(run.line_count, 3);

    let warn = run
        .spans
        .iter()
        .find(|span| span.color == Some(Rgba::new(185, 28, 28, 255)))
        .unwrap_or_else(|| panic!("no red span in {:?}", run.spans));
    assert_eq!(words(run, warn), "Cards that fail twice are paused");
    assert!(!warn.underline);

    let link = run
        .spans
        .iter()
        .find(|span| span.underline)
        .unwrap_or_else(|| panic!("no underlined span in {:?}", run.spans));
    assert_eq!(words(run, link), "billing page");
    assert_eq!(link.color, Some(Rgba::new(9, 105, 218, 255)));

    let list = dom.display_list().unwrap();
    assert!(list.entries.iter().all(|entry| entry.tag != "a"));
    for entry in list.entries.iter().filter(|entry| entry.tag == "span") {
        let text = entry.texts.first().map(|run| run.text.as_str()).unwrap_or("");
        assert!(
            text != "billing page" && !text.starts_with("Cards that fail"),
            "inline spans have no entry of their own: {text}"
        );
    }
}

/// `code` sets a monospace family, so its span carries one and the paragraph around it keeps
/// its own: the export answer is 67 characters, one 469px line (67 * 7), in a 542px box.
/// The topic is 1 + 40 + 20 + 12 + 1 = 74px tall, at y = 174.
#[test]
fn fr34_helpcenter_inline_code_carries_its_own_family() {
    let mut dom = open();
    click(&mut dom, "head-export", 1.0);

    assert_rect(entry(&dom, "topic-export").rect, 16.0, 174.0, 568.0, 74.0, "topic");
    assert_rect(entry(&dom, "answer-export").rect, 29.0, 215.0, 542.0, 20.0, "answer");

    let run = answer(&dom, "answer-export");
    assert_eq!(run.line_count, 1);
    assert_close(run.rect.width, 469.0, "answer width");
    let code = run
        .spans
        .iter()
        .find(|span| span.font_family.is_some())
        .unwrap_or_else(|| panic!("no span with its own family in {:?}", run.spans));
    assert_eq!(words(run, code), "export.csv");
    let bold = run.spans.iter().find(|span| span.bold).expect("a bold span");
    assert_eq!(words(run, bold), "Export");
}

/// The shortcuts answer is a numbered list at x = 29 with 20px of padding on the left, so
/// its items are at x = 49 and 522px wide, one 20px line each, from y = 274 + 1 + 40 = 315.
/// A marker is a run of its own, two characters (14px), ending one space (7px) before the
/// item: x = 49 - 7 - 14 = 28. The item's text is the second run.
#[test]
fn fr34_helpcenter_the_shortcuts_are_a_numbered_list() {
    let mut dom = open();
    click(&mut dom, "head-shortcuts", 1.0);

    assert_rect(entry(&dom, "answer-shortcuts").rect, 29.0, 315.0, 542.0, 60.0, "list");
    for (index, (marker, text)) in [
        ("1.", "Press / to focus the search"),
        ("2.", "Press Esc to clear it"),
        ("3.", "Press Enter to open a topic"),
    ]
    .iter()
    .enumerate()
    {
        let id = format!("shortcut-{}", index + 1);
        let top = 315.0 + 20.0 * index as f32;
        let item = entry(&dom, &id);
        assert_rect(item.rect, 49.0, top, 522.0, 20.0, &id);
        assert_eq!(item.texts.len(), 2, "#{id}: the marker and the text");
        assert_eq!(item.texts[0].marker, RunMarker::Marker);
        assert_eq!(item.texts[0].text, *marker);
        assert_rect(item.texts[0].rect, 28.0, top, 14.0, 20.0, &format!("{id} marker"));
        assert_eq!(item.texts[1].text, *text);
        assert_close(item.texts[1].rect.x, 49.0, &format!("{id} text x"));
    }
}

/// Typing "pass" keeps the topics whose name holds it, ignoring case: only "Reset your
/// password". The summary says "1 of 4 topics" and the topic moves up to y = 124. A search
/// that matches nothing shows the message in the summary's place below it, at y = 92 + 20
/// + 12 = 124, and an empty list. Clearing the field brings every topic back, and a topic
/// that was open stays open.
#[test]
fn fr34_helpcenter_search_filters_topics_by_name() {
    let mut dom = open();
    click(&mut dom, "head-reset", 1.0);

    search(&mut dom, "PASS");
    assert_eq!(summary(&dom), "1 of 4 topics");
    assert_rect(entry(&dom, "topic-reset").rect, 16.0, 124.0, 568.0, 94.0, "reset");
    for id in ["export", "billing", "shortcuts"] {
        assert!(dom.element_by_id(&format!("topic-{id}")).is_none(), "#{id}");
    }
    assert!(dom.element_by_id("empty").is_none());

    search(&mut dom, "zzz");
    assert_eq!(summary(&dom), "0 of 4 topics");
    assert_rect(entry(&dom, "empty").rect, 16.0, 124.0, 568.0, 20.0, "empty");
    assert!(dom.element_by_id("topic-reset").is_none());

    search(&mut dom, "");
    assert_eq!(summary(&dom), "4 of 4 topics");
    assert!(dom.element_by_id("empty").is_none());
    assert_rect(entry(&dom, "topic-reset").rect, 16.0, 124.0, 568.0, 94.0, "reset open");
    assert_rect(entry(&dom, "topic-export").rect, 16.0, 226.0, 568.0, 42.0, "export");
}
