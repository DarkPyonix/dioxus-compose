//! HTML screens zoomed as a whole, the way an editor zooms.
//!
//! The renderer reports `k = os * 1.2^level`. A page written with HTML elements is laid
//! out in a viewport of the window's size over `k`, drawn by a renderer that scales its
//! island root by `k`, and measured at `k`. These tests drive a Host the way a window does:
//! they send the renderer's reports as events and read the records the renderer would
//! receive. The widths asserted are the ones the page's boxes are given, in the page's own
//! pixels.
//!
//! The renderer reports the window in the units its widgets are drawn in, which already
//! include the application's share of the zoom, `1.2^level`. A level of zero makes the
//! window's size over `k` the whole story; the tests with a level say so.

use std::cell::Cell;
use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::measure::{
    MEASURE_RECORD_LEN, MEASURE_TEXT, MeasureResult, RESULT_OK, RESULT_UNKNOWN_NODE,
    TEXT_TEXT_LENGTH_AT, TEXT_TEXT_OFFSET_AT, TEXT_ZOOM_AT,
};
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    EventPayload, Modifier, PropertyKind, WidgetKind, WindowHeightClass, WindowSizeClass,
};

thread_local! {
    static CALLS: Cell<usize> = const { Cell::new(0) };
    /// The zoom each text record of this thread's measure calls carried, in order.
    static ZOOMS: std::cell::RefCell<Vec<f32>> = const { std::cell::RefCell::new(Vec::new()) };
}

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Seven pixels a character and twenty-one a line, whatever the zoom.
unsafe extern "C" fn measure_by_the_character(
    requests: *const u8,
    length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    // SAFETY: the caller hands `length` readable bytes and room for `count` results.
    let buffer = unsafe { std::slice::from_raw_parts(requests, length as usize) };
    CALLS.with(|calls| calls.set(calls.get() + 1));
    for index in 0..count as usize {
        let record = &buffer[index * MEASURE_RECORD_LEN..(index + 1) * MEASURE_RECORD_LEN];
        let result = if u16::from_le_bytes([record[0], record[1]]) == MEASURE_TEXT {
            ZOOMS.with_borrow_mut(|zooms| {
                zooms.push(f32::from_bits(read_u32(record, TEXT_ZOOM_AT)));
            });
            let at = read_u32(record, TEXT_TEXT_OFFSET_AT) as usize;
            let len = read_u32(record, TEXT_TEXT_LENGTH_AT) as usize;
            let text = std::str::from_utf8(&buffer[at..at + len]).expect("the text is UTF-8");
            MeasureResult {
                width: text.chars().count() as f32 * 7.0,
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

fn install() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: measure_by_the_character,
    });
    CALLS.with(|calls| calls.set(0));
    ZOOMS.with_borrow_mut(Vec::clear);
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![
            "html, body { margin: 0; padding: 0; } \
             .bar { display: block; width: 100%; height: 40px; background: #ff0000; } \
             .a { display: block; width: 100px; height: 10px; } \
             @media (min-width: 1000px) { .a { width: 200px; } } \
             .b { display: block; width: 150px; height: 11px; } \
             @media (min-resolution: 1.4dppx) { .b { width: 300px; } }"
                .to_string(),
        ],
        ..HtmlConfig::default()
    }
}

fn page() -> Element {
    rsx! {
        div { class: "bar" }
        div { class: "a" }
        div { class: "b" }
    }
}

fn window_event(width_dp: f32, height_dp: f32) -> HostEvent<'static> {
    HostEvent {
        node_id: 0,
        handler_id: 0,
        payload: EventPayload::WindowSizeChanged {
            width_dp,
            height_dp,
            class: WindowSizeClass::from_width_dp(width_dp),
            height_class: WindowHeightClass::from_height_dp(height_dp),
        },
    }
}

fn zoom_event(k: f32, os: f32, level: i32) -> HostEvent<'static> {
    HostEvent {
        node_id: 0,
        handler_id: 0,
        payload: EventPayload::ZoomChanged { k, os, level },
    }
}

/// What a batch said, owned, so it outlives the Host call it came from.
#[derive(Debug, Default)]
struct Seen {
    /// Every `RequiredSize` a node was given.
    sizes: Vec<(f32, f32)>,
    /// The nodes that were given a modifier.
    modified: Vec<u32>,
    /// The nodes the renderer was asked to measure.
    observed: Vec<u32>,
    /// Every property written, with its value where it is a number.
    props: Vec<(u32, PropertyKind, Option<f32>)>,
    created: Vec<(u32, WidgetKind)>,
    texts: Vec<String>,
    /// The zoom level every window record in the batch asked for.
    levels: Vec<Option<i8>>,
    clickable: Vec<(u32, u64)>,
}

impl Seen {
    fn island_zooms(&self) -> Vec<(u32, f32)> {
        self.props
            .iter()
            .filter(|&&(_, property, _)| property == PropertyKind::IslandZoom)
            .map(|&(node, _, value)| (node, value.unwrap_or(f32::NAN)))
            .collect()
    }

    fn has_size(&self, width: f32, height: f32) -> bool {
        self.sizes
            .iter()
            .any(|&(w, h)| (w - width).abs() < 0.01 && (h - height).abs() < 0.01)
    }

    fn widths_of_height(&self, height: f32) -> Vec<f32> {
        self.sizes
            .iter()
            .filter(|&&(_, h)| (h - height).abs() < 0.01)
            .map(|&(w, _)| w)
            .collect()
    }
}

fn seen(bytes: &[u8]) -> Seen {
    let mut seen = Seen::default();
    for mutation in decode_batch(bytes).expect("the batch decodes") {
        match mutation {
            Mutation::Create { node_id, widget } => seen.created.push((node_id, widget)),
            Mutation::SetModifier {
                node_id, modifier, ..
            } => {
                seen.modified.push(node_id);
                match modifier {
                    Modifier::ObserveSize { .. } => seen.observed.push(node_id),
                    Modifier::RequiredSize { width, height } => seen.sizes.push((width, height)),
                    Modifier::Clickable { handler_id } => seen.clickable.push((node_id, handler_id)),
                    _ => {}
                }
            }
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => {
                if let PropertyValue::String(text) = value
                    && property == PropertyKind::Text
                {
                    seen.texts.push(text.to_string());
                }
                seen.props.push((
                    node_id,
                    property,
                    match value {
                        PropertyValue::Float(number) => Some(number),
                        _ => None,
                    },
                ));
            }
            Mutation::SetWindow(window) => seen.levels.push(window.zoom_level),
            _ => {}
        }
    }
    seen
}

fn started() -> (Host, Seen) {
    install();
    let mut host = Host::html(page, config);
    let first = seen(host.rebuild().unwrap());
    (host, first)
}

/// The batch a report produced.
fn report(host: &mut Host, event: HostEvent<'_>) -> Seen {
    let (batch, _) = host.dispatch(event).unwrap();
    seen(batch)
}

/// The page is laid out in the window's size over `k`: at 1200 wide, 960, 800 and 600.
#[test]
fn fr34_4_css_viewport_is_the_window_over_k() {
    for (k, expected) in [(1.25_f32, 960.0_f32), (1.5, 800.0), (2.0, 600.0)] {
        let (mut host, _) = started();
        let plain = report(&mut host, window_event(1200.0, 900.0));
        assert!(
            plain.has_size(1200.0, 40.0),
            "at k = 1 the bar is the window wide: {:?}",
            plain.sizes
        );
        let zoomed = report(&mut host, zoom_event(k, k, 0));
        assert!(
            zoomed.has_size(expected, 40.0),
            "at k = {k} the bar is {expected} wide, the window over k: {:?}",
            zoomed.sizes
        );
    }
}

/// Window sizes arrive in zoomed dp, so the viewport is the window over the system's text
/// size `os`, which equals the window over `k` at level 0. Level 2 at `os` = 1.25 is
/// k = 1.8, and a window of 1200 zoomed dp is 960 page pixels wide.
#[test]
fn fr34_4_the_application_share_is_not_divided_out_twice() {
    let (mut host, _) = started();
    report(&mut host, window_event(1200.0, 900.0));
    let zoomed = report(&mut host, zoom_event(1.25 * 1.44, 1.25, 2));
    assert!(
        zoomed.has_size(960.0, 40.0),
        "1200 zoomed dp over os = 1.25 are 960 page pixels: {:?}",
        zoomed.sizes
    );
}

/// The root of the page names `k` for the renderer to draw it at, and names it again only
/// when it changes.
#[test]
fn fr34_4_island_zoom_is_written_on_the_window_box() {
    install();
    let mut host = Host::html(page, config);
    let first = seen(host.rebuild().unwrap());
    let boxes: Vec<u32> = first
        .created
        .iter()
        .filter(|(_, widget)| *widget == WidgetKind::AbsoluteBox)
        .map(|(node, _)| *node)
        .collect();
    let zooms = first.island_zooms();
    assert_eq!(zooms.len(), 1, "one island root names the factor: {zooms:?}");
    assert_eq!(zooms[0].1, 1.0, "before the renderer reports anything it is one");
    assert!(
        boxes.contains(&zooms[0].0),
        "the factor is on an AbsoluteBox, the only node the renderer reads it from"
    );
    let island = zooms[0].0;

    let zoomed = report(&mut host, zoom_event(1.5, 1.5, 0));
    assert_eq!(zoomed.island_zooms(), vec![(island, 1.5)]);

    let (batch, _) = host.dispatch(zoom_event(1.5, 1.5, 0)).unwrap();
    assert!(
        seen(batch).island_zooms().is_empty(),
        "the same report twice is one change"
    );
    let frame = seen(host.render_frame(0).unwrap());
    assert!(
        frame.island_zooms().is_empty(),
        "a frame at the same factor writes nothing"
    );
}

/// A change of `k` measures the text again, once, at the new `k`; a frame after it asks
/// the renderer nothing.
#[test]
fn fr34_4_a_zoom_change_clears_the_measure_cache_and_lays_out_once() {
    fn labels() -> Element {
        rsx! {
            div { style: "display: flex; width: 700px",
                span { "alpha" }
                span { "bravo!" }
                span { "charlie" }
            }
        }
    }
    fn plain() -> HtmlConfig {
        HtmlConfig {
            stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
            ..HtmlConfig::default()
        }
    }

    install();
    let mut host = Host::html(labels, plain);
    host.rebuild().unwrap();
    let settled = CALLS.with(Cell::get);
    assert!(settled >= 1, "the renderer was never asked");
    assert!(
        ZOOMS.with_borrow(|zooms| zooms.iter().all(|&zoom| zoom == 1.0)),
        "before any report the text is measured at one"
    );

    host.render_frame(0).unwrap();
    assert_eq!(CALLS.with(Cell::get), settled, "a frame at the same zoom asks nothing");

    ZOOMS.with_borrow_mut(Vec::clear);
    host.dispatch(zoom_event(1.5, 1.5, 0)).unwrap();
    let after = CALLS.with(Cell::get);
    assert!(after > settled, "the zoom changed and the text was not measured again");
    assert!(
        after - settled <= 3,
        "{} measure calls for one change of zoom: the page asks per node, not per pass",
        after - settled
    );
    assert!(
        ZOOMS.with_borrow(|zooms| !zooms.is_empty() && zooms.iter().all(|&zoom| zoom == 1.5)),
        "every question after the change is asked at 1.5: {:?}",
        ZOOMS.with_borrow(Clone::clone)
    );

    host.render_frame(0).unwrap();
    host.dispatch(zoom_event(1.5, 1.5, 0)).unwrap();
    assert_eq!(
        CALLS.with(Cell::get),
        after,
        "after the one relayout nothing is asked again at the same zoom"
    );
}

/// A `width` query is answered with the window over `k`, a `resolution` query with `k`, as
/// a browser's zoom answers them.
#[test]
fn fr34_4_media_queries_follow_the_zoom() {
    let (mut host, first) = started();
    assert_eq!(first.widths_of_height(10.0), vec![100.0], "800 wide is below min-width: 1000px");
    assert_eq!(first.widths_of_height(11.0), vec![150.0], "one dppx is below min-resolution: 1.4dppx");
    let plain = report(&mut host, window_event(1200.0, 900.0));
    assert_eq!(plain.widths_of_height(10.0), vec![200.0], "1200 wide matches min-width: 1000px");

    let zoomed = report(&mut host, zoom_event(1.5, 1.5, 0));
    assert_eq!(
        zoomed.widths_of_height(10.0),
        vec![100.0],
        "the viewport is 800 at k = 1.5, below min-width: 1000px"
    );
    assert_eq!(
        zoomed.widths_of_height(11.0),
        vec![300.0],
        "1.5 dppx matches min-resolution: 1.4dppx"
    );

    let back = report(&mut host, zoom_event(1.0, 1.0, 0));
    assert_eq!(back.widths_of_height(10.0), vec![200.0]);
    assert_eq!(back.widths_of_height(11.0), vec![150.0]);
}

fn with_field() -> Element {
    rsx! {
        div { class: "bar" }
        input { style: "width: 200px; height: 24px; font-size: 16px", value: "x" }
    }
}

/// A field inside the page is sent at the page's own pixels whatever `k` is. The renderer
/// draws the island at `k` with a font scale of one, so the Host scales nothing: a zoom
/// changes the root's factor and the boxes that depend on the viewport, and no field gets a
/// record of its own.
#[test]
fn fr34_4_a_text_field_in_the_page_is_not_scaled_twice() {
    install();
    let mut host = Host::html(with_field, config);
    let first = seen(host.rebuild().unwrap());
    let field = first
        .created
        .iter()
        .find_map(|&(node, widget)| (widget == WidgetKind::TextField).then_some(node))
        .expect("the input is a TextField");
    assert!(
        first
            .props
            .contains(&(field, PropertyKind::FontSize, Some(16.0))),
        "the field is sent at its CSS font size: {:?}",
        first.props
    );
    assert!(first.has_size(200.0, 24.0));

    let zoomed = report(&mut host, zoom_event(2.0, 2.0, 0));
    assert!(
        !zoomed.modified.contains(&field),
        "the field's size was not rescaled: {:?}",
        zoomed.sizes
    );
    assert!(
        !zoomed.props.iter().any(|&(node, _, _)| node == field),
        "the field's font was not rescaled: {:?}",
        zoomed.props
    );
    assert!(
        zoomed
            .props
            .iter()
            .all(|&(_, property, _)| property == PropertyKind::IslandZoom),
        "the only property a zoom writes is the island's factor: {:?}",
        zoomed.props
    );
}

fn zoom_buttons() -> Element {
    let zoom = use_zoom();
    let Zoom { k, level, .. } = zoom.get();
    rsx! {
        div {
            style: "width: 100px; height: 20px",
            onclick: move |_| {
                zoom.zoom_in();
                zoom.zoom_in();
            },
            "k {k:.2} level {level}"
        }
    }
}

/// The hook reads the report and wakes the page when it changes, and the setter's request
/// leaves on the window record in the batch the click produced.
#[test]
fn fr34_4_use_zoom_reads_the_report_and_sets_the_level() {
    install();
    let mut host = Host::html(zoom_buttons, HtmlConfig::default);
    let first = seen(host.rebuild().unwrap());
    assert!(first.texts.contains(&"k 1.00 level 0".to_string()), "{:?}", first.texts);
    let (node, handler) = first.clickable[0];

    let reported = report(&mut host, zoom_event(1.8, 1.25, 2));
    assert!(
        reported.texts.contains(&"k 1.80 level 2".to_string()),
        "the page read the report: {:?}",
        reported.texts
    );

    let (batch, _) = host
        .dispatch(HostEvent {
            node_id: node,
            handler_id: handler,
            payload: EventPayload::Clicked,
        })
        .unwrap();
    assert_eq!(
        seen(batch).levels,
        vec![Some(4)],
        "two steps up from the level the renderer reported is one request for level 4"
    );
}

fn set_level_button() -> Element {
    let zoom = use_zoom();
    rsx! {
        div {
            style: "width: 100px; height: 20px",
            onclick: move |_| zoom.set_level(3),
            "set"
        }
    }
}

#[test]
fn fr34_4_set_level_reaches_the_window_record() {
    install();
    let mut host = Host::html(set_level_button, HtmlConfig::default);
    let first = seen(host.rebuild().unwrap());
    let (node, handler) = first.clickable[0];
    assert!(first.levels.iter().all(Option::is_none), "nothing asked at start");
    let (batch, _) = host
        .dispatch(HostEvent {
            node_id: node,
            handler_id: handler,
            payload: EventPayload::Clicked,
        })
        .unwrap();
    assert_eq!(seen(batch).levels, vec![Some(3)]);
}

fn wide_bar() -> Element {
    rsx! {
        div { style: "display: block; width: 100%; height: 40px; background: #ff0000" }
    }
}

mod widgets {
    use dioxus_compose::prelude::*;

    pub fn screen() -> Element {
        rsx! {
            Column {
                HtmlView { app: super::wide_bar, config: Some(super::config), height: 200.0 }
            }
        }
    }
}

/// An HTML island among Compose widgets is drawn at `k` too: its box names the factor, and
/// its page is laid out in the box's width over the system's share of it.
#[test]
fn fr34_4_an_htmlview_names_the_zoom_and_lays_out_in_its_own_viewport() {
    install();
    let mut host = Host::new(widgets::screen);
    let first = seen(host.rebuild().unwrap());
    let observed = first
        .observed
        .first()
        .copied()
        .expect("the island's box is measured");
    let zooms = first.island_zooms();
    assert_eq!(zooms.len(), 1, "the island's box names the factor: {zooms:?}");
    assert_eq!(zooms[0].1, 1.0);

    // The box measures 420 units. Reported at 1.5 with no application zoom, its page is
    // 280 pixels wide.
    let measured = HostEvent {
        node_id: observed,
        handler_id: 0,
        payload: EventPayload::WindowSizeChanged {
            width_dp: 420.0,
            height_dp: 200.0,
            class: WindowSizeClass::from_width_dp(420.0),
            height_class: WindowHeightClass::from_height_dp(200.0),
        },
    };
    // A widget app's Host answers the zoom report itself, so it reaches the island with no
    // error from the widget path.
    let (batch, _) = host.dispatch(measured).unwrap();
    let plain = seen(batch);
    assert!(
        plain.sizes.iter().any(|&(w, _)| (w - 420.0).abs() < 0.01),
        "laid out at 420 before any zoom: {:?}",
        plain.sizes
    );

    let zoomed = report(&mut host, zoom_event(1.5, 1.5, 0));
    assert_eq!(
        zoomed.island_zooms(),
        vec![(zooms[0].0, 1.5)],
        "the box names the new factor"
    );
    assert!(
        zoomed.sizes.iter().any(|&(w, _)| (w - 280.0).abs() < 0.01),
        "the page is laid out in 420 / 1.5 pixels: {:?}",
        zoomed.sizes
    );
}
