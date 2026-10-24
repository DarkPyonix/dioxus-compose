//! HTML text sized by the renderer's measure call, not by Parley.
//!
//! The renderer here is a test renderer that gives every character 7px and every line 21px,
//! numbers Parley would never produce for the default font, and counts the calls it gets.
//! A page is laid out through a Host the way a window lays it out, so the layout runs
//! inside a Host call and the measure call is open to it.

use std::cell::Cell;
use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::{
    HtmlConfig, OverflowWrap, ParleyMeasurer, RendererMeasurer, TextLineHeight,
    TextMeasureRequest, TextMeasurer, TextStyle, TextWhiteSpace, WidthConstraint, WordBreak,
};
use dioxus_compose::measure::{
    CONSTRAINT_AT_MOST, MEASURE_RECORD_LEN, MEASURE_TEXT, MeasureResult, RESULT_OK,
    RESULT_UNKNOWN_NODE, TEXT_CONSTRAINT_AT, TEXT_TEXT_LENGTH_AT, TEXT_TEXT_OFFSET_AT,
    TEXT_WIDTH_AT,
};
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::schema::Modifier;
use dioxus_compose::window::reset_window_size;

const ADVANCE: f32 = 7.0;
const LINE: f32 = 21.0;

thread_local! {
    /// How many measure calls this thread's renderer has answered.
    static CALLS: Cell<usize> = const { Cell::new(0) };
    /// How many records the first call of the test carried.
    static FIRST_CALL: Cell<usize> = const { Cell::new(0) };
    /// How many text records all calls carried.
    static TEXT_RECORDS: Cell<usize> = const { Cell::new(0) };
}

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Every character is `ADVANCE` wide and every line `LINE` tall. A text that does not fit
/// the width it is asked at is cut to one line of that width, which is enough for a page of
/// short labels.
unsafe extern "C" fn measure_by_the_character(
    requests: *const u8,
    length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    // SAFETY: the caller hands `length` readable bytes and room for `count` results.
    let buffer = unsafe { std::slice::from_raw_parts(requests, length as usize) };
    CALLS.with(|calls| calls.set(calls.get() + 1));
    FIRST_CALL.with(|first| {
        if first.get() == 0 {
            first.set(count as usize);
        }
    });
    for index in 0..count as usize {
        let record = &buffer[index * MEASURE_RECORD_LEN..(index + 1) * MEASURE_RECORD_LEN];
        let result = if u16::from_le_bytes([record[0], record[1]]) == MEASURE_TEXT {
            TEXT_RECORDS.with(|records| records.set(records.get() + 1));
            let at = read_u32(record, TEXT_TEXT_OFFSET_AT) as usize;
            let len = read_u32(record, TEXT_TEXT_LENGTH_AT) as usize;
            let text = std::str::from_utf8(&buffer[at..at + len]).expect("the text is UTF-8");
            let mut width = text.chars().count() as f32 * ADVANCE;
            if read_u32(record, TEXT_CONSTRAINT_AT) == CONSTRAINT_AT_MOST {
                width = width.min(f32::from_bits(read_u32(record, TEXT_WIDTH_AT)));
            }
            MeasureResult {
                width,
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

fn install() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: measure_by_the_character,
    });
    CALLS.with(|calls| calls.set(0));
    FIRST_CALL.with(|first| first.set(0));
    TEXT_RECORDS.with(|records| records.set(0));
}

mod page {
    use dioxus_compose::html::prelude::*;

    pub fn labels() -> Element {
        rsx! {
            div { style: "display: flex; width: 700px",
                span { "alpha" }
                span { "bravo!" }
                span { "charlie" }
                span { "delta" }
                span { "echo" }
                span { "foxtrot" }
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

fn required_sizes(mutations: &[Mutation<'_>]) -> Vec<(f32, f32)> {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                modifier: Modifier::RequiredSize { width, height },
                ..
            } => Some((*width, *height)),
            _ => None,
        })
        .collect()
}

#[test]
fn fr34_text_boxes_take_the_size_the_renderer_measured() {
    install();
    reset_window_size();
    let mut host = Host::html(page::labels, config);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let sizes = required_sizes(&first);
    for text in ["alpha", "bravo!", "charlie", "delta", "echo", "foxtrot"] {
        let width = text.chars().count() as f32 * ADVANCE;
        assert!(
            sizes.iter().any(|&(w, h)| w == width && h == LINE),
            "the box of {text:?} is not {width} by {LINE}, the size the renderer measured: {sizes:?}"
        );
    }
}

#[test]
fn fr34_text_is_measured_in_one_call_per_pass_not_one_per_node() {
    install();
    reset_window_size();
    let mut host = Host::html(page::labels, config);
    host.rebuild().unwrap();
    let calls = CALLS.with(Cell::get);
    let first = FIRST_CALL.with(Cell::get);
    assert!(calls >= 1, "the renderer was never asked");
    assert!(
        calls <= 3,
        "{calls} measure calls for a page of six labels: the page asks per node, not per pass"
    );
    assert!(
        first >= 6,
        "the first call carried {first} questions, fewer than the six labels of the page"
    );
}

fn style() -> TextStyle {
    TextStyle {
        font_family: vec!["sans-serif".to_string()],
        font_size: 14.0,
        font_weight: 400.0,
        italic: false,
        letter_spacing: 0.0,
        line_height: TextLineHeight::Normal,
        word_break: WordBreak::Normal,
        overflow_wrap: OverflowWrap::Normal,
    }
}

#[test]
fn fr34_without_a_renderer_to_ask_the_answers_are_parleys() {
    let style = style();
    let request = TextMeasureRequest {
        text: "Explorer",
        style: &style,
        width: WidthConstraint::MaxContent,
        white_space: TextWhiteSpace::NORMAL,
    };
    let mut measurer = RendererMeasurer::new();
    let answered = measurer.measure(&request);
    let parley = ParleyMeasurer::new().measure(&request);
    assert_eq!(answered, parley);
    // Outside a Host call nothing can be asked, so nothing changed and no pass repeats.
    assert!(!measurer.settle());
    assert_eq!(measurer.measure(&request), parley);
    assert_eq!(measurer.calls(), 0);
}

#[test]
fn fr34_a_settled_text_is_not_asked_again() {
    install();
    reset_window_size();
    let mut host = Host::html(page::labels, config);
    host.rebuild().unwrap();
    let asked = TEXT_RECORDS.with(Cell::get);
    let distinct = 6 * 3;
    assert!(
        asked <= distinct * 3,
        "{asked} text questions for six labels: the answers were not kept between passes"
    );
}
