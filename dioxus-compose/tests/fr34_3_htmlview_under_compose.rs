//! An `HtmlView` as the only child of a Compose root, the way an application whose window is
//! an HTML page inside a Compose root is built: the page's widget trees get their box, and
//! the page follows every resize of the box, shrinking as well as growing.

use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::measure::{MEASURE_RECORD_LEN, MeasureResult, RESULT_OK};
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch};
use dioxus_compose::schema::{EventPayload, Modifier};
use dioxus_compose::window::reset_window_size;

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

/// A renderer that knows every node and says each one is 10 by 10.
unsafe extern "C" fn measure_every_node(
    _requests: *const u8,
    length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    assert!(length as usize >= count as usize * MEASURE_RECORD_LEN);
    for index in 0..count as usize {
        // SAFETY: the caller hands room for `count` results.
        unsafe {
            results.add(index).write(MeasureResult {
                width: 10.0,
                height: 10.0,
                status: RESULT_OK,
                ..MeasureResult::default()
            });
        }
    }
    0
}

fn install() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: measure_every_node,
    });
    reset_window_size();
}

mod widgets {
    use dioxus_compose::prelude::*;

    /// Draws nothing until the page has given it a box, the way a code editor that must
    /// be told its size does, and then a column of exactly that box.
    pub fn filler() -> Element {
        let (width, height) = dioxus_compose::use_widget_box();
        if width <= 0.0 || height <= 0.0 {
            return rsx! {};
        }
        rsx! {
            Column { width: Some(width), height: Some(height) }
        }
    }
}

mod page {
    use super::widgets;
    use dioxus_compose::html::prelude::*;

    pub fn app() -> Element {
        rsx! {
            div { style: "display: flex; width: 400px; height: 300px",
                div { style: "width: 100px" }
                WidgetView { app: widgets::filler, style: "flex: 1; height: 100%" }
            }
        }
    }

    /// A bar as wide as the page.
    pub fn bar() -> Element {
        rsx! {
            article { style: "display: block; width: 100%; height: 40px; background: #ff0000" }
        }
    }
}

fn widget_screen() -> Element {
    rsx! {
        HtmlView { app: page::app, fill_max_height: true }
    }
}

fn bar_screen() -> Element {
    rsx! {
        HtmlView { app: page::bar, fill_max_height: true }
    }
}

/// The width and height modifiers written for nodes of widget trees.
fn sizes_in_widget_range(mutations: &[Mutation<'_>]) -> (Vec<f32>, Vec<f32>) {
    let (mut widths, mut heights) = (Vec::new(), Vec::new());
    for mutation in mutations {
        if let Mutation::SetModifier {
            node_id, modifier, ..
        } = mutation
            && (0x4000_0000..0x8000_0000).contains(node_id)
        {
            match modifier {
                Modifier::Width(w) => widths.push(*w),
                Modifier::Height(h) => heights.push(*h),
                _ => {}
            }
        }
    }
    (widths, heights)
}

fn widget_sizes_over_frames(mut host: Host) -> (Vec<f32>, Vec<f32>) {
    let (mut widths, mut heights) =
        sizes_in_widget_range(&decode_batch(host.rebuild().unwrap()).unwrap());
    for _ in 0..6 {
        let (w, h) = sizes_in_widget_range(&decode_batch(host.render_frame(0).unwrap()).unwrap());
        widths.extend(w);
        heights.extend(h);
    }
    (widths, heights)
}

#[test]
fn fr34_3_widget_view_in_a_page_that_is_the_root_gets_its_box() {
    install();
    let (widths, heights) = widget_sizes_over_frames(Host::html(page::app, HtmlConfig::default));
    assert!(
        widths.contains(&300.0) && heights.contains(&300.0),
        "the baseline: a page that is the root gives its widget the 300 by 300 box: {widths:?} {heights:?}",
    );
}

#[test]
fn fr34_3_widget_view_inside_html_view_under_a_compose_root_gets_its_box() {
    install();
    let (widths, heights) = widget_sizes_over_frames(Host::new(widget_screen));
    assert!(
        widths.contains(&300.0) && heights.contains(&300.0),
        "the widget never got the 300 by 300 box the flex row left it, so a widget that \
         waits for its box draws nothing: {widths:?} {heights:?}",
    );
}

fn required_widths(mutations: &[Mutation<'_>]) -> Vec<f32> {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                modifier: Modifier::RequiredSize { width, .. },
                ..
            } => Some(*width),
            _ => None,
        })
        .collect()
}

fn resize(host: &mut Host, node_id: u32, width_dp: f32, height_dp: f32) -> Vec<f32> {
    let event = HostEvent::new(
        node_id,
        0,
        EventPayload::WindowSizeChanged {
            width_dp,
            height_dp,
            class: WindowSizeClass::from_width_dp(width_dp),
            height_class: WindowHeightClass::from_height_dp(height_dp),
            safe_area_leading: 0.0,
            safe_area_top: 0.0,
            safe_area_trailing: 0.0,
            safe_area_bottom: 0.0,
        },
    );
    let (batch, _) = host.dispatch(event).unwrap();
    required_widths(&decode_batch(batch).unwrap())
}

#[test]
fn fr34_3_html_view_under_a_compose_root_relayouts_on_every_resize() {
    install();
    let mut host = Host::new(bar_screen);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let node = first
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::ObserveSize { .. },
                ..
            } => Some(*node_id),
            _ => None,
        })
        .expect("the HtmlView asked the renderer to measure its box");
    // Opening size, then shrink, grow and shrink again.
    resize(&mut host, node, 900.0, 600.0);
    for (width, height) in [(500.0, 400.0), (700.0, 500.0), (500.0, 400.0), (300.0, 300.0)] {
        let widths = resize(&mut host, node, width, height);
        assert!(
            widths.contains(&width),
            "the page was not laid out again at {width}: {widths:?}",
        );
    }
}
