//! An italic label is measured with the same description of its slant as it is drawn with.
//!
//! The draw sends a run's text style without a slant and the slant as one span over the
//! whole text, because a text style on the wire has no italic. The renderer reads a slant in
//! the style and a slant in a span differently (the font's tracking follows the style's), so
//! a measure request that put the slant in the style measured a different width from the
//! one drawn, and the last glyph of an italic label was cut.

use std::cell::RefCell;
use std::ffi::c_int;

use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::measure::{
    MEASURE_RECORD_LEN, MEASURE_TEXT, MeasureResult, RESULT_OK, RESULT_UNKNOWN_NODE,
    TEXT_ITALIC_AT, TEXT_SPANS_COUNT_AT, TEXT_SPANS_OFFSET_AT, TEXT_TEXT_LENGTH_AT,
    TEXT_TEXT_OFFSET_AT,
};
use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::spans::{SPAN_LEN, TextSpans};
use dioxus_compose::window::reset_window_size;
use dioxus_compose::{Host, PropertyKind};

/// One text question as the renderer received it.
#[derive(Clone, Debug)]
struct Asked {
    text: String,
    italic: bool,
    spans: Vec<u8>,
}

thread_local! {
    static ASKED: RefCell<Vec<Asked>> = const { RefCell::new(Vec::new()) };
}

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

unsafe extern "C" fn record_questions(
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
            let at = read_u32(record, TEXT_TEXT_OFFSET_AT) as usize;
            let len = read_u32(record, TEXT_TEXT_LENGTH_AT) as usize;
            let text = std::str::from_utf8(&buffer[at..at + len])
                .expect("the text is UTF-8")
                .to_string();
            let spans_at = read_u32(record, TEXT_SPANS_OFFSET_AT) as usize;
            let spans_count = read_u32(record, TEXT_SPANS_COUNT_AT) as usize;
            let spans = buffer[spans_at..spans_at + spans_count * SPAN_LEN].to_vec();
            let width = text.chars().count() as f32 * 7.0;
            ASKED.with(|asked| {
                asked.borrow_mut().push(Asked {
                    text,
                    italic: record[TEXT_ITALIC_AT] != 0,
                    spans,
                })
            });
            MeasureResult {
                width,
                height: 21.0,
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

    pub fn tab() -> Element {
        rsx! {
            div { style: "display: flex; width: 400px",
                span { style: "font-style: italic", "README.md" }
            }
        }
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        ..HtmlConfig::default()
    }
}

/// The slant as a list of (start, length, italic) runs.
fn slant(spans: &[u8]) -> Vec<(u32, u32, bool)> {
    TextSpans::from_bytes(spans.to_vec())
        .spans()
        .map(|span| (span.start, span.length, span.italic))
        .collect()
}

#[test]
fn fr34_an_italic_label_is_measured_with_the_style_it_is_drawn_with() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: record_questions,
    });
    ASKED.with(|asked| asked.borrow_mut().clear());
    reset_window_size();
    let mut host = Host::html(page::tab, config);
    let batch = decode_batch(host.rebuild().unwrap()).unwrap();

    let drawn: Vec<u8> = batch
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::SetProp {
                property: PropertyKind::Spans,
                value: PropertyValue::Bytes(bytes),
                ..
            } => Some(bytes.to_vec()),
            _ => None,
        })
        .expect("the italic label was drawn without a slanted span");
    let drawn = slant(&drawn);
    assert_eq!(
        drawn,
        vec![(0, "README.md".len() as u32, true)],
        "the label is drawn as one italic span over all of its text"
    );

    let asked: Vec<Asked> = ASKED.with(|asked| asked.borrow().clone());
    let questions: Vec<&Asked> = asked
        .iter()
        .filter(|question| question.text == "README.md")
        .collect();
    assert!(!questions.is_empty(), "the label was never measured");
    for question in questions {
        // The draw's text style carries no slant, so the measured one must not either.
        assert!(
            !question.italic,
            "the label was measured with a slanted text style and drawn with an upright one"
        );
        assert_eq!(
            slant(&question.spans),
            drawn,
            "the label was measured with other spans than it is drawn with"
        );
    }
}
