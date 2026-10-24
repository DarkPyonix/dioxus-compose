//! Focus-state selectors, `color-mix()` and `:has()` in the HTML and CSS path.
//!
//! The pinned blitz-dom (0.2.4) and stylo (0.8.0) decide what can hold here:
//! - `:focus` follows the focus state `HtmlDom::focus` and `blur` set on the node.
//! - blitz-dom answers `false` for `:focus-within` and `:focus-visible` whatever the focus
//!   state is, so those two are pinned as not matching rather than emulated.
//! - stylo's servo selector parser has `:has()` switched off, so a rule using it is dropped.
//! - `color-mix()` is parsed by stylo and resolved when the colour is converted.

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{HtmlConfig, HtmlDom, Rgba};

const SHEET: &str = "
html, body { margin: 0; padding: 0; }
.row { height: 20px; background-color: rgb(0, 0, 255); }
.row:focus { background-color: rgb(255, 0, 0); }
.group { height: 20px; background-color: rgb(0, 0, 255); }
.group:focus-within { background-color: rgb(255, 0, 0); }
.visible { height: 20px; background-color: rgb(0, 0, 255); }
.visible:focus-visible { background-color: rgb(255, 0, 0); }
.mixed { height: 20px; background-color: color-mix(in srgb, red 50%, blue); }
.holder { height: 20px; background-color: rgb(0, 0, 255); }
.holder:has(> .x) { background-color: rgb(255, 0, 0); }
";

fn page() -> Element {
    rsx! {
        div { id: "row", class: "row", tabindex: "0" }
        div { id: "group", class: "group",
            button { id: "tab", tabindex: "0" }
        }
        div { id: "visible", class: "visible", tabindex: "0" }
        div { id: "mixed", class: "mixed" }
        div { id: "holder", class: "holder",
            div { id: "child", class: "x" }
        }
    }
}

fn dom() -> HtmlDom {
    HtmlDom::with_config(
        page,
        HtmlConfig {
            stylesheets: vec![SHEET.to_string()],
            ..HtmlConfig::default()
        },
    )
}

fn background(dom: &mut HtmlDom, id: &str) -> Option<Rgba> {
    dom.layout(400.0, 300.0, 1.0);
    let node = dom.element_by_id(id).expect("the element is in the page");
    dom.display_list().unwrap().get(node).unwrap().background
}

const BLUE: Option<Rgba> = Some(Rgba::new(0, 0, 255, 255));
const RED: Option<Rgba> = Some(Rgba::new(255, 0, 0, 255));

#[test]
fn fr34_focus_matches_the_focused_node_and_blur_reverts() {
    let mut dom = dom();
    assert_eq!(background(&mut dom, "row"), BLUE);
    let row = dom.element_by_id("row").unwrap();
    dom.focus(row);
    assert_eq!(background(&mut dom, "row"), RED);
    dom.blur(row);
    assert_eq!(background(&mut dom, "row"), BLUE);
}

#[test]
fn fr34_focus_within_matches_tab_group() {
    // Pinned: blitz-dom 0.2.4 hardcodes `:focus-within` to false, so focusing a
    // descendant does not restyle the ancestor. When blitz-dom implements it, this
    // test must flip to RED while focused and BLUE after the blur.
    let mut dom = dom();
    let tab = dom.element_by_id("tab").unwrap();
    dom.focus(tab);
    assert_eq!(background(&mut dom, "group"), BLUE);
    dom.blur(tab);
    assert_eq!(background(&mut dom, "group"), BLUE);
}

#[test]
fn fr34_focus_visible_matches_keyboard_focus() {
    // Pinned: blitz-dom 0.2.4 hardcodes `:focus-visible` to false, and `HtmlDom::focus`
    // does not say whether the focus came from the keyboard or the pointer, so the
    // two cannot be told apart here either way.
    let mut dom = dom();
    let node = dom.element_by_id("visible").unwrap();
    dom.focus(node);
    assert_eq!(background(&mut dom, "visible"), BLUE);
}

#[test]
fn fr34_color_mix_resolves() {
    let mut dom = dom();
    let colour = background(&mut dom, "mixed").expect("color-mix() gives a background");
    // Half red, half blue in sRGB is (127.5, 0, 127.5); either rounding is right.
    assert!((127..=128).contains(&colour.r), "{colour:?}");
    assert_eq!(colour.g, 0, "{colour:?}");
    assert!((127..=128).contains(&colour.b), "{colour:?}");
    assert_eq!(colour.a, 255);
}

#[test]
fn fr34_has_selector_is_not_parsed_by_pinned_stylo() {
    // Pinned: stylo 0.8.0's servo selector parser returns false from `parse_has`, so the
    // `:has(> .x)` rule is dropped and the parent keeps its base background even though
    // it has a matching child.
    let mut dom = dom();
    assert_eq!(background(&mut dom, "holder"), BLUE);
}
