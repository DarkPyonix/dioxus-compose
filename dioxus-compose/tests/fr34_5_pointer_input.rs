//! Pointer input in the HTML path: hover state, mouse, pointer and wheel events, capture,
//! `pointer-events`, and the `cursor` and `user-select` of a node. The reports are the ones
//! the renderer sends for a subscribed node: the node, and a position in its own space.
//! Expected numbers follow from the CSS in each fixture.

use std::cell::RefCell;

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    Cursor, HtmlConfig, HtmlDom, PointerInput, PointerKind, Rgba, UserSelect, WheelMode,
};
use dioxus_html::InteractionElementOffset;

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
.row { height: 20px; background-color: rgb(0, 0, 255); }
.row:hover { background-color: rgb(255, 0, 0); }
.press { height: 20px; background-color: rgb(0, 0, 255); }
.press:active { background-color: rgb(0, 255, 0); }
.group { background-color: rgb(0, 0, 255); }
.group:hover { background-color: rgb(255, 0, 0); }
#sash { position: absolute; left: 100px; top: 100px; width: 4px; height: 50px;
        cursor: col-resize; }
#other { position: absolute; left: 200px; top: 100px; width: 50px; height: 50px; }
.nosel { user-select: none; }
.nosel .sel { -webkit-user-select: text; }
#ssid { user-select: all; }
.imp { user-select: none !important; }
/* a comment with { braces } */
@media (min-width: 9999px) { .never { user-select: none; } }
#cover { position: absolute; left: 300px; top: 0; width: 50px; height: 50px;
         pointer-events: none; }
";

fn page() -> Element {
    rsx! {
        div { id: "a", class: "row",
            onmouseenter: move |_| log("enter a"),
            onmouseleave: move |_| log("leave a"),
            onmousemove: move |e| {
                let at = e.element_coordinates();
                log(format!("move a {} {}", at.x, at.y));
            },
        }
        div { id: "b", class: "row" }
        div { id: "press", class: "press" }
        div { id: "group", class: "group",
            div { id: "inner", style: "height: 10px" }
        }
        div { id: "sash",
            onmousedown: move |_| log("down sash"),
            onmousemove: move |e| {
                let at = e.element_coordinates();
                log(format!("move sash {} {}", at.x, at.y));
            },
            onmouseup: move |_| log("up sash"),
        }
        div { id: "other",
            onmouseenter: move |_| log("enter other"),
            onmousemove: move |_| log("move other"),
        }
        div { id: "cover", onmousemove: move |_| log("move cover") }
        div { id: "dbl",
            style: "position: absolute; left: 0; top: 200px; width: 50px; height: 50px",
            ondoubleclick: move |_| log("dblclick"),
            oncontextmenu: move |e| {
                log("contextmenu");
                e.prevent_default();
            },
            onwheel: move |e| {
                let delta = e.delta().strip_units();
                log(format!("wheel {} {}", delta.x, delta.y));
                e.prevent_default();
            },
        }
        div { id: "tree", style: "cursor: pointer; user-select: none",
            div { id: "leaf" }
        }
        div { id: "plain" }
        div { id: "ss", class: "nosel",
            div { id: "ss_child" }
            div { id: "ss_text", class: "sel" }
            div { id: "ssid", class: "nosel" }
            div { id: "ssimp", class: "imp", style: "user-select: text" }
        }
        div { id: "never", class: "never" }
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

fn background(dom: &mut HtmlDom, id: &str) -> Option<Rgba> {
    dom.layout(400.0, 300.0, 1.0);
    let node = dom.element_by_id(id).expect("the element is in the page");
    dom.display_list().unwrap().get(node).unwrap().background
}

fn at(dom: &mut HtmlDom, id: &str, kind: PointerKind, x: f32, y: f32) -> bool {
    let node = dom.element_by_id(id).expect("the element is in the page");
    dom.pointer_event(node, &PointerInput::new(kind, x, y))
}

const BLUE: Option<Rgba> = Some(Rgba::new(0, 0, 255, 255));
const RED: Option<Rgba> = Some(Rgba::new(255, 0, 0, 255));
const GREEN: Option<Rgba> = Some(Rgba::new(0, 255, 0, 255));

#[test]
fn fr34_5_hover_matches_hover_rule() {
    let mut dom = dom();
    assert_eq!(background(&mut dom, "a"), BLUE);
    at(&mut dom, "a", PointerKind::Move, 5.0, 5.0);
    assert_eq!(background(&mut dom, "a"), RED);
    // The ancestors of the box under the pointer match too.
    at(&mut dom, "inner", PointerKind::Move, 5.0, 5.0);
    assert_eq!(background(&mut dom, "group"), RED);
    // Leaving the page clears the state.
    at(&mut dom, "a", PointerKind::Leave, 5.0, -50.0);
    assert_eq!(background(&mut dom, "a"), BLUE);
    assert_eq!(background(&mut dom, "group"), BLUE);
}

#[test]
fn fr34_5_hover_moves_with_pointer() {
    let mut dom = dom();
    at(&mut dom, "a", PointerKind::Enter, 5.0, 5.0);
    assert_eq!(background(&mut dom, "a"), RED);
    assert_eq!(background(&mut dom, "b"), BLUE);
    at(&mut dom, "b", PointerKind::Move, 5.0, 5.0);
    assert_eq!(background(&mut dom, "a"), BLUE);
    assert_eq!(background(&mut dom, "b"), RED);
    let b = dom.element_by_id("b").unwrap();
    assert!(dom.is_hovered(b));
    assert_eq!(dom.hovered(), Some(b));
}

#[test]
fn fr34_5_active_matches_active_rule() {
    let mut dom = dom();
    assert_eq!(background(&mut dom, "press"), BLUE);
    let node = dom.element_by_id("press").unwrap();
    dom.pointer_event(node, &PointerInput::press(5.0, 5.0, 1, 1));
    assert_eq!(background(&mut dom, "press"), GREEN);
    assert!(dom.is_active(node));
    dom.pointer_event(node, &PointerInput::release(5.0, 5.0, 1, 1));
    assert_eq!(background(&mut dom, "press"), BLUE);
    assert!(!dom.is_active(node));
}

#[test]
fn fr34_5_mouseenter_leave_reach_handlers() {
    let mut dom = dom();
    at(&mut dom, "a", PointerKind::Enter, 5.0, 5.0);
    at(&mut dom, "a", PointerKind::Move, 6.0, 7.0);
    at(&mut dom, "a", PointerKind::Leave, 6.0, 25.0);
    assert_eq!(take_log(), ["enter a", "move a 6 7", "leave a"]);
}

#[test]
fn fr34_5_dblclick_reaches_handler() {
    let mut dom = dom();
    let node = dom.element_by_id("dbl").unwrap();
    dom.pointer_event(node, &PointerInput::press(5.0, 5.0, 1, 1));
    dom.pointer_event(node, &PointerInput::release(5.0, 5.0, 1, 1));
    assert_eq!(take_log(), Vec::<String>::new());
    dom.pointer_event(node, &PointerInput::press(5.0, 5.0, 1, 2));
    dom.pointer_event(node, &PointerInput::release(5.0, 5.0, 1, 2));
    assert_eq!(take_log(), ["dblclick"]);
}

#[test]
fn fr34_5_contextmenu_targets_node() {
    let mut dom = dom();
    // The handler prevents the default, so the report says it was consumed.
    assert!(at(&mut dom, "dbl", PointerKind::ContextMenu, 5.0, 5.0));
    assert_eq!(take_log(), ["contextmenu"]);
    // A node without a handler for it leaves the report unconsumed.
    assert!(!at(&mut dom, "b", PointerKind::ContextMenu, 5.0, 5.0));
    assert_eq!(take_log(), Vec::<String>::new());
}

#[test]
fn fr34_5_wheel_reaches_handler() {
    let mut dom = dom();
    let node = dom.element_by_id("dbl").unwrap();
    let consumed = dom.pointer_event(node, &PointerInput::wheel(5.0, 5.0, 0.0, 30.0));
    assert!(consumed);
    assert_eq!(take_log(), ["wheel 0 30"]);
    let lines = PointerInput {
        delta_mode: WheelMode::Line,
        ..PointerInput::wheel(5.0, 5.0, 0.0, 3.0)
    };
    dom.pointer_event(node, &lines);
    assert_eq!(take_log(), ["wheel 0 3"]);
}

#[test]
fn fr34_5_pointer_capture_sash_drag() {
    let mut dom = dom();
    let sash = dom.element_by_id("sash").unwrap();
    let other = dom.element_by_id("other").unwrap();
    dom.pointer_event(sash, &PointerInput::press(2.0, 10.0, 1, 1));
    // Far outside the 4 pixel sash, over the other box.
    let moved = PointerInput {
        buttons: 1,
        ..PointerInput::new(PointerKind::Move, 120.0, 10.0)
    };
    dom.pointer_event(sash, &moved);
    // The renderer sends nothing to the other box meanwhile, and a report that did arrive
    // is not for it.
    dom.pointer_event(other, &PointerInput::new(PointerKind::Enter, 10.0, 10.0));
    dom.pointer_event(other, &PointerInput::new(PointerKind::Move, 10.0, 10.0));
    dom.pointer_event(sash, &PointerInput::release(120.0, 10.0, 1, 1));
    assert_eq!(
        take_log(),
        ["down sash", "move sash 120 10", "up sash"],
        "the drag belongs to the sash from the press to the release"
    );
    // After the release the pointer is an ordinary one again.
    dom.pointer_event(other, &PointerInput::new(PointerKind::Enter, 10.0, 10.0));
    assert_eq!(take_log(), ["enter other"]);
    // And the sash takes a move only where it is.
    dom.pointer_event(sash, &moved);
    assert_eq!(take_log(), Vec::<String>::new());
}

#[test]
fn fr34_5_pointer_events_none_skips_hit() {
    let mut dom = dom();
    // The cover has `pointer-events: none`, so a pointer over it is over the page behind.
    at(&mut dom, "cover", PointerKind::Move, 5.0, 5.0);
    assert_eq!(take_log(), Vec::<String>::new());
    assert_eq!(dom.hovered(), None);
}

#[test]
fn fr34_5_cursor_reported_for_hit_node() {
    let dom = dom();
    let sash = dom.element_by_id("sash").unwrap();
    assert_eq!(dom.cursor(sash), Cursor::ColResize);
    let tree = dom.element_by_id("tree").unwrap();
    let leaf = dom.element_by_id("leaf").unwrap();
    assert_eq!(dom.cursor(tree), Cursor::Pointer);
    // The cursor inherits.
    assert_eq!(dom.cursor(leaf), Cursor::Pointer);
    let plain = dom.element_by_id("plain").unwrap();
    assert_eq!(dom.cursor(plain), Cursor::Auto);
}

#[test]
fn fr34_5_user_select_none_blocks_selection() {
    let dom = dom();
    let tree = dom.element_by_id("tree").unwrap();
    let leaf = dom.element_by_id("leaf").unwrap();
    let plain = dom.element_by_id("plain").unwrap();
    assert_eq!(dom.user_select(tree), UserSelect::None);
    assert_eq!(dom.user_select(leaf), UserSelect::None);
    assert_eq!(dom.user_select(plain), UserSelect::Auto);
}

#[test]
fn fr34_5_subscriptions_follow_listeners_and_hover_rules() {
    let dom = dom();
    let subscriptions = dom.pointer_subscriptions();
    let kinds = |id: &str| {
        let node = dom.element_by_id(id).unwrap();
        subscriptions
            .iter()
            .find(|subscription| subscription.node == node)
            .map(|subscription| subscription.kinds)
    };
    // Enter, Leave and Move for `a`, and nothing else.
    assert_eq!(
        kinds("a"),
        Some(PointerKind::Enter.bit() | PointerKind::Leave.bit() | PointerKind::Move.bit())
    );
    assert_eq!(
        kinds("sash"),
        Some(PointerKind::Down.bit() | PointerKind::Move.bit() | PointerKind::Up.bit())
    );
    assert_eq!(
        kinds("dbl"),
        Some(PointerKind::Up.bit() | PointerKind::ContextMenu.bit() | PointerKind::Wheel.bit())
    );
    // A node with no listener is not subscribed.
    assert_eq!(kinds("plain"), None);
    // The sheet has `:hover` and `:active` rules, so the root tracks them for the page.
    let root = subscriptions.iter().find(|subscription| {
        subscription.kinds & PointerKind::Down.bit() != 0
            && subscription.kinds & PointerKind::Cancel.bit() != 0
    });
    assert!(root.is_some());
}

#[test]
fn fr34_5_no_hover_rules_means_no_root_subscription() {
    fn bare() -> Element {
        rsx! { div { id: "x" } }
    }
    let dom = HtmlDom::new(bare);
    assert!(dom.pointer_subscriptions().is_empty());
}

#[test]
fn nfr7_unknown_pointer_kind_is_none() {
    assert_eq!(PointerKind::from_wire(0), None);
    assert_eq!(PointerKind::from_wire(9), None);
    assert_eq!(PointerKind::from_wire(4), Some(PointerKind::Down));
}

#[test]
fn fr34_5_user_select_from_stylesheet_rules() {
    let dom = dom();
    let id = |name: &str| dom.element_by_id(name).unwrap();
    // A rule on the parent reaches the child by inheritance.
    assert_eq!(dom.user_select(id("ss")), UserSelect::None);
    assert_eq!(dom.user_select(id("ss_child")), UserSelect::None);
    // A more specific selector beats the rule before it, and the prefixed name counts.
    assert_eq!(dom.user_select(id("ss_text")), UserSelect::Text);
    // An id selector beats a class selector whatever the order.
    assert_eq!(dom.user_select(id("ssid")), UserSelect::All);
    // An important rule beats the element's own style attribute.
    assert_eq!(dom.user_select(id("ssimp")), UserSelect::None);
    // A rule that no node matches, or that sits in a media block, changes nothing.
    assert_eq!(dom.user_select(id("never")), UserSelect::Auto);
}

#[test]
fn fr34_5_hover_change_asks_for_a_frame() {
    let mut dom = dom();
    assert!(!dom.take_pointer_restyle());
    at(&mut dom, "a", PointerKind::Move, 5.0, 5.0);
    assert!(dom.take_pointer_restyle());
    assert!(!dom.take_pointer_restyle());
    // A move inside the same box changes nothing, so asks for nothing.
    at(&mut dom, "a", PointerKind::Move, 6.0, 5.0);
    assert!(!dom.take_pointer_restyle());
    let node = dom.element_by_id("press").unwrap();
    dom.pointer_event(node, &PointerInput::press(5.0, 5.0, 1, 1));
    assert!(dom.take_pointer_restyle());
}
