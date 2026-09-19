#![deny(unsafe_op_in_unsafe_fn)]

pub mod protocol;
pub mod schema;

pub use dioxus_core::{Element, VirtualDom};
pub use dioxus_core_macro::{component, rsx};
pub use schema::{EventPayload, LoopMode, Modifier, SCHEMA_HASH, Selection, WidgetKind};

pub mod prelude {
    pub use crate::elements as dioxus_elements;
    pub use crate::{Element, LaunchBuilder, LoopMode, Modifier, component, launch, rsx};
    pub use dioxus_core::{Callback, Event, EventHandler, Properties, VirtualDom};
    pub use dioxus_hooks::*;
    pub use dioxus_signals::*;
}

// The implementation is added with the host boundary module.
pub fn launch(_app: fn() -> Element) {
    unimplemented!("host boundary is not initialized")
}

pub struct LaunchBuilder;

pub mod elements {
    // Filled by the Dioxus renderer implementation.
}
