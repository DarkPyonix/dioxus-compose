//! Listeners for the motion events `dioxus-html` has no attribute for.
//!
//! `dioxus-html` declares `onanimationstart`, `onanimationiteration`, `onanimationend` and
//! `ontransitionend`. The Host also sends `transitionrun`, `transitionstart`,
//! `transitioncancel` and `animationcancel`, which `rsx!` cannot spell on an element.
//! Each function here builds the listener attribute for one of them, to be spread into an
//! element: `div { ..[ontransitionrun(|event| { /* ... */ })] }`.

use dioxus_core::{Attribute, AttributeValue, Event};
use dioxus_html::{AnimationData, PlatformEventData, TransitionData};

macro_rules! motion_listener {
    ($(#[$doc:meta])* $name:ident, $data:ty) => {
        $(#[$doc])*
        pub fn $name(mut handler: impl FnMut(Event<$data>) + 'static) -> Attribute {
            Attribute::new(
                stringify!($name),
                AttributeValue::listener(move |e: Event<PlatformEventData>| {
                    let event: Event<$data> = e.map(|data| data.into());
                    handler(event);
                }),
                None,
                false,
            )
        }
    };
}

motion_listener!(
    /// A transition of a property of the element was sent to the renderer. It comes before
    /// `transitionstart`.
    ontransitionrun,
    TransitionData
);
motion_listener!(
    /// A transition of a property of the element left its delay and began to change the
    /// value.
    ontransitionstart,
    TransitionData
);
motion_listener!(
    /// A transition that had not ended was replaced by another of the same property, or
    /// the element went away.
    ontransitioncancel,
    TransitionData
);
motion_listener!(
    /// A `@keyframes` animation that had not ended was replaced, taken off the element, or
    /// the element went away.
    onanimationcancel,
    AnimationData
);
