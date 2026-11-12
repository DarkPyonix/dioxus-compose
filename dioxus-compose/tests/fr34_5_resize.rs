//! `onresize` on HTML elements: sent after a layout pass when an element's content box
//! size changed, once on its first layout, and never for a size it was already told.

use std::cell::RefCell;

use dioxus_compose::html::HtmlDom;
use dioxus_compose::html::prelude::*;
use dioxus_elements::ResizeEvent;
use dioxus_hooks::use_signal;
use dioxus_signals::WritableExt;

/// A resize event's content box and border box sizes.
type Sizes = ((f64, f64), (f64, f64));

thread_local! {
    /// Each resize event's content and border box sizes, in order.
    static SEEN: RefCell<Vec<Sizes>> = const { RefCell::new(Vec::new()) };
}

fn take() -> Vec<Sizes> {
    SEEN.with(|seen| std::mem::take(&mut *seen.borrow_mut()))
}

fn record(evt: ResizeEvent) {
    let content = evt.data().get_content_box_size().unwrap();
    let border = evt.data().get_border_box_size().unwrap();
    SEEN.with(|seen| {
        seen.borrow_mut().push((
            (content.width, content.height),
            (border.width, border.height),
        ))
    });
}

fn growing() -> Element {
    let mut wide = use_signal(|| false);
    let width = if wide() { 200 } else { 100 };
    rsx! {
        div {
            id: "grow",
            style: "width: {width}px; height: 40px; padding: 5px; border: 2px solid black",
            onresize: record,
            onclick: move |_| wide.set(true),
        }
        div { id: "other", style: "width: 10px; height: 10px", onclick: move |_| {} }
    }
}

#[test]
fn fr34_5_resize_fires_after_layout_when_content_box_changes() {
    take();
    let mut dom = HtmlDom::new(growing);
    dom.layout(800.0, 600.0, 1.0);
    assert_eq!(
        take(),
        vec![((100.0, 40.0), (114.0, 54.0))],
        "the first layout reports the content box once"
    );
    let grow = dom.element_by_id("grow").unwrap();
    dom.click_node(grow);
    dom.render();
    assert!(take().is_empty(), "nothing is sent before the layout pass");
    dom.layout(800.0, 600.0, 1.0);
    assert_eq!(take(), vec![((200.0, 40.0), (214.0, 54.0))]);
    dom.layout(800.0, 600.0, 1.0);
    assert!(take().is_empty(), "no further change, no further event");
}

fn fluid() -> Element {
    rsx! {
        div { id: "fluid", style: "height: 20px; margin: 0", onresize: record }
    }
}

#[test]
fn fr34_5_resize_fires_on_window_resize_alone() {
    take();
    let mut dom = HtmlDom::with_config(
        fluid,
        dioxus_compose::html::HtmlConfig {
            stylesheets: vec!["body { margin: 0 }".into()],
            ..Default::default()
        },
    );
    dom.layout(800.0, 600.0, 2.0);
    assert_eq!(take(), vec![((800.0, 20.0), (800.0, 20.0))]);
    dom.layout(500.0, 600.0, 2.0);
    assert_eq!(
        take(),
        vec![((500.0, 20.0), (500.0, 20.0))],
        "the window alone changed, in CSS pixels"
    );
    dom.layout(500.0, 300.0, 2.0);
    assert!(
        take().is_empty(),
        "a height the element ignores changes nothing"
    );
}

#[test]
fn fr34_5_resize_not_sent_when_size_is_unchanged() {
    take();
    let mut dom = HtmlDom::new(growing);
    dom.layout(800.0, 600.0, 1.0);
    assert_eq!(take().len(), 1);
    // A layout pass with another element's change, and one that skips layout entirely.
    let other = dom.element_by_id("other").unwrap();
    dom.click_node(other);
    dom.render();
    dom.layout(800.0, 600.0, 1.0);
    dom.layout(800.0, 600.0, 1.0);
    dom.set_zoom(1.5);
    dom.layout(800.0, 600.0, 1.0);
    assert!(take().is_empty());
}
