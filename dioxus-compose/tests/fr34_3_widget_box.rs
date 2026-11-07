//! A widget tree reads the box the page gave it, so it can fill a box a stylesheet sizes.

use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::measure::{MEASURE_RECORD_LEN, MeasureResult, RESULT_OK};
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::schema::Modifier;
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

mod widgets {
    use dioxus_compose::prelude::*;

    /// Takes the box the page gave it: the widget has nothing to fill without this.
    pub fn filler() -> Element {
        let (width, height) = dioxus_compose::use_widget_box();
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

#[test]
fn fr34_3_widget_reads_the_box_the_page_gave_it() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: measure_every_node,
    });
    reset_window_size();
    let mut host = Host::html(page::app, HtmlConfig::default);
    let (mut widths, mut heights) =
        sizes_in_widget_range(&decode_batch(host.rebuild().unwrap()).unwrap());
    for _ in 0..4 {
        let (w, h) = sizes_in_widget_range(&decode_batch(host.render_frame(0).unwrap()).unwrap());
        widths.extend(w);
        heights.extend(h);
    }
    assert!(
        widths.contains(&300.0) && heights.contains(&300.0),
        "the widget never got the 300 by 300 box the flex row left it: {widths:?} {heights:?}",
    );
}
