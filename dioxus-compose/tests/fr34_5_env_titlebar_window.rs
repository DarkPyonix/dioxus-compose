//! `env(titlebar-area-*)` on a page that fills the window takes the window's title band as
//! the renderer reports it, and follows each new report.
//!
//! The renderer's window report names the band's height as `safe_area_top` and the room
//! the window buttons take at each end as `safe_area_leading` and `safe_area_trailing`.
//! The title bar area is the part of the band beside the buttons: it starts past the
//! leading buttons, ends before the trailing ones and is as tall as the band.

use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::measure::{MeasureResult, RESULT_UNKNOWN_NODE};
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch};
use dioxus_compose::schema::{EventPayload, Modifier, WindowHeightClass, WindowSizeClass};

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

/// The page has no text, so nothing is measured.
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

/// A bar sized and placed only by the four variables, with fallbacks no window report
/// produces, so the size it is given says which values were read.
fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![
            "html, body { margin: 0; } \
             #bar { position: absolute; background: #0000ff; \
                    left: env(titlebar-area-x, 1px); top: env(titlebar-area-y, 2px); \
                    width: env(titlebar-area-width, 111px); \
                    height: env(titlebar-area-height, 13px); }"
                .to_string(),
        ],
        ..HtmlConfig::default()
    }
}

fn page() -> Element {
    rsx! {
        div { id: "bar" }
    }
}

fn window_event(
    width_dp: f32,
    height_dp: f32,
    top: f32,
    leading: f32,
    trailing: f32,
) -> HostEvent<'static> {
    HostEvent {
        node_id: 0,
        handler_id: 0,
        payload: EventPayload::WindowSizeChanged {
            width_dp,
            height_dp,
            class: WindowSizeClass::from_width_dp(width_dp),
            height_class: WindowHeightClass::from_height_dp(height_dp),
            safe_area_top: top,
            safe_area_leading: leading,
            safe_area_trailing: trailing,
        },
    }
}

/// Width and height, or x and y.
type Pairs = Vec<(f32, f32)>;

/// Every `RequiredSize` and `Offset` a batch gave a node.
fn sizes_and_offsets(bytes: &[u8]) -> (Pairs, Pairs) {
    let mut sizes = Vec::new();
    let mut offsets = Vec::new();
    for mutation in decode_batch(bytes).expect("the batch decodes") {
        if let Mutation::SetModifier { modifier, .. } = mutation {
            match modifier {
                Modifier::RequiredSize { width, height } => sizes.push((width, height)),
                Modifier::Offset { x, y } => offsets.push((x, y)),
                _ => {}
            }
        }
    }
    (sizes, offsets)
}

fn has(values: &[(f32, f32)], expected: (f32, f32)) -> bool {
    values
        .iter()
        .any(|&(a, b)| (a - expected.0).abs() < 0.01 && (b - expected.1).abs() < 0.01)
}

#[test]
fn fr34_5_env_titlebar_area_follows_the_window() {
    install();
    let mut host = Host::html(page, config);

    // Before any report there is no band, so each `env()` takes its fallback.
    let (sizes, _) = sizes_and_offsets(host.rebuild().unwrap());
    assert!(has(&sizes, (111.0, 13.0)), "the fallbacks apply: {sizes:?}");

    // A 1100 wide window with a 28 tall band and 78 of buttons at its leading edge: the
    // area is the 1022 beside them, 28 tall, starting 78 in.
    let (batch, _) = host
        .dispatch(window_event(1100.0, 700.0, 28.0, 78.0, 0.0))
        .unwrap();
    let (sizes, offsets) = sizes_and_offsets(batch);
    assert!(
        has(&sizes, (1022.0, 28.0)),
        "the area beside the buttons: {sizes:?}"
    );
    assert!(
        has(&offsets, (78.0, 0.0)),
        "the area starts past the buttons: {offsets:?}"
    );

    // The buttons move to the trailing edge and the band grows: the area follows.
    let (batch, _) = host
        .dispatch(window_event(1100.0, 700.0, 32.0, 0.0, 138.0))
        .unwrap();
    let (sizes, offsets) = sizes_and_offsets(batch);
    assert!(
        has(&sizes, (962.0, 32.0)),
        "the area before the buttons: {sizes:?}"
    );
    assert!(
        has(&offsets, (0.0, 0.0)),
        "the area starts at the leading edge: {offsets:?}"
    );

    // A window with no band has no area, and the fallbacks apply again.
    let (batch, _) = host
        .dispatch(window_event(1100.0, 700.0, 0.0, 0.0, 0.0))
        .unwrap();
    let (sizes, _) = sizes_and_offsets(batch);
    assert!(
        has(&sizes, (111.0, 13.0)),
        "the fallbacks apply again: {sizes:?}"
    );
}
