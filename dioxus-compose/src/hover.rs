//! `use_hover`: whether the pointer is over a node, kept as a signal.
//!
//! It is the `pointer_kinds` and `on_pointer` pair every hover highlight writes by hand,
//! with the `Signal<bool>` that goes with it. It adds no event and no boundary call.

use compose_rust::schema::PointerKind;
use compose_rust::{PointerEvent, PointerKinds};
use dioxus_signals::{ReadableExt as _, Signal, WritableExt as _};

/// The hover state of one node, from [`use_hover`].
///
/// Copy it into the node's `pointer_kinds` and `on_pointer`, and read `is_hovered` where
/// the highlight is chosen.
#[derive(Clone, Copy)]
pub struct HoverHandle {
    hovered: Signal<bool>,
}

impl HoverHandle {
    /// Whether the pointer is over the node. Reading it subscribes the calling component,
    /// which draws again when the answer changes.
    pub fn is_hovered(&self) -> bool {
        *self.hovered.read()
    }

    /// The kinds to subscribe the node to: the pointer entering and leaving, and nothing
    /// else, so a moving pointer costs no event.
    pub const fn kinds(&self) -> PointerKinds {
        PointerKinds::ENTER.union(PointerKinds::LEAVE)
    }

    /// The handler for the node's `on_pointer`. It sets the state on `Enter` and clears it
    /// on `Leave`, writes nothing when the state is already that, and never consumes the
    /// event, so an ancestor's handler and the Renderer's own handling still get it.
    pub fn handler(&self) -> impl FnMut(PointerEvent) + 'static {
        let mut hovered = self.hovered;
        move |event: PointerEvent| {
            let next = match event.kind() {
                PointerKind::Enter => true,
                PointerKind::Leave => false,
                _ => return,
            };
            if *hovered.peek() != next {
                hovered.set(next);
            }
        }
    }
}

/// Tracks whether the pointer is over a node.
///
/// ```ignore
/// let hover = use_hover();
/// rsx! {
///     Row {
///         pointer_kinds: hover.kinds(),
///         on_pointer: hover.handler(),
///         background: if hover.is_hovered() { Some(row_hover_paint) } else { None },
///     }
/// }
/// ```
///
/// A touch screen sends no `Enter`, so the answer stays false there: a control that shows
/// only on hover cannot be reached by touch, and keeping that from happening is the
/// application's job.
pub fn use_hover() -> HoverHandle {
    HoverHandle {
        hovered: dioxus_hooks::use_signal(|| false),
    }
}
