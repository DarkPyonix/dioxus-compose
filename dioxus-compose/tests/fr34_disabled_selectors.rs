//! `:disabled` and `:enabled` in the HTML and CSS path, matched the way Chromium matches
//! them.
//!
//! blitz-dom 0.2.4 answers `false` for both whatever the document holds. The patch under
//! scripts/patches/blitz-dom-0.2.4/ gives every form control the state HTML defines:
//! - `button`, `input`, `select`, `textarea` and `fieldset` are disabled by their own
//!   `disabled` attribute, or by being inside a `fieldset` that has it and not inside that
//!   fieldset's first `legend` child;
//! - `optgroup` is disabled by its own attribute, `option` by its own or its parent
//!   `optgroup`'s;
//! - each of those that is not disabled is enabled, and any other element is neither.
//!
//! These tests need the `patched-blitz` feature, which the workspace's checks turn on.

#![cfg(feature = "patched-blitz")]

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{HtmlConfig, HtmlDom, Rgba};
use dioxus_hooks::use_signal;
use dioxus_signals::{ReadableExt, WritableExt};

/// Red for `:disabled`, green for `:enabled`, blue for an element that is neither.
const SHEET: &str = "
html, body { margin: 0; padding: 0; }
button, input, select, textarea, fieldset, optgroup, option, legend, div {
    display: block;
    width: 40px;
    height: 10px;
    margin: 0;
    padding: 0;
    border: 0;
    background-color: rgb(0, 0, 255);
}
fieldset, optgroup, legend, .holder { height: auto; min-height: 10px; }
:enabled { background-color: rgb(0, 255, 0); }
:disabled { background-color: rgb(255, 0, 0); }
";

const RED: Option<Rgba> = Some(Rgba::new(255, 0, 0, 255));
const GREEN: Option<Rgba> = Some(Rgba::new(0, 255, 0, 255));
const BLUE: Option<Rgba> = Some(Rgba::new(0, 0, 255, 255));

fn dom(app: fn() -> Element) -> HtmlDom {
    HtmlDom::with_config(
        app,
        HtmlConfig {
            stylesheets: vec![SHEET.to_string()],
            ..HtmlConfig::default()
        },
    )
}

#[track_caller]
fn background(dom: &mut HtmlDom, id: &str) -> Option<Rgba> {
    dom.layout(400.0, 600.0, 1.0);
    let node = dom
        .element_by_id(id)
        .unwrap_or_else(|| panic!("no element with id {id}"));
    dom.display_list()
        .unwrap()
        .get(node)
        .unwrap_or_else(|| panic!("#{id} has no display list entry"))
        .background
}

#[track_caller]
fn assert_backgrounds(dom: &mut HtmlDom, expected: &[(&str, Option<Rgba>)]) {
    for &(id, colour) in expected {
        assert_eq!(background(dom, id), colour, "#{id}");
    }
}

fn controls() -> Element {
    rsx! {
        button { id: "button-on" }
        button { id: "button-off", disabled: true }
        input { id: "input-on" }
        input { id: "input-off", disabled: true }
        textarea { id: "textarea-on" }
        textarea { id: "textarea-off", disabled: true }
        select { id: "select-on" }
        select { id: "select-off", disabled: true }
        fieldset { id: "fieldset-on" }
        div { id: "plain", class: "holder", disabled: true }
        div { class: "holder",
            optgroup { id: "group-on",
                option { id: "option-on" }
                option { id: "option-off", disabled: true }
            }
            optgroup { id: "group-off", disabled: true,
                option { id: "option-in-disabled-group" }
            }
        }
    }
}

#[test]
fn fr34_disabled_matches_form_controls_with_the_disabled_attribute() {
    let mut dom = dom(controls);
    assert_backgrounds(
        &mut dom,
        &[
            ("button-on", GREEN),
            ("button-off", RED),
            ("input-on", GREEN),
            ("input-off", RED),
            ("textarea-on", GREEN),
            ("textarea-off", RED),
            ("select-on", GREEN),
            ("select-off", RED),
            ("fieldset-on", GREEN),
        ],
    );
}

#[test]
fn fr34_disabled_and_enabled_skip_elements_that_are_not_form_controls() {
    let mut dom = dom(controls);
    // A `div` with a `disabled` attribute is neither disabled nor enabled.
    assert_eq!(background(&mut dom, "plain"), BLUE);
}

#[test]
fn fr34_disabled_optgroup_disables_its_options() {
    let mut dom = dom(controls);
    assert_backgrounds(
        &mut dom,
        &[
            ("group-on", GREEN),
            ("option-on", GREEN),
            ("option-off", RED),
            ("group-off", RED),
            ("option-in-disabled-group", RED),
        ],
    );
}

fn fieldsets() -> Element {
    rsx! {
        fieldset { id: "outer", disabled: true,
            legend { id: "first-legend",
                button { id: "in-first-legend" }
            }
            legend { id: "second-legend",
                button { id: "in-second-legend" }
            }
            div { class: "holder",
                input { id: "deep-input" }
            }
            fieldset { id: "inner",
                legend {
                    button { id: "in-inner-legend" }
                }
                textarea { id: "in-inner" }
            }
        }
        fieldset { id: "open",
            legend {
                button { id: "open-legend-button" }
            }
            fieldset { id: "closed", disabled: true,
                legend {
                    button { id: "closed-legend-button" }
                }
                button { id: "closed-button" }
            }
        }
    }
}

/// Everything inside a disabled fieldset is disabled except what is inside its first
/// `legend` child; a second legend does not count, and neither does the first legend of a
/// fieldset nested inside the disabled one.
#[test]
fn fr34_disabled_fieldset_disables_its_descendants_outside_the_first_legend() {
    let mut dom = dom(fieldsets);
    assert_backgrounds(
        &mut dom,
        &[
            ("outer", RED),
            ("first-legend", BLUE),
            ("in-first-legend", GREEN),
            ("second-legend", BLUE),
            ("in-second-legend", RED),
            ("deep-input", RED),
            ("inner", RED),
            ("in-inner-legend", RED),
            ("in-inner", RED),
            ("open", GREEN),
            ("open-legend-button", GREEN),
            ("closed", RED),
            ("closed-legend-button", GREEN),
            ("closed-button", RED),
        ],
    );
}

fn toggled() -> Element {
    let mut off = use_signal(|| false);
    let disabled = *off.read();
    rsx! {
        div {
            id: "toggle",
            onclick: move |_| {
                let now = *off.peek();
                off.set(!now);
            },
        }
        div { class: "holder",
            button { id: "first", disabled: disabled }
            button { id: "second" }
        }
        fieldset { id: "set", disabled: disabled,
            button { id: "in-set" }
        }
    }
}

/// Setting and removing `disabled` restyles the element and, for a fieldset, what is
/// inside it. Two sibling buttons that differ only in it are styled apart, which stylo's
/// style sharing would not do if the state were not on the element.
#[test]
fn fr34_disabled_follows_changes_of_the_disabled_attribute() {
    let mut dom = dom(toggled);
    assert_backgrounds(
        &mut dom,
        &[
            ("first", GREEN),
            ("second", GREEN),
            ("set", GREEN),
            ("in-set", GREEN),
        ],
    );

    let toggle = dom.element_by_id("toggle").unwrap();
    dom.click_node(toggle);
    dom.render();
    assert_backgrounds(
        &mut dom,
        &[
            ("first", RED),
            ("second", GREEN),
            ("set", RED),
            ("in-set", RED),
        ],
    );

    dom.click_node(toggle);
    dom.render();
    assert_backgrounds(
        &mut dom,
        &[
            ("first", GREEN),
            ("second", GREEN),
            ("set", GREEN),
            ("in-set", GREEN),
        ],
    );
}
