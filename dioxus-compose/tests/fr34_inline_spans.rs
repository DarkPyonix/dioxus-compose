//! An inline formatting context of mixed styles is one text with spans.
//!
//! `p { "a " b { "bold" } " c" }` is one paragraph: the renderer measures it whole, so the
//! bold word wraps with what surrounds it, and draws it whole, so the letters land where
//! it measured them. These tests read the spans out of the display list, the measure
//! request and the batch, with a measurer that gives every character 10px and every line
//! 20px.

use std::cell::RefCell;
use std::ffi::c_int;
use std::rc::Rc;

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::fonts::FontRef;
use dioxus_compose::html::{
    BaseDocument, DisplayList, HtmlConfig, NodeEntry, SpanRun, TextMeasureRequest, TextMeasurer,
    TextMetrics, WidthConstraint, element_by_id, layout_document,
};
use dioxus_compose::measure::{
    MEASURE_RECORD_LEN, MEASURE_TEXT, MeasureResult, RESULT_OK, RESULT_UNKNOWN_NODE,
    TEXT_SPANS_COUNT_AT,
};
use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{GenericFamily, PropertyKind};
use dioxus_compose::spans::TextSpans;
use dioxus_compose::window::reset_window_size;

const CHAR: f32 = 10.0;
const LINE: f32 = 20.0;

/// Every character is 10px wide and every line 20px tall, and the runs it was asked about
/// are written down.
#[derive(Clone, Default)]
struct Recorder {
    asked: Rc<RefCell<Vec<(String, Vec<SpanRun>)>>>,
}

impl Recorder {
    fn metrics(request: &TextMeasureRequest<'_>) -> TextMetrics {
        let width = request.text.chars().count() as f32 * CHAR;
        TextMetrics {
            width: match request.width {
                WidthConstraint::AtMost(limit) => width.min(limit),
                _ => width,
            },
            height: LINE,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

impl TextMeasurer for Recorder {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        self.asked
            .borrow_mut()
            .push((request.text.to_string(), Vec::new()));
        Self::metrics(request)
    }

    fn measure_spans(&mut self, request: &TextMeasureRequest<'_>, runs: &[SpanRun]) -> TextMetrics {
        self.asked
            .borrow_mut()
            .push((request.text.to_string(), runs.to_vec()));
        Self::metrics(request)
    }
}

fn parse(body: &str, css: &str) -> BaseDocument {
    let html = format!(
        "<!DOCTYPE html><html><head><style>html, body {{ margin: 0; }} \
         b {{ font-weight: 700; }} i {{ font-style: italic; }} \
         u {{ text-decoration: underline; }} s {{ text-decoration: line-through; }} \
         {css}</style></head>\
         <body>{body}</body></html>"
    );
    let config = DocumentConfig {
        viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
        ..DocumentConfig::default()
    };
    HtmlDocument::from_html(&html, config).into_inner()
}

fn laid_out(body: &str, css: &str) -> (BaseDocument, DisplayList, Recorder) {
    let mut doc = parse(body, css);
    let mut recorder = Recorder::default();
    let list = layout_document(&mut doc, &mut recorder);
    (doc, list, recorder)
}

fn entry<'a>(list: &'a DisplayList, doc: &BaseDocument, id: &str) -> &'a NodeEntry {
    let node = element_by_id(doc, id).unwrap_or_else(|| panic!("no element {id}"));
    list.get(node)
        .unwrap_or_else(|| panic!("#{id} has no display list entry"))
}

/// What each run of a text covers, as the string it indexes.
fn covered<'a>(text: &'a str, spans: &[SpanRun]) -> Vec<&'a str> {
    spans
        .iter()
        .map(|span| &text[span.start..span.end])
        .collect()
}

#[test]
fn fr34_mixed_inline_text_is_one_run_with_spans() {
    let (doc, list, _) = laid_out(r#"<p id="p">a <b>bold</b> c</p>"#, "");
    let texts = &entry(&list, &doc, "p").texts;
    assert_eq!(texts.len(), 1, "{texts:?}");
    let run = &texts[0];
    assert_eq!(run.text, "a bold c");
    assert_eq!(run.spans.len(), 1, "{:?}", run.spans);
    assert!(run.spans[0].bold);
    assert_eq!(covered(&run.text, &run.spans), ["bold"]);
    // One text, one line: 8 characters of 10px.
    assert_eq!(run.line_count, 1);
    assert_eq!(run.rect.width, 8.0 * CHAR);
    assert_eq!(run.rect.height, LINE);
}

#[test]
fn fr34_the_measurer_is_asked_about_the_whole_context_with_its_spans() {
    let (_, _, recorder) = laid_out(r#"<p>a <b>bold</b> c</p><p>plain</p>"#, "");
    let asked = recorder.asked.borrow();
    let mixed: Vec<_> = asked
        .iter()
        .filter(|(text, _)| text == "a bold c")
        .collect();
    assert!(!mixed.is_empty(), "{asked:?}");
    for (text, runs) in mixed {
        assert_eq!(covered(text, runs), ["bold"], "{runs:?}");
    }
    // Text in one style carries no runs, so it travels as it did before spans existed.
    assert!(
        asked
            .iter()
            .filter(|(text, _)| text == "plain")
            .all(|(_, runs)| runs.is_empty()),
        "{asked:?}"
    );
}

#[test]
fn fr34_spans_survive_white_space_collapsing_across_holders() {
    // The blanks at the ends, the run of blanks after "a" and the ones around "bold" are
    // one space each in the text, and the bold element's span starts at its first letter.
    let (doc, list, _) = laid_out("<p id=\"p\">  a   <b> bold </b>  c  </p>", "");
    let run = &entry(&list, &doc, "p").texts[0];
    assert_eq!(run.text, "a bold c");
    assert_eq!(run.spans.len(), 1, "{:?}", run.spans);
    assert_eq!(run.spans[0].start, 2, "{:?}", run.spans);
    assert_eq!(covered(&run.text, &run.spans)[0].trim_end(), "bold");
    // The marker of white space collapsing across a boundary: "x" and "y" touch the bold
    // element with no blank of their own between them.
    let (doc, list, _) = laid_out(r#"<p id="p">x<b>bold</b>y</p>"#, "");
    let run = &entry(&list, &doc, "p").texts[0];
    assert_eq!(run.text, "xboldy");
    assert_eq!(covered(&run.text, &run.spans), ["bold"]);
}

#[test]
fn fr34_spans_keep_their_place_through_text_transform() {
    // "ﬁ" is three bytes and becomes the two bytes "FI", so every range after it moves.
    let (doc, list, _) = laid_out(
        "<p id=\"p\" style=\"text-transform: uppercase\">\u{fb01} <b>bold</b></p>",
        "",
    );
    let run = &entry(&list, &doc, "p").texts[0];
    assert_eq!(run.text, "FI BOLD");
    assert_eq!(run.spans.len(), 1, "{:?}", run.spans);
    assert_eq!(covered(&run.text, &run.spans), ["BOLD"]);
    assert_eq!(run.spans[0].start, 3);

    // A transform on the inner element only changes what it wraps.
    let (doc, list, _) = laid_out(
        r#"<p id="p">a <b style="text-transform: uppercase">bold</b> c</p>"#,
        "",
    );
    let run = &entry(&list, &doc, "p").texts[0];
    assert_eq!(run.text, "a BOLD c");
    assert_eq!(covered(&run.text, &run.spans), ["BOLD"]);
}

#[test]
fn fr34_spans_carry_italic_lines_colour_and_family() {
    let (doc, list, _) = laid_out(
        r#"<p id="p">a <i>it</i> <u>un</u> <s>st</s> <em class="c">co</em> <code>mono</code></p>"#,
        // A monospace family alone makes the browser's default size smaller; the explicit
        // size keeps `code` the size of the text around it, as a span needs.
        ".c { color: #ff0000; font-style: normal; } \
         code { font-family: monospace; font-size: 1em; } p { font-family: sans-serif; }",
    );
    let run = &entry(&list, &doc, "p").texts[0];
    let text = &run.text;
    let of = |word: &str| {
        run.spans
            .iter()
            .find(|span| &text[span.start..span.end] == word)
            .unwrap_or_else(|| panic!("no span over {word:?}: {:?}", run.spans))
    };
    assert!(of("it").italic);
    assert!(of("un").underline);
    assert!(of("st").line_through);
    assert!(of("co").color.is_some_and(|colour| colour.r == 255));
    assert_eq!(
        of("mono")
            .font_family
            .as_deref()
            .map(|f| f.first().cloned()),
        Some(Some("monospace".to_string()))
    );
}

/// A size, a letter spacing or a line height cannot ride on a span, so the context stays
/// with the fallback shaper, drawn a piece at a time, and nothing is carried as spans.
#[test]
fn fr34_mixed_sizes_stay_with_the_fallback_shaper() {
    for inner in [
        "font-size: 24px",
        "letter-spacing: 2px",
        "line-height: 40px",
        "font-weight: 300",
    ] {
        let (doc, list, recorder) = laid_out(
            &format!(r#"<p id="p">a <span style="{inner}">big</span> c</p>"#),
            "",
        );
        let texts = &entry(&list, &doc, "p").texts;
        assert!(
            texts.iter().all(|run| run.spans.is_empty()),
            "{inner}: {texts:?}"
        );
        assert!(
            texts.iter().all(|run| run.text != "a big c"),
            "{inner}: the context was carried as one text: {texts:?}"
        );
        assert!(
            recorder
                .asked
                .borrow()
                .iter()
                .all(|(_, runs)| runs.is_empty()),
            "{inner}: the measurer was handed spans"
        );
    }
}

// ------------------------------------------------------------------------------------------
// Through a Host: the measure request and the batch carry the same spans.

thread_local! {
    /// The span count of every text record the renderer was asked about.
    static ASKED: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

unsafe extern "C" fn measure_by_the_character(
    requests: *const u8,
    length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    // SAFETY: the caller hands `length` readable bytes and room for `count` results.
    let buffer = unsafe { std::slice::from_raw_parts(requests, length as usize) };
    for index in 0..count as usize {
        let record = &buffer[index * MEASURE_RECORD_LEN..(index + 1) * MEASURE_RECORD_LEN];
        let result = if u16::from_le_bytes([record[0], record[1]]) == MEASURE_TEXT {
            let at = TEXT_SPANS_COUNT_AT;
            let spans =
                u32::from_le_bytes([record[at], record[at + 1], record[at + 2], record[at + 3]]);
            ASKED.with(|asked| asked.borrow_mut().push(spans));
            MeasureResult {
                width: 80.0,
                height: LINE,
                first_baseline: 16.0,
                line_count: 1,
                status: RESULT_OK,
                ..MeasureResult::default()
            }
        } else {
            MeasureResult {
                status: RESULT_UNKNOWN_NODE,
                ..MeasureResult::default()
            }
        };
        // SAFETY: `index` is below `count`.
        unsafe { results.add(index).write(result) };
    }
    0
}

mod page {
    use dioxus_compose::html::prelude::*;

    pub fn paragraph() -> Element {
        rsx! {
            p { "a " b { "bold" } " c " code { "mono" } }
        }
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![
            "html, body { margin: 0; padding: 0; } p { font-family: sans-serif; } \
             b { font-weight: 700; } code { font-family: monospace; font-size: 1em; }"
                .to_string(),
        ],
        ..HtmlConfig::default()
    }
}

#[test]
fn fr34_the_measure_request_and_the_batch_carry_the_spans() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: measure_by_the_character,
    });
    ASKED.with(|asked| asked.borrow_mut().clear());
    reset_window_size();
    let mut host = Host::html(page::paragraph, config);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();

    assert!(
        ASKED.with(|asked| asked.borrow().iter().any(|&spans| spans >= 2)),
        "the measure request carried no spans: {:?}",
        ASKED.with(|asked| asked.borrow().clone())
    );

    let mut spans: Option<TextSpans> = None;
    let mut fonts: Option<Vec<u8>> = None;
    let mut text = None;
    for record in &first {
        match record {
            Mutation::SetProp {
                property: PropertyKind::Spans,
                value: PropertyValue::Bytes(bytes),
                ..
            } => spans = Some(TextSpans::from_bytes(bytes.to_vec())),
            Mutation::SetProp {
                property: PropertyKind::SpanFonts,
                value: PropertyValue::Bytes(bytes),
                ..
            } => fonts = Some(bytes.to_vec()),
            Mutation::SetProp {
                property: PropertyKind::Text,
                value: PropertyValue::String(shown),
                ..
            } => text = Some(shown.to_string()),
            _ => {}
        }
    }
    let text = text.expect("the paragraph is sent as one text");
    assert_eq!(text, "a bold c mono");
    let spans = spans
        .expect("the paragraph's spans are sent")
        .with_font_table(fonts.expect("the monospace run names its font"));
    let runs: Vec<_> = spans.spans().collect();
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(
        &text[runs[0].start as usize..][..runs[0].length as usize],
        "bold"
    );
    assert!(runs[0].bold);
    assert_eq!(
        &text[runs[1].start as usize..][..runs[1].length as usize],
        "mono"
    );
    assert_eq!(
        spans.span_fonts(),
        vec![(1, vec![FontRef::Generic(GenericFamily::Monospace)])]
    );
}

// ------------------------------------------------------------------------------------------
// An element with a handler keeps it.

thread_local! {
    static PRESSED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

mod handled {
    use dioxus_compose::html::prelude::*;

    pub fn paragraph() -> Element {
        rsx! {
            p { id: "p",
                "a "
                b {
                    id: "word",
                    onclick: move |_| super::PRESSED.with(|pressed| pressed.borrow_mut().push("word")),
                    "bold"
                }
                " c"
            }
        }
    }
}

#[test]
fn fr34_an_onclick_on_an_inline_element_in_a_mixed_paragraph_still_fires() {
    use dioxus_compose::html::HtmlDom;

    PRESSED.with(|pressed| pressed.borrow_mut().clear());
    let mut dom = HtmlDom::with_config(
        handled::paragraph,
        HtmlConfig {
            stylesheets: vec!["html, body { margin: 0; } b { font-weight: 700; }".to_string()],
            measurer: Some(Box::new(Recorder::default())),
            ..HtmlConfig::default()
        },
    );
    dom.layout(400.0, 400.0, 1.0);
    let word = dom.element_by_id("word").unwrap();
    let paragraph = dom.element_by_id("p").unwrap();
    let list = dom.display_list().unwrap();
    let texts = &list.get(paragraph).unwrap().texts;
    // The element that listens keeps a run of its own, which a click reaches.
    let run = texts
        .iter()
        .find(|run| run.owner == word)
        .unwrap_or_else(|| panic!("no run owned by the listening element: {texts:?}"));
    assert!(texts.iter().all(|run| run.spans.is_empty()), "{texts:?}");
    let (x, y) = (
        run.rect.x + run.rect.width / 2.0,
        run.rect.y + run.rect.height / 2.0,
    );
    assert_eq!(dom.click(x, y), Some(word));
    assert_eq!(PRESSED.with(|pressed| pressed.borrow().clone()), vec!["word"]);
}
