//! Programmatic scrolling on the HTML path: `MountedData::scroll_to` and `scroll` become
//! the renderer's `ScrollIntoView` and `ScrollTo` requests, and the renderer's
//! `ScrollChanged` reports become `get_scroll_offset`, `get_scroll_size` and `onscroll`.
//! The renderer is the mock one, so these read the records it would receive and send the
//! reports it would send.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use dioxus_compose::Host;
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, PropertyKind, Rect, WidgetKind};
use dioxus_elements::geometry::euclid::Vector2D;
use dioxus_elements::{MountedData, ScrollBehavior, ScrollLogicalPosition, ScrollToOptions};

/// What an `onscroll` handler read: `scrollTop`, `scrollLeft`, `scrollHeight`,
/// `scrollWidth`, `clientHeight` and `clientWidth`.
type Heard = (f64, f64, i32, i32, i32, i32);

thread_local! {
    static MOUNTED: RefCell<HashMap<&'static str, Rc<MountedData>>> =
        RefCell::new(HashMap::new());
    static SCROLLS: RefCell<Vec<Heard>> =
        const { RefCell::new(Vec::new()) };
}

fn mounted(id: &'static str) -> Rc<MountedData> {
    MOUNTED.with(|mounted| {
        mounted
            .borrow()
            .get(id)
            .cloned()
            .unwrap_or_else(|| panic!("#{id} was mounted"))
    })
}

fn keep(id: &'static str, data: Rc<MountedData>) {
    MOUNTED.with(|mounted| mounted.borrow_mut().insert(id, data));
}

/// Polls a future that the HTML path answers at once.
fn now<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the HTML path answers at once"),
    }
}

const SHEET: &str = "
html, body { margin: 0; padding: 0; }
#outer { width: 200px; height: 100px; overflow: auto; }
#spacer { height: 150px; }
#inner { height: 50px; overflow-x: hidden; overflow-y: auto; }
.row { height: 20px; }
";

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![SHEET.to_string()],
        ..HtmlConfig::default()
    }
}

/// Two scroll containers, one inside the other, and a row deep in the inner one. `#outer`
/// scrolls both ways, which the renderer draws as two containers, a row inside a column;
/// `#inner` scrolls only up and down.
fn nested() -> Element {
    rsx! {
        div { id: "outer",
            onmounted: move |evt| keep("outer", evt.data()),
            onscroll: move |evt| {
                let data = evt.data();
                SCROLLS.with(|scrolls| {
                    scrolls.borrow_mut().push((
                        data.scroll_top(),
                        data.scroll_left(),
                        data.scroll_height(),
                        data.scroll_width(),
                        data.client_height(),
                        data.client_width(),
                    ))
                });
            },
            div { id: "spacer" }
            div { id: "inner",
                for index in 0..10 {
                    if index == 7 {
                        div { class: "row", id: "target",
                            onmounted: move |evt| keep("target", evt.data()),
                            "row {index}"
                        }
                    } else {
                        div { class: "row", "row {index}" }
                    }
                }
            }
        }
    }
}

/// What a batch asked of the scroll containers, owned.
#[derive(Debug, Default)]
struct Seen {
    /// The scroll columns created, in the order they were created.
    columns: Vec<u32>,
    /// The scroll rows created.
    rows: Vec<u32>,
    /// Each `OnScrollChanged` handler, by the node it was set on.
    handlers: HashMap<u32, u64>,
    /// `(container, target, target_kind, align_x, align_y, flags, rect)`.
    into_view: Vec<(u32, u32, u16, u16, u16, u16, Rect)>,
    /// `(container, flags, x_px, y_px)`.
    scroll_to: Vec<(u32, u32, f64, f64)>,
    /// Every node the batch created.
    created: Vec<u32>,
}

fn seen(bytes: &[u8]) -> Seen {
    let mut seen = Seen::default();
    for record in decode_batch(bytes).expect("the batch decodes") {
        match record {
            Mutation::Create { node_id, widget } => {
                seen.created.push(node_id);
                if widget == WidgetKind::ScrollColumn {
                    seen.columns.push(node_id);
                }
                if widget == WidgetKind::ScrollRow {
                    seen.rows.push(node_id);
                }
            }
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnScrollChanged,
                value: PropertyValue::Integer(handler),
            } => {
                seen.handlers.insert(node_id, handler as u64);
            }
            Mutation::ScrollIntoView {
                node_id,
                target,
                target_kind,
                align_x,
                align_y,
                flags,
                rect,
            } => seen
                .into_view
                .push((node_id, target, target_kind, align_x, align_y, flags, rect)),
            Mutation::ScrollTo {
                node_id,
                flags,
                x_px,
                y_px,
                ..
            } => seen.scroll_to.push((node_id, flags, x_px, y_px)),
            _ => {}
        }
    }
    seen
}

/// The page's scroll column, then `#outer`'s, then `#inner`'s: the bridge makes the page's
/// first and builds the tree depth first. `#outer`'s row is its only one.
struct Started {
    host: Host,
    page: u32,
    outer: u32,
    outer_row: u32,
    inner: u32,
    handlers: HashMap<u32, u64>,
}

fn started() -> Started {
    MOUNTED.with(|mounted| mounted.borrow_mut().clear());
    SCROLLS.with(|scrolls| scrolls.borrow_mut().clear());
    let mut host = Host::html(nested, config);
    let first = seen(host.rebuild().expect("the first batch encodes"));
    let [page, outer, inner] = first.columns[..] else {
        panic!("three scroll columns: {first:#?}");
    };
    let [outer_row] = first.rows[..] else {
        panic!("one scroll row: {first:#?}");
    };
    assert!(
        [outer, outer_row, inner]
            .iter()
            .all(|node| first.handlers.contains_key(node)),
        "every scroll container reports where it is: {first:#?}"
    );
    Started {
        host,
        page,
        outer,
        outer_row,
        inner,
        handlers: first.handlers,
    }
}

fn frame(host: &mut Host) -> Seen {
    seen(host.render_frame(0).expect("the frame encodes"))
}

fn send(host: &mut Host, event: HostEvent<'_>) -> Seen {
    let (bytes, _) = host.dispatch(event).expect("the event is dispatched");
    seen(bytes)
}

fn zoom(host: &mut Host, k: f32) -> Seen {
    send(
        host,
        HostEvent {
            node_id: 0,
            handler_id: 0,
            payload: EventPayload::ZoomChanged { k, os: k, level: 0 },
        },
    )
}

/// The renderer's report on the vertical scroll `node`, in renderer pixels.
fn report(
    started: &mut Started,
    node: u32,
    y_px: f64,
    max_y_px: f64,
    (width_px, height_px): (f64, f64),
) -> Seen {
    let handler_id = started.handlers[&node];
    send(
        &mut started.host,
        HostEvent {
            node_id: node,
            handler_id,
            payload: EventPayload::ScrollChanged {
                item: 0,
                settled: true,
                can_scroll_backward: y_px > 0.0,
                can_scroll_forward: y_px < max_y_px,
                x_px: 0.0,
                y_px,
                max_x_px: 0.0,
                max_y_px,
                viewport_width_px: width_px,
                viewport_height_px: height_px,
            },
        },
    )
}

const NODE: u16 = 1;
const START: u16 = 1;
const CENTER: u16 = 2;
const END: u16 = 3;
const NEAREST: u16 = 4;
const ANIMATE: u16 = 1;

#[test]
fn fr34_5_scroll_into_view_writes_one_request_per_container() {
    let mut started = started();
    let _ = now(mounted("target").scroll_to(ScrollBehavior::Instant));
    let next = frame(&mut started.host);
    let containers: Vec<u32> = next.into_view.iter().map(|request| request.0).collect();
    assert_eq!(
        containers,
        [
            started.inner,
            started.outer_row,
            started.outer,
            started.page
        ],
        "one request for each container around the row, innermost first, then the page's \
         own scroll. `#outer`'s two axes are two containers, its row inside its column: \
         {next:#?}"
    );
    let target = next.into_view[0].1;
    assert!(
        ![
            started.page,
            started.outer,
            started.outer_row,
            started.inner
        ]
        .contains(&target),
        "the target is the row's own node: {next:#?}"
    );
    for (_, named, kind, align_x, align_y, flags, rect) in &next.into_view {
        assert_eq!(*named, target, "every request names the row");
        assert_eq!(*kind, NODE, "the target is a node");
        assert_eq!(*rect, Rect::default(), "the whole node");
        assert_eq!((*align_x, *align_y), (NEAREST, NEAREST));
        assert_eq!(*flags, 0, "instant does not animate");
    }
    assert!(next.scroll_to.is_empty());
    // Asked once, written once.
    assert!(frame(&mut started.host).into_view.is_empty());
}

#[test]
fn fr34_5_scroll_into_view_uses_the_requested_alignment() {
    let mut started = started();
    let _ = now(mounted("target").scroll_to_with_options(ScrollToOptions {
        behavior: ScrollBehavior::Instant,
        vertical: ScrollLogicalPosition::Center,
        horizontal: ScrollLogicalPosition::End,
    }));
    let next = frame(&mut started.host);
    assert_eq!(next.into_view.len(), 4, "{next:#?}");
    for (_, _, _, align_x, align_y, flags, _) in &next.into_view {
        assert_eq!((*align_x, *align_y), (END, CENTER));
        assert_eq!(*flags, 0);
    }

    let _ = now(mounted("target").scroll_to_with_options(ScrollToOptions {
        behavior: ScrollBehavior::Smooth,
        vertical: ScrollLogicalPosition::Start,
        horizontal: ScrollLogicalPosition::Nearest,
    }));
    let next = frame(&mut started.host);
    for (_, _, _, align_x, align_y, flags, _) in &next.into_view {
        assert_eq!((*align_x, *align_y), (NEAREST, START));
        assert_eq!(*flags, ANIMATE, "smooth animates");
    }

    // `scroll_to(behavior)` is nearest on both axes.
    let _ = now(mounted("target").scroll_to(ScrollBehavior::Smooth));
    let next = frame(&mut started.host);
    assert_eq!(next.into_view.len(), 4, "{next:#?}");
    for (_, _, _, align_x, align_y, flags, _) in &next.into_view {
        assert_eq!((*align_x, *align_y), (NEAREST, NEAREST));
        assert_eq!(*flags, ANIMATE);
    }
}

#[test]
fn fr34_5_scroll_sets_the_container_offset_in_renderer_pixels() {
    let mut started = started();
    // The page is drawn at k = 1.5 on a screen of density 2: a CSS pixel is three
    // renderer pixels.
    zoom(&mut started.host, 1.5);

    // Before any report the Host does not know the density, so the move waits for one.
    let _ = now(mounted("outer").scroll(Vector2D::new(10.0, 40.0), ScrollBehavior::Instant));
    let waiting = frame(&mut started.host);
    assert!(waiting.scroll_to.is_empty(), "{waiting:#?}");

    // `#outer` is 200 by 100 CSS pixels, which the renderer reports as 600 by 300.
    let (outer, row) = (started.outer, started.outer_row);
    let reported = report(&mut started, outer, 0.0, 300.0, (600.0, 300.0));
    assert_eq!(
        reported.scroll_to,
        [(outer, 1 << 2, 0.0, 120.0), (row, 1 << 1, 30.0, 0.0)],
        "40 CSS pixels down is 120 renderer pixels and 10 across is 30. Each axis goes to \
         its own container, which is written only that axis: {reported:#?}"
    );

    // From then on a move goes out in the frame it was asked in, and a smooth one animates.
    let _ = now(mounted("outer").scroll(Vector2D::new(0.0, 25.0), ScrollBehavior::Smooth));
    let next = frame(&mut started.host);
    assert_eq!(
        next.scroll_to,
        [
            (outer, 1 | (1 << 2), 0.0, 75.0),
            (row, 1 | (1 << 1), 0.0, 0.0)
        ]
    );

    // An element that does not scroll is not moved.
    let _ = now(mounted("target").scroll(Vector2D::new(0.0, 25.0), ScrollBehavior::Instant));
    let next = frame(&mut started.host);
    assert!(next.scroll_to.is_empty(), "{next:#?}");
}

#[test]
fn fr34_5_scroll_offset_reads_the_last_report() {
    let mut started = started();
    let outer = mounted("outer");
    let offset = now(outer.get_scroll_offset()).unwrap();
    assert_eq!((offset.x, offset.y), (0.0, 0.0), "no report yet: zero");
    let size = now(outer.get_scroll_size()).unwrap();
    assert_eq!(
        (size.width, size.height),
        (200.0, 200.0),
        "before a report, the laid-out content: the spacer and the inner box"
    );

    // Density 2 at k = 1: the 100 pixel tall viewport is 200 renderer pixels.
    let node = started.outer;
    report(&mut started, node, 120.0, 200.0, (400.0, 200.0));
    let offset = now(outer.get_scroll_offset()).unwrap();
    assert_eq!((offset.x, offset.y), (0.0, 60.0));
    let size = now(outer.get_scroll_size()).unwrap();
    assert_eq!(
        (size.width, size.height),
        (200.0, 200.0),
        "the content is the largest offset plus the viewport"
    );

    report(&mut started, node, 50.0, 200.0, (400.0, 200.0));
    let offset = now(outer.get_scroll_offset()).unwrap();
    assert_eq!(offset.y, 25.0, "the last report wins");

    // An element that does not scroll is at zero.
    let offset = now(mounted("target").get_scroll_offset()).unwrap();
    assert_eq!((offset.x, offset.y), (0.0, 0.0));
}

#[test]
fn fr34_5_scroll_report_reaches_onscroll() {
    let mut started = started();
    SCROLLS.with(|scrolls| scrolls.borrow_mut().clear());
    let node = started.outer;
    report(&mut started, node, 80.0, 200.0, (400.0, 200.0));
    assert_eq!(
        SCROLLS.with(|scrolls| scrolls.borrow().clone()),
        [(40.0, 0.0, 200, 200, 100, 200)],
        "scrollTop, scrollLeft, scrollHeight, scrollWidth, clientHeight, clientWidth in CSS \
         pixels"
    );
    // The inner container's report is its own and does not reach `#outer`'s handler.
    let inner = started.inner;
    report(&mut started, inner, 10.0, 300.0, (400.0, 100.0));
    assert_eq!(SCROLLS.with(|scrolls| scrolls.borrow().len()), 1);
}
