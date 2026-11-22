//! `position: sticky` clamping against the nearest scroll container and containing block.

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{HtmlConfig, HtmlDom, ScrollPosition, element_by_id};

fn scroll_pos(x: f64, y: f64, width: f64, height: f64, cw: f64, ch: f64) -> ScrollPosition {
    ScrollPosition {
        x,
        y,
        scroll_width: width,
        scroll_height: height,
        client_width: cw,
        client_height: ch,
    }
}

#[test]
fn fr34_5_sticky_pins_at_scroll_edge() {
    const SHEET: &str = "
        html, body { margin: 0; padding: 0; }
        #scroller { width: 200px; height: 100px; overflow: auto; }
        #before { width: 200px; height: 40px; }
        #sticky { position: sticky; top: 10px; width: 200px; height: 30px; }
        #after { width: 200px; height: 300px; }
    ";

    fn app() -> Element {
        rsx! {
            div { id: "scroller",
                div { id: "before" }
                div { id: "sticky" }
                div { id: "after" }
            }
        }
    }

    let mut dom = HtmlDom::with_config(
        app,
        HtmlConfig {
            stylesheets: vec![SHEET.to_string()],
            ..HtmlConfig::default()
        },
    );

    let _ = dom.layout(400.0, 300.0, 1.0);
    let scroller = element_by_id(dom.document(), "scroller").expect("#scroller");
    let sticky = element_by_id(dom.document(), "sticky").expect("#sticky");

    // At scroll 0, the sticky box sits at its normal-flow position (y = 40), not shifted by
    // `top: 10px` as `position: relative` would be.
    let rect = dom
        .layout(400.0, 300.0, 1.0)
        .get(sticky)
        .expect("sticky entry")
        .rect;
    assert_eq!(rect.y, 40.0, "normal flow before reaching top edge: {rect:?}");
    assert_eq!(dom.hit_test(50.0, 50.0), Some(sticky));

    // Scrolling 20px keeps the box at y = 40 (screen y = 20, still below top: 10px).
    dom.scroll_changed(scroller, scroll_pos(0.0, 20.0, 200.0, 370.0, 200.0, 100.0));
    let rect = dom
        .layout(400.0, 300.0, 1.0)
        .get(sticky)
        .expect("sticky entry")
        .rect;
    assert_eq!(rect.y, 40.0, "still in flow at scroll 20: {rect:?}");

    // Scrolling 80px pins the sticky box at screen y = 10 (content y = 80 + 10 = 90).
    dom.scroll_changed(scroller, scroll_pos(0.0, 80.0, 200.0, 370.0, 200.0, 100.0));
    let rect = dom
        .layout(400.0, 300.0, 1.0)
        .get(sticky)
        .expect("sticky entry")
        .rect;
    assert_eq!(rect.y, 90.0, "pinned at top: 10px edge when scrolled 80px: {rect:?}");
    // Hit testing at screen y = 20 (inside [10, 40]) lands on the pinned sticky box.
    assert_eq!(dom.hit_test(50.0, 20.0), Some(sticky));
}

#[test]
fn fr34_5_sticky_stays_in_containing_block() {
    const SHEET: &str = "
        html, body { margin: 0; padding: 0; }
        #scroller { width: 200px; height: 100px; overflow: auto; }
        #section { width: 200px; height: 160px; }
        #sticky { position: sticky; top: 0px; width: 200px; height: 30px; }
        #tail { width: 200px; height: 300px; }
    ";

    fn app() -> Element {
        rsx! {
            div { id: "scroller",
                div { id: "section",
                    div { id: "sticky" }
                }
                div { id: "tail" }
            }
        }
    }

    let mut dom = HtmlDom::with_config(
        app,
        HtmlConfig {
            stylesheets: vec![SHEET.to_string()],
            ..HtmlConfig::default()
        },
    );

    let _ = dom.layout(400.0, 300.0, 1.0);
    let scroller = element_by_id(dom.document(), "scroller").expect("#scroller");
    let sticky = element_by_id(dom.document(), "sticky").expect("#sticky");
    let tail = element_by_id(dom.document(), "tail").expect("#tail");
    let rect = dom
        .layout(400.0, 300.0, 1.0)
        .get(sticky)
        .expect("sticky entry")
        .rect;
    assert_eq!(rect.y, 0.0);

    // Scrolled 60px: pinned at viewport top (content y = 60, bottom = 90 <= section bottom 160).
    dom.scroll_changed(scroller, scroll_pos(0.0, 60.0, 200.0, 460.0, 200.0, 100.0));
    let rect = dom
        .layout(400.0, 300.0, 1.0)
        .get(sticky)
        .expect("sticky entry")
        .rect;
    assert_eq!(rect.y, 60.0, "pinned inside section at scroll 60: {rect:?}");
    assert_eq!(dom.hit_test(50.0, 15.0), Some(sticky));

    // Scrolled 150px: section ends at 160, so sticky (height 30) stops at content y = 130
    // (screen y = -20..10), staying inside #section rather than overflowing into #tail.
    dom.scroll_changed(scroller, scroll_pos(0.0, 150.0, 200.0, 460.0, 200.0, 100.0));
    let rect = dom
        .layout(400.0, 300.0, 1.0)
        .get(sticky)
        .expect("sticky entry")
        .rect;
    assert_eq!(rect.y, 130.0, "clamped to containing block bottom (160 - 30): {rect:?}");
    // Screen y = 5 is still within sticky's visible bottom 10px; screen y = 20 is in #tail.
    assert_eq!(dom.hit_test(50.0, 5.0), Some(sticky));
    assert_eq!(dom.hit_test(50.0, 20.0), Some(tail));
}
