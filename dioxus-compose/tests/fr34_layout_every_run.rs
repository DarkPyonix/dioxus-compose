//! A page is laid out the same however the renderer's answers arrive.

use std::cell::Cell;
use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::measure::{
    CONSTRAINT_AT_MOST, MEASURE_RECORD_LEN, MEASURE_TEXT, MEASURE_UNAVAILABLE, MeasureResult,
    RESULT_OK, RESULT_UNKNOWN_NODE, TEXT_CONSTRAINT_AT, TEXT_TEXT_LENGTH_AT, TEXT_TEXT_OFFSET_AT,
    TEXT_WIDTH_AT,
};
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::schema::Modifier;
use dioxus_compose::window::reset_window_size;

const ADVANCE: f32 = 7.0;
const LINE: f32 = 21.0;

thread_local! {
    static REFUSE: Cell<bool> = const { Cell::new(false) };
}

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

unsafe extern "C" fn measure(
    requests: *const u8,
    length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    if REFUSE.with(Cell::get) {
        return MEASURE_UNAVAILABLE;
    }
    // SAFETY: the caller hands `length` readable bytes and room for `count` results.
    let buffer = unsafe { std::slice::from_raw_parts(requests, length as usize) };
    for index in 0..count as usize {
        let record = &buffer[index * MEASURE_RECORD_LEN..(index + 1) * MEASURE_RECORD_LEN];
        let result = if u16::from_le_bytes([record[0], record[1]]) == MEASURE_TEXT {
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
        measure,
    });
}

mod page {
    use dioxus_compose::html::prelude::*;

    pub fn labels() -> Element {
        rsx! {
            div { style: "display: flex; width: 700px",
                span { "alpha" }
                span { "bravo!" }
                span { "charlie" }
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

/// Every box size the batches set, by node, the last one written winning.
fn sizes_by_node(batches: &[Vec<u8>]) -> std::collections::BTreeMap<u32, (f32, f32)> {
    let mut sizes = std::collections::BTreeMap::new();
    for batch in batches {
        for mutation in decode_batch(batch).unwrap() {
            if let Mutation::SetModifier {
                node_id,
                modifier: Modifier::RequiredSize { width, height },
                ..
            } = mutation
            {
                sizes.insert(node_id, (width, height));
            }
        }
    }
    sizes
}

/// A page laid out before the renderer could measure (it has not composed its first
/// frame) sizes its text with Parley's estimates. Those estimates used to stay: the page
/// was laid out again only where something changed, so which boxes kept an estimate
/// depended on when the renderer became ready, and differed from run to run. Now the
/// next frame measures the text again and lays the page out with the renderer's answers.
#[test]
fn fr34_text_measured_before_the_renderer_could_answer_is_measured_again() {
    install();
    reset_window_size();
    REFUSE.with(|refuse| refuse.set(true));
    let mut host = Host::html(page::labels, config);
    let first = required_sizes(&decode_batch(host.rebuild().unwrap()).unwrap());
    assert!(
        !first.contains(&(35.0, LINE)),
        "the renderer refused, yet its sizes arrived: {first:?}"
    );
    REFUSE.with(|refuse| refuse.set(false));
    let next = required_sizes(&decode_batch(host.render_frame(16_000_000).unwrap()).unwrap());
    for text in ["alpha", "bravo!", "charlie"] {
        let width = text.chars().count() as f32 * ADVANCE;
        assert!(
            next.contains(&(width, LINE)),
            "the box of {text:?} did not take the renderer's {width} by {LINE} once it \
             could answer: {next:?}"
        );
    }
    let settled = decode_batch(host.render_frame(32_000_000).unwrap()).unwrap();
    assert!(
        settled.is_empty(),
        "a settled page with nothing changed wrote records again: {settled:?}"
    );
}

/// However many frames the renderer took to become ready, the page ends with the boxes it
/// has when the renderer answers from the first frame.
#[test]
fn fr34_layout_is_the_same_every_run_whenever_the_renderer_is_ready() {
    install();
    let mut expected = None;
    for refused_frames in 0..4u64 {
        reset_window_size();
        REFUSE.with(|refuse| refuse.set(refused_frames > 0));
        let mut host = Host::html(page::labels, config);
        let mut batches = vec![host.rebuild().unwrap().to_vec()];
        for frame in 1..=6u64 {
            if frame >= refused_frames {
                REFUSE.with(|refuse| refuse.set(false));
            }
            batches.push(host.render_frame(frame * 16_000_000).unwrap().to_vec());
        }
        let sizes = sizes_by_node(&batches);
        match &expected {
            None => expected = Some(sizes),
            Some(expected) => assert_eq!(
                &sizes, expected,
                "with the renderer refusing {refused_frames} frames the page ended elsewhere"
            ),
        }
    }
}
