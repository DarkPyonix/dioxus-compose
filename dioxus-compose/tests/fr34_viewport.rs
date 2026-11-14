//! The viewport a whole-screen page is laid out in is the area the window draws it in.
//!
//! These tests drive a Host the way a window does: they send the renderer's size reports as
//! events and read the records the renderer would receive. A page that fills the window
//! is laid out at the size of that area as soon as the renderer has said what it is, and
//! not at the size the Host guesses before it hears anything.
//!
//! The renderer reports two sizes. The window's measures the whole window, title band
//! included, and names the band's height as `safe_area_top`; it is sent only when the
//! window's size class changes, so a narrow window may never send one. The page's own
//! box, which the page asks to have measured, fills the area below the band and is
//! reported on its first layout.

use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::measure::{MeasureResult, RESULT_UNKNOWN_NODE};
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch};
use dioxus_compose::schema::{
    EventPayload, Modifier, WidgetKind, WindowHeightClass, WindowSizeClass,
};

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

/// The pages here have no text, so nothing is measured.
unsafe extern "C" fn measure_nothing(
    _requests: *const u8,
    _length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    for index in 0..count as usize {
        // SAFETY: `index` is below `count`.
        unsafe {
            results.add(index).write(MeasureResult {
                status: RESULT_UNKNOWN_NODE,
                ..MeasureResult::default()
            })
        };
    }
    0
}

fn install() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: measure_nothing,
    });
}

/// A bar across the top and, under it, a page that fills the viewport's height with a
/// 22 pixel status bar pinned to its bottom, the way an editor's window is laid out.
fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![
            "html, body { margin: 0; padding: 0; height: 100%; } \
             .app { display: grid; grid-template-rows: 1fr 22px; height: 100%; } \
             .main { background: #ffffff; } \
             .status { background: #0000ff; }"
                .to_string(),
        ],
        ..HtmlConfig::default()
    }
}

fn page() -> Element {
    rsx! {
        div { class: "app",
            div { class: "main" }
            div { class: "status" }
        }
    }
}

fn size_event(node_id: u32, width_dp: f32, height_dp: f32, band: f32) -> HostEvent<'static> {
    HostEvent {
        node_id,
        handler_id: 0,
        payload: EventPayload::WindowSizeChanged {
            width_dp,
            height_dp,
            class: WindowSizeClass::from_width_dp(width_dp),
            height_class: WindowHeightClass::from_height_dp(height_dp),
            safe_area_leading: 0.0,
            safe_area_top: band,
            safe_area_trailing: 0.0,
        },
    }
}

/// What a batch said, owned, so it outlives the Host call it came from.
#[derive(Debug, Default)]
struct Seen {
    /// Every `RequiredSize` a node was given.
    sizes: Vec<(f32, f32)>,
    /// The nodes the renderer was asked to measure.
    observed: Vec<u32>,
    created: Vec<(u32, WidgetKind)>,
}

impl Seen {
    fn has_size(&self, width: f32, height: f32) -> bool {
        self.sizes
            .iter()
            .any(|&(w, h)| (w - width).abs() < 0.01 && (h - height).abs() < 0.01)
    }

    fn has_width(&self, width: f32) -> bool {
        self.sizes.iter().any(|&(w, _)| (w - width).abs() < 0.01)
    }
}

fn seen(bytes: &[u8]) -> Seen {
    let mut seen = Seen::default();
    for mutation in decode_batch(bytes).expect("the batch decodes") {
        match mutation {
            Mutation::Create { node_id, widget } => seen.created.push((node_id, widget)),
            Mutation::SetModifier {
                node_id, modifier, ..
            } => match modifier {
                Modifier::ObserveSize { .. } => seen.observed.push(node_id),
                Modifier::RequiredSize { width, height } => seen.sizes.push((width, height)),
                _ => {}
            },
            _ => {}
        }
    }
    seen
}

fn report(host: &mut Host, event: HostEvent<'_>) -> Seen {
    let (batch, _) = host.dispatch(event).unwrap();
    seen(batch)
}

/// The page's box, which the first batch asks the renderer to measure.
fn page_box(first: &Seen) -> u32 {
    let observed = first
        .observed
        .first()
        .copied()
        .expect("the page asks the renderer to measure the box it is drawn in");
    assert!(
        first.created.contains(&(observed, WidgetKind::AbsoluteBox)),
        "the measured node is the page's own box: {:?}",
        first.created
    );
    observed
}

/// A narrow window, such as the to-do sample's 520 by 640, is in the size class the Host
/// assumes before it hears anything, so the renderer never reports the window. The page's
/// box is reported on its first layout, and the page is laid out at that size.
#[test]
fn fr34_the_page_is_laid_out_again_when_its_box_reports_its_size() {
    install();
    let mut host = Host::html(page, config);
    let first = seen(host.rebuild().unwrap());
    let island = page_box(&first);

    let measured = report(&mut host, size_event(island, 520.0, 574.0, 0.0));
    assert!(
        measured.has_size(520.0, 574.0),
        "the page is given the box's size, 520 by 574: {:?}",
        measured.sizes
    );
    assert!(
        measured.has_size(520.0, 552.0),
        "the page's content row fills the height less the 22 pixel status bar: {:?}",
        measured.sizes
    );
    assert!(
        !measured.has_width(800.0),
        "nothing is laid out at the 800 pixel guess any more: {:?}",
        measured.sizes
    );
}

/// The window's report measures the whole window, title band included. The page is drawn
/// below the band, so it is laid out in the window's height less the band, and a status
/// bar pinned to the bottom of a full-height page is inside the window.
#[test]
fn fr34_the_page_is_laid_out_below_the_title_band() {
    install();
    let mut host = Host::html(page, config);
    host.rebuild().unwrap();

    let window = report(&mut host, size_event(0, 1100.0, 700.0, 66.0));
    assert!(
        window.has_size(1100.0, 634.0),
        "the page is 700 less the 66 pixel band tall: {:?}",
        window.sizes
    );
    assert!(
        window.has_size(1100.0, 612.0),
        "the content row ends where the status bar starts, 22 pixels above the bottom: {:?}",
        window.sizes
    );
    assert!(
        !window.has_size(1100.0, 700.0),
        "the band is not part of the page: {:?}",
        window.sizes
    );
}

/// The box is measured on every dp change, not only when its size class changes, since it
/// is the area the page is drawn in.
#[test]
fn fr34_the_page_box_is_measured_on_every_dp_change() {
    install();
    let mut host = Host::html(page, config);
    let first = decode_batch(host.rebuild().unwrap()).expect("the batch decodes");
    let every_dp: Vec<bool> = first
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                modifier: Modifier::ObserveSize { every_dp, .. },
                ..
            } => Some(*every_dp),
            _ => None,
        })
        .collect();
    assert_eq!(every_dp, vec![true], "one box, observed at every dp");
}

/// Once the box has reported, the window's report no longer moves the page. A window whose
/// content runs under its title bar (a full-size content view) gives the box the whole
/// window, and its report still carries the title band as `safe_area_top`: taking the band
/// out of the box's height left an empty strip at the bottom of the window.
#[test]
fn fr34_the_box_report_outranks_the_window_report() {
    install();
    let mut host = Host::html(page, config);
    let first = seen(host.rebuild().unwrap());
    let island = page_box(&first);

    let boxed = report(&mut host, size_event(island, 1440.0, 900.0, 0.0));
    assert!(
        boxed.has_size(1440.0, 900.0),
        "the page fills the box, 1440 by 900: {:?}",
        boxed.sizes
    );

    let window = report(&mut host, size_event(0, 1440.0, 900.0, 66.0));
    assert!(
        !window.has_size(1440.0, 834.0),
        "the band is not taken out of a box that already reported: {:?}",
        window.sizes
    );

    let narrower = report(&mut host, size_event(island, 1000.0, 900.0, 0.0));
    assert!(
        narrower.has_size(1000.0, 900.0),
        "a resize reaches the page through the box: {:?}",
        narrower.sizes
    );
    let stale = report(&mut host, size_event(0, 1200.0, 900.0, 66.0));
    assert!(
        !stale.has_width(1200.0),
        "a window report after the box's does not move the page: {:?}",
        stale.sizes
    );
}
