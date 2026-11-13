//! The user-agent `user-select: none` of a control's label on the wire. A browser never
//! lets the label of a `<button>`, a `<select>` or a button-like `<input>` be selected,
//! and the renderer selects text inside an HTML region by default, so the Host sends
//! `UserSelect::None` on those nodes unless the page sets `user-select` on them itself.

use dioxus_compose::Host;
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::PropertyKind;

/// The `UserSelect` values the batch writes, in order.
fn user_selects(bytes: &[u8]) -> Vec<i64> {
    decode_batch(bytes)
        .expect("the batch decodes")
        .into_iter()
        .filter_map(|record| match record {
            Mutation::SetProp {
                property: PropertyKind::UserSelect,
                value: PropertyValue::Integer(value),
                ..
            } => Some(value),
            _ => None,
        })
        .collect()
}

thread_local! {
    static SHEET: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![SHEET.with(|sheet| sheet.get()).to_string()],
        ..HtmlConfig::default()
    }
}

fn button_page() -> Element {
    rsx! { button { "zsh" } }
}

fn select_page() -> Element {
    rsx! { select { option { "one" } option { "two" } } }
}

fn submit_page() -> Element {
    rsx! { input { r#type: "submit", value: "Send" } }
}

fn plain_page() -> Element {
    rsx! { div { "words" } }
}

fn first(page: fn() -> Element, css: &'static str) -> Vec<i64> {
    SHEET.with(|sheet| sheet.set(css));
    let mut host = Host::html(page, config);
    user_selects(host.rebuild().unwrap())
}

#[test]
fn fr34_5_a_button_label_is_not_selectable() {
    let sent = first(button_page, "");
    assert!(!sent.is_empty() && sent.iter().all(|v| *v == 1), "{sent:?}");
    // Text outside a control stays selectable: nothing is written for it.
    assert_eq!(first(plain_page, ""), Vec::<i64>::new());
}

#[test]
fn fr34_5_a_select_and_its_options_are_not_selectable() {
    let sent = first(select_page, "");
    assert!(!sent.is_empty() && sent.iter().all(|v| *v == 1), "{sent:?}");
}

#[test]
fn fr34_5_a_submit_input_is_not_selectable() {
    let sent = first(submit_page, "");
    assert!(!sent.is_empty() && sent.iter().all(|v| *v == 1), "{sent:?}");
}

#[test]
fn fr34_5_author_user_select_overrides_the_button_default() {
    let sent = first(button_page, "button { user-select: text; }");
    assert!(!sent.contains(&1), "{sent:?}");
    assert!(sent.contains(&2), "{sent:?}");
    // `auto` is the page's word too, and leaves the label selectable.
    assert!(!first(button_page, "button { user-select: auto; }").contains(&1));
    // An ancestor's `user-select: text` does not reach past the button's own value.
    let sent = first(button_page, "body { user-select: text; }");
    assert!(sent.contains(&1), "{sent:?}");
}
