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

thread_local! {
    /// Every value the props widget was drawn with, in order.
    static DRAWN: std::cell::RefCell<Vec<i32>> = const { std::cell::RefCell::new(Vec::new()) };
    /// How many times the props widget's hook state was made.
    static MADE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

mod prop_widgets {
    use super::{DRAWN, MADE};
    use dioxus_compose::prelude::*;

    #[component]
    pub fn Shown(value: i32) -> Element {
        use_hook(|| MADE.with(|made| made.set(made.get() + 1)));
        DRAWN.with(|drawn| drawn.borrow_mut().push(value));
        rsx! {
            Text { text: "value {value}" }
        }
    }
}

mod prop_page {
    use super::prop_widgets::{Shown, ShownProps};
    use dioxus_compose::html::prelude::*;
    use dioxus_hooks::use_signal;

    /// A click adds one to the count; the widget is given `count / step`, so with a large
    /// step a click changes the page but not the widget's props.
    fn page(step: i32) -> Element {
        let mut count = use_signal(|| 0);
        let value = count() / step;
        rsx! {
            div { style: "width: 400px",
                div { onclick: move |_| count += 1, "count {count}" }
                WidgetView { app: Shown, props: ShownProps { value }, style: "display: block" }
            }
        }
    }

    pub fn changing() -> Element {
        page(1)
    }

    pub fn steady() -> Element {
        page(1000)
    }
}

/// The click handler of the page's counting div, a `Clickable` modifier with a small id.
fn page_click(mutations: &[Mutation<'_>]) -> Option<(u32, u64)> {
    mutations.iter().find_map(|mutation| match mutation {
        Mutation::SetModifier {
            node_id,
            modifier: Modifier::Clickable { handler_id },
            ..
        } if *handler_id < 1 << 40 => Some((*node_id, *handler_id)),
        _ => None,
    })
}

fn prop_host(app: fn() -> dioxus_compose::Element) -> Host {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: measure_every_node,
    });
    reset_window_size();
    DRAWN.with(|drawn| drawn.borrow_mut().clear());
    MADE.with(|made| made.set(0));
    Host::html(app, HtmlConfig::default)
}

/// Whether `batch` changes a node of a widget tree.
fn writes_widget(batch: &[Mutation<'_>]) -> bool {
    batch.iter().any(|mutation| {
        matches!(mutation, Mutation::SetProp { node_id, .. } | Mutation::SetText { node_id, .. }
            if (0x4000_0000..0x8000_0000).contains(node_id))
    })
}

/// Clicks the page's counting div twice, rendering frames after each click, and returns
/// whether any batch after the first wrote a widget node.
fn click_twice(host: &mut Host, click: (u32, u64)) -> bool {
    let mut wrote_widget = false;
    for _ in 0..2 {
        let (batch, _) = host
            .dispatch(dioxus_compose::protocol::HostEvent {
                node_id: click.0,
                handler_id: click.1,
                payload: dioxus_compose::schema::EventPayload::Clicked,
            })
            .unwrap();
        wrote_widget |= writes_widget(&decode_batch(batch).unwrap());
        for _ in 0..3 {
            let batch = decode_batch(host.render_frame(0).unwrap()).unwrap();
            wrote_widget |= writes_widget(&batch);
        }
    }
    wrote_widget
}

#[test]
fn fr34_3_widget_view_passes_props_to_the_tree() {
    let mut host = prop_host(prop_page::changing);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    assert_eq!(DRAWN.with(|drawn| drawn.borrow().clone()), vec![0]);
    assert!(
        first.iter().any(
            |mutation| matches!(mutation, Mutation::SetProp { node_id, .. }
            if (0x4000_0000..0x8000_0000).contains(node_id))
        ),
        "the widget tree drawn with its props wrote nothing",
    );
}

#[test]
fn fr34_3_changed_props_redraw_the_tree_and_keep_its_state() {
    let mut host = prop_host(prop_page::changing);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let click = page_click(&first).expect("the counting div is not clickable");
    let wrote = click_twice(&mut host, click);
    let drawn = DRAWN.with(|drawn| drawn.borrow().clone());
    assert!(wrote, "the new props were not written: drawn {drawn:?}");
    assert_eq!(
        drawn.last(),
        Some(&2),
        "the tree did not draw with the new props: {drawn:?}"
    );
    assert!(drawn.contains(&1), "a change was missed: {drawn:?}");
    assert_eq!(
        MADE.with(std::cell::Cell::get),
        1,
        "the tree's hook state was made again",
    );
}

#[test]
fn fr34_3_equal_props_do_not_redraw_the_tree() {
    let mut host = prop_host(prop_page::steady);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    let click = page_click(&first).expect("the counting div is not clickable");
    click_twice(&mut host, click);
    assert_eq!(
        DRAWN.with(|drawn| drawn.borrow().clone()),
        vec![0],
        "the tree drew again with props equal to the last ones",
    );
}
