#![allow(non_snake_case)]

use dioxus_core::{Callback, Element, EventHandler};
use dioxus_core_macro::{Props, component, rsx};
use dioxus_hooks::use_signal;
use dioxus_signals::WritableExt as _;

use crate as dioxus_elements;
use crate::Key;
use std::cell::Cell;
use std::rc::Rc;

/// A key-down event whose consumption state is shared with the Host boundary.
#[derive(Clone, Debug)]
pub struct KeyEvent {
    key: Key,
    shift_key: bool,
    ctrl_key: bool,
    alt_key: bool,
    meta_key: bool,
    consumed: Rc<Cell<bool>>,
}

impl KeyEvent {
    pub(crate) fn new(
        key: Key,
        shift_key: bool,
        ctrl_key: bool,
        alt_key: bool,
        meta_key: bool,
    ) -> Self {
        Self {
            key,
            shift_key,
            ctrl_key,
            alt_key,
            meta_key,
            consumed: Rc::new(Cell::new(false)),
        }
    }

    pub fn key(&self) -> Key {
        self.key
    }

    pub fn shift_key(&self) -> bool {
        self.shift_key
    }

    pub fn ctrl_key(&self) -> bool {
        self.ctrl_key
    }

    pub fn alt_key(&self) -> bool {
        self.alt_key
    }

    pub fn meta_key(&self) -> bool {
        self.meta_key
    }

    pub fn consume(&self) {
        self.consumed.set(true);
    }

    pub fn consumed(&self) -> bool {
        self.consumed.get()
    }
}

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
    #[props(default)] on_key_down: EventHandler<KeyEvent>,
) -> Element {
    rsx! {
        textfield {
            placeholder,
            enabled,
            multiline,
            onvaluechange: move |event| on_value_change.call((*event.data()).clone()),
            onsubmit: move |event| on_submit.call((*event.data()).clone()),
            onfocuslost: move |_| on_focus_lost.call(()),
            onkeydown: move |event| on_key_down.call((*event.data()).clone()),
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

/// FR-8: the visible item range the Renderer asks the Host to materialise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RangeRequest {
    start: u32,
    count: u32,
}

impl RangeRequest {
    pub(crate) fn new(start: u32, count: u32) -> Self {
        Self { start, count }
    }

    pub fn start(&self) -> usize {
        self.start as usize
    }

    pub fn count(&self) -> usize {
        self.count as usize
    }
}

/// A windowed list (FR-8). The Host declares `item_count` and a stable key per item, and
/// materialises only the range the Renderer last requested plus `buffer` items on each side.
/// Scroll position and item identity stay in the Renderer (D5); the data stays in the Host,
/// so scrolling back re-materialises an identical subtree.
#[component]
pub fn LazyColumn(
    item_count: usize,
    #[props(default = 4)] buffer: usize,
    #[props(default)] key_of: Option<Callback<usize, String>>,
    item: Callback<usize, Element>,
) -> Element {
    let mut range = use_signal(|| (0_usize, 0_usize));
    let (start, count) = range();
    let first = start.saturating_sub(buffer);
    let last = start
        .saturating_add(count)
        .saturating_add(buffer)
        .min(item_count);
    rsx! {
        lazycolumn {
            item_count: item_count as i64,
            onrangerequest: move |event: dioxus_core::Event<RangeRequest>| {
                let requested = event.data();
                range.set((requested.start(), requested.count()));
            },
            for index in first..last {
                {
                    let item_key = key_of
                        .map_or_else(|| index.to_string(), |key_of| key_of.call(index));
                    rsx! {
                        composebox { key: "{item_key}", item_key, {item.call(index)} }
                    }
                }
            }
        }
    }
}
