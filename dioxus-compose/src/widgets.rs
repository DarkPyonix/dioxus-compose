#![allow(non_snake_case)]

use dioxus_core::{Element, EventHandler};
use dioxus_core_macro::{Props, component, rsx};

use crate as dioxus_elements;

#[component]
pub fn Column(
    #[props(default)] fill_max_width: bool,
    #[props(default)] fill_max_height: bool,
    children: Element,
) -> Element {
    rsx! {
        column { fill_max_width, fill_max_height, {children} }
    }
}

#[component]
pub fn Row(
    #[props(default)] fill_max_width: bool,
    #[props(default)] fill_max_height: bool,
    children: Element,
) -> Element {
    rsx! {
        row { fill_max_width, fill_max_height, {children} }
    }
}

#[component]
pub fn ComposeBox(
    #[props(default)] fill_max_width: bool,
    #[props(default)] fill_max_height: bool,
    children: Element,
) -> Element {
    rsx! {
        composebox { fill_max_width, fill_max_height, {children} }
    }
}

#[component]
pub fn Text(#[props(into)] text: String) -> Element {
    rsx! { text { text } }
}

#[component]
pub fn TextField(
    #[props(into, default)] placeholder: String,
    #[props(default = true)] enabled: bool,
    #[props(default)] multiline: bool,
    #[props(default)] on_value_change: EventHandler<String>,
    #[props(default)] on_submit: EventHandler<String>,
    #[props(default)] on_focus_lost: EventHandler<()>,
) -> Element {
    rsx! {
        textfield {
            placeholder,
            enabled,
            multiline,
            onvaluechange: move |event| on_value_change.call((*event.data()).clone()),
            onsubmit: move |event| on_submit.call((*event.data()).clone()),
            onfocuslost: move |_| on_focus_lost.call(()),
        }
    }
}

#[component]
pub fn Button(
    #[props(into)] text: String,
    #[props(default = true)] enabled: bool,
    #[props(default)] on_click: EventHandler<()>,
) -> Element {
    rsx! {
        button { text, enabled, onclick: move |_| on_click.call(()) }
    }
}

#[component]
pub fn Spacer(#[props(default)] width: f32, #[props(default)] height: f32) -> Element {
    rsx! { spacer { width, height } }
}
