//! HTML drag and drop in the HTML path: `dragstart`, `dragenter`, `dragover`, `dragleave`,
//! `drop` and `dragend`, made from the pointer reports the renderer sends for the node
//! that took the press. Positions follow from the CSS in the fixture.

use std::cell::RefCell;

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{HtmlConfig, HtmlDom, KeyInput, PointerInput, PointerKind};
use dioxus_elements::DragEvent;
use dioxus_html::{Code, InteractionElementOffset, InteractionLocation, Key, Modifiers};

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

const SHEET: &str = "
html, body { margin: 0; padding: 0; }
#board { position: absolute; left: 0; top: 0; width: 400px; height: 100px; }
#src { position: absolute; left: 0; top: 0; width: 50px; height: 50px; }
#zone { position: absolute; left: 100px; top: 0; width: 50px; height: 50px; }
#plain { position: absolute; left: 200px; top: 0; width: 50px; height: 50px; }
#veil { position: absolute; left: 200px; top: 0; width: 50px; height: 50px;
        pointer-events: none; }
#still { position: absolute; left: 300px; top: 0; width: 50px; height: 50px; }
";

/// The text a drag event's `dataTransfer` holds, or `-` when it holds none.
fn text(evt: &DragEvent) -> String {
    evt.data_transfer()
        .get_data("text/plain")
        .unwrap_or_else(|| "-".to_string())
}

fn page() -> Element {
    rsx! {
        div { id: "board",
            ondrop: move |_| log("drop board"),
            ondragend: move |_| log("dragend board"),
            div { id: "src", draggable: "true",
                ondragstart: move |e| {
                    let at = e.client_coordinates();
                    e.data_transfer().set_data("text/plain", "tab-1").unwrap();
                    log(format!("dragstart {} {}", at.x, at.y));
                },
                ondragend: move |e| log(format!("dragend src {}", text(&e))),
                onclick: move |_| log("click src"),
            }
            div { id: "zone",
                ondragenter: move |_| log("enter zone"),
                ondragover: move |e| {
                    let (at, own) = (e.client_coordinates(), e.element_coordinates());
                    log(format!("over zone {} {} / {} {}", at.x, at.y, own.x, own.y));
                    e.prevent_default();
                },
                ondragleave: move |_| log("leave zone"),
                ondrop: move |e| log(format!("drop zone {}", text(&e))),
            }
            div { id: "plain",
                ondragenter: move |_| log("enter plain"),
                ondragover: move |_| log("over plain"),
                ondragleave: move |_| log("leave plain"),
                ondrop: move |_| log("drop plain"),
            }
            div { id: "veil",
                ondragenter: move |_| log("enter veil"),
                ondragover: move |e| {
                    log("over veil");
                    e.prevent_default();
                },
                ondrop: move |_| log("drop veil"),
            }
            div { id: "still",
                ondragstart: move |_| log("dragstart still"),
                onclick: move |_| log("click still"),
            }
        }
    }
}

fn dom() -> HtmlDom {
    let mut dom = HtmlDom::with_config(
        page,
        HtmlConfig {
            stylesheets: vec![SHEET.to_string()],
            ..HtmlConfig::default()
        },
    );
    dom.layout(400.0, 300.0, 1.0);
    take_log();
    dom
}

/// A report for `#src`, the node that takes the press and so keeps the pointer until the
/// release. `#src` is at the page's corner, so its own space is the page's.
fn report(dom: &mut HtmlDom, input: PointerInput) -> bool {
    let src = dom.element_by_id("src").unwrap();
    dom.pointer_event(src, &input)
}

fn press(dom: &mut HtmlDom, x: f32, y: f32) -> bool {
    report(dom, PointerInput::press(x, y, 1, 1))
}

fn drag_to(dom: &mut HtmlDom, x: f32, y: f32) -> bool {
    report(
        dom,
        PointerInput {
            buttons: 1,
            ..PointerInput::new(PointerKind::Move, x, y)
        },
    )
}

fn release(dom: &mut HtmlDom, x: f32, y: f32) -> bool {
    report(dom, PointerInput::release(x, y, 1, 1))
}

/// Presses `#src` and moves far enough for the drag to start.
fn start(dom: &mut HtmlDom) {
    press(dom, 10.0, 10.0);
    drag_to(dom, 20.0, 10.0);
    assert!(dom.is_dragging());
    assert_eq!(take_log(), ["dragstart 10 10"]);
}

#[test]
fn fr34_5_drag_start_carries_text_to_drop() {
    let mut dom = dom();
    // The renderer is asked for the press, the moves, the release and the cancel of a
    // draggable element, which has no mouse or pointer listener of its own.
    let src = dom.element_by_id("src").unwrap();
    let wanted = PointerKind::Down.bit()
        | PointerKind::Move.bit()
        | PointerKind::Up.bit()
        | PointerKind::Cancel.bit();
    let subscribed = dom
        .pointer_subscriptions()
        .into_iter()
        .find(|subscription| subscription.node == src)
        .expect("the draggable element takes pointer reports");
    assert_eq!(subscribed.kinds & wanted, wanted);

    press(&mut dom, 10.0, 10.0);
    // Up to 4 CSS px from the press nothing starts.
    assert!(!drag_to(&mut dom, 13.0, 10.0));
    assert!(!drag_to(&mut dom, 14.0, 10.0));
    assert!(!dom.is_dragging());
    assert_eq!(take_log(), Vec::<String>::new());
    // Past it the element receives `dragstart`, at the place of the press.
    assert!(drag_to(&mut dom, 14.5, 10.0));
    assert!(dom.is_dragging());
    assert_eq!(take_log(), ["dragstart 10 10"]);

    assert!(drag_to(&mut dom, 125.0, 30.0));
    assert_eq!(take_log(), ["enter zone", "over zone 125 30 / 25 30"]);
    // The drop reads what `dragstart` set, and bubbles.
    assert!(release(&mut dom, 125.0, 30.0));
    assert_eq!(
        take_log(),
        [
            "drop zone tab-1",
            "drop board",
            "dragend src tab-1",
            "dragend board"
        ]
    );
    assert!(!dom.is_dragging());
}

#[test]
fn fr34_5_drag_enter_over_leave_follow_the_pointer() {
    let mut dom = dom();
    start(&mut dom);
    drag_to(&mut dom, 110.0, 10.0);
    assert_eq!(take_log(), ["enter zone", "over zone 110 10 / 10 10"]);
    // A move inside the same element is `dragover` alone.
    drag_to(&mut dom, 120.0, 40.0);
    assert_eq!(take_log(), ["over zone 120 40 / 20 40"]);
    // `#veil` lies over `#plain` with `pointer-events: none`, so the pointer is over
    // `#plain`: it is entered before the element left is told.
    drag_to(&mut dom, 225.0, 25.0);
    assert_eq!(take_log(), ["enter plain", "leave zone", "over plain"]);
    drag_to(&mut dom, 226.0, 25.0);
    assert_eq!(take_log(), ["over plain"]);
    // Between the boxes the pointer is over `#board`, which listens for neither.
    drag_to(&mut dom, 175.0, 25.0);
    assert_eq!(take_log(), ["leave plain"]);
    drag_to(&mut dom, 110.0, 10.0);
    assert_eq!(take_log(), ["enter zone", "over zone 110 10 / 10 10"]);
}

#[test]
fn fr34_5_drop_needs_prevent_default_on_dragover() {
    // `#plain` hears `dragover` and does not prevent its default: no drop.
    let mut dom = dom();
    start(&mut dom);
    drag_to(&mut dom, 225.0, 25.0);
    assert_eq!(take_log(), ["enter plain", "over plain"]);
    release(&mut dom, 225.0, 25.0);
    assert_eq!(
        take_log(),
        ["leave plain", "dragend src tab-1", "dragend board"]
    );

    // `#zone` prevented it, but the pointer left for `#plain` before the release: what
    // counts is the last `dragover`, the one under the pointer.
    let mut dom = self::dom();
    start(&mut dom);
    drag_to(&mut dom, 125.0, 25.0);
    drag_to(&mut dom, 225.0, 25.0);
    take_log();
    release(&mut dom, 225.0, 25.0);
    assert_eq!(
        take_log(),
        ["leave plain", "dragend src tab-1", "dragend board"]
    );

    // A release somewhere the last move was not asks the element there first.
    let mut dom = self::dom();
    start(&mut dom);
    drag_to(&mut dom, 225.0, 25.0);
    take_log();
    release(&mut dom, 125.0, 25.0);
    assert_eq!(
        take_log(),
        [
            "enter zone",
            "leave plain",
            "over zone 125 25 / 25 25",
            "drop zone tab-1",
            "drop board",
            "dragend src tab-1",
            "dragend board"
        ]
    );
}

#[test]
fn fr34_5_drag_end_reaches_the_source_after_drop_or_cancel() {
    // After a drop, wherever it was.
    let mut dom = dom();
    start(&mut dom);
    drag_to(&mut dom, 125.0, 25.0);
    take_log();
    release(&mut dom, 125.0, 25.0);
    let heard = take_log();
    assert_eq!(heard[0], "drop zone tab-1");
    assert_eq!(heard[heard.len() - 2], "dragend src tab-1");

    // Escape cancels: `dragleave` and `dragend`, no `drop`, and the key goes no further.
    let mut dom = self::dom();
    start(&mut dom);
    drag_to(&mut dom, 125.0, 25.0);
    take_log();
    let escape = KeyInput::new(Key::Escape, Code::Escape, Modifiers::empty());
    assert!(dom.key_down_input(None, &escape));
    assert_eq!(
        take_log(),
        ["leave zone", "dragend src tab-1", "dragend board"]
    );
    assert!(!dom.is_dragging());
    // The pointer is still down, and what it does until its release is no drag.
    drag_to(&mut dom, 30.0, 25.0);
    release(&mut dom, 30.0, 25.0);
    let src = dom.element_by_id("src").unwrap();
    assert_eq!(dom.click_node(src), None);
    assert_eq!(take_log(), Vec::<String>::new());
    // With no drag on, Escape is an ordinary key.
    assert!(!dom.key_down_input(None, &escape));

    // A pointer the system takes away cancels the same way.
    let mut dom = self::dom();
    start(&mut dom);
    drag_to(&mut dom, 125.0, 25.0);
    take_log();
    report(
        &mut dom,
        PointerInput::new(PointerKind::Cancel, 125.0, 25.0),
    );
    assert_eq!(
        take_log(),
        ["leave zone", "dragend src tab-1", "dragend board"]
    );
    assert!(!dom.is_dragging());
}

#[test]
fn fr34_5_finished_drag_does_not_click() {
    let mut dom = dom();
    let src = dom.element_by_id("src").unwrap();
    // The release is still inside `#src`, where a renderer reports a click.
    start(&mut dom);
    release(&mut dom, 20.0, 10.0);
    take_log();
    assert_eq!(dom.click_node(src), None);
    assert_eq!(dom.click(20.0, 10.0), Some(src), "only that one click");
    assert_eq!(take_log(), ["click src"]);

    // The same when the click is reported before the release.
    start(&mut dom);
    assert_eq!(dom.click_node(src), None);
    release(&mut dom, 20.0, 10.0);
    take_log();
    // The next press is a new one.
    press(&mut dom, 10.0, 10.0);
    release(&mut dom, 10.0, 10.0);
    assert_eq!(dom.click_node(src), Some(src));
    assert_eq!(take_log(), ["click src"]);

    // A press that stays within the threshold is a click and no drag.
    press(&mut dom, 10.0, 10.0);
    drag_to(&mut dom, 13.0, 12.0);
    release(&mut dom, 13.0, 12.0);
    assert_eq!(dom.click_node(src), Some(src));
    assert_eq!(take_log(), ["click src"]);

    // An element without `draggable=\"true\"` never starts one, however far it moves.
    let still = dom.element_by_id("still").unwrap();
    dom.pointer_event(still, &PointerInput::press(10.0, 10.0, 1, 1));
    dom.pointer_event(
        still,
        &PointerInput {
            buttons: 1,
            ..PointerInput::new(PointerKind::Move, 40.0, 10.0)
        },
    );
    dom.pointer_event(still, &PointerInput::release(40.0, 10.0, 1, 1));
    assert!(!dom.is_dragging());
    assert_eq!(dom.click_node(still), Some(still));
    assert_eq!(take_log(), ["click still"]);
}
