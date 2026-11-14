//! A comment makes no box. An empty `rsx!` branch leaves a placeholder comment in the
//! document, and a browser never lays a comment out, so in a grid or a flex container it
//! takes no track, no item slot and no gap.

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{HtmlDom, Rect};

const CSS: &str = "
    html, body { margin: 0; height: 100%; }
    .page { display: grid; height: 100%; row-gap: 8px;
            grid-template-rows: 52px 1fr 260px 32px;
            grid-template-areas: 'top' 'main' 'panel' 'status'; }
    .row { display: flex; column-gap: 10px; }
    .row > div { width: 20px; height: 20px; }
";

fn page() -> Element {
    let open = false;
    rsx! {
        style { "{CSS}" }
        div { class: "page",
            div { id: "top", style: "grid-area: top;" }
            div { id: "main", style: "grid-area: main;" }
            div { id: "panel", style: "grid-area: panel;" }
            div { id: "status", style: "grid-area: status;" }
            if open { div { "a" } }
            if open { div { "b" } }
            if open { div { "c" } }
        }
        div { class: "row", id: "row",
            div { id: "first" }
            if open { div {} }
            if open { div {} }
            div { id: "second" }
        }
    }
}

fn rect_of(dom: &mut HtmlDom, id: &str) -> Rect {
    let node = dom.element_by_id(id).unwrap_or_else(|| panic!("no #{id}"));
    dom.display_list()
        .and_then(|list| list.get(node))
        .unwrap_or_else(|| panic!("#{id} has no box"))
        .rect
}

/// The grid's named rows fill the page exactly: the placeholders after them add no
/// implicit rows and no gaps, so the 1fr row is 900 - 52 - 260 - 32 - 3 * 8 = 532 tall and
/// the status row ends at the page bottom.
#[test]
fn fr34_comment_children_take_no_grid_track_or_gap() {
    let mut dom = HtmlDom::new(page);
    dom.layout(1440.0, 900.0, 1.0);
    let main = rect_of(&mut dom, "main");
    assert_eq!((main.y, main.height), (60.0, 532.0));
    let status = rect_of(&mut dom, "status");
    assert_eq!(status.y + status.height, 900.0);
}

/// Two flex items with placeholders between them sit one gap apart, not three.
#[test]
fn fr34_comment_children_take_no_flex_gap() {
    let mut dom = HtmlDom::new(page);
    dom.layout(1440.0, 900.0, 1.0);
    let first = rect_of(&mut dom, "first");
    let second = rect_of(&mut dom, "second");
    assert_eq!(second.x - (first.x + first.width), 10.0);
}
