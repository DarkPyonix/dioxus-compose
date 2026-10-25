//! `ListDetail`: a list and a detail side by side, or one at a time when the place is
//! narrow. A composition over `SplitPane`, with no widget of its own.

use dioxus_core::{Element, EventHandler};
use dioxus_core_macro::{Props, component, rsx};

use crate as dioxus_elements;
use crate::hooks::use_node_size;
use crate::widgets::{Column, SplitPane};

/// A list and the detail of what is selected in it.
///
/// `selected` is the application's: `Some` means the detail should show where only one
/// pane fits, `None` means the list. Going back calls `on_select_change` with `None` once.
/// Clearing the selection while both panes are on screen is the application's to do. The
/// composition keeps no selection of its own, so narrowing and widening the place leaves
/// `selected` as it was.
///
/// Both panes are children of one `SplitPane`, so which of them shows is a property
/// change and neither is rebuilt when the selection changes. The width below which one
/// pane shows is the design system's, unless `collapse_below` is given.
///
/// With `collapse_below: Some(width)` the composition measures its own width and, below
/// `width` dp, draws only the pane `selected` points at. The detail brings its own back
/// control, which calls `on_select_change` with `None`. Nothing is measured until the Renderer reports a size, and until then the
/// design system decides. Crossing the threshold rebuilds the panes, because they move
/// between parents.
///
/// ```ignore
/// ListDetail::<SessionId> {
///     selected: selected(),
///     on_select_change: move |next| selected.set(next),
///     list: rsx! { SessionList {} },
///     detail: rsx! { SessionView {} },
/// }
/// ```
#[component]
pub fn ListDetail<K: Clone + PartialEq + 'static>(
    /// What is selected, or `None` for the list.
    #[props(default)]
    selected: Option<K>,
    /// Called with `None` when the user goes back from the detail.
    #[props(default)]
    on_select_change: EventHandler<Option<K>>,
    /// The list pane.
    list: Element,
    /// The detail pane.
    detail: Element,
    /// Below this width in dp the composition shows one pane, whatever the design system
    /// would do. Left out, the design system decides.
    #[props(default)]
    collapse_below: Option<f32>,
    #[props(default)] weight: Option<f32>,
    #[props(default)] fill_max_width: bool,
    #[props(default)] fill_max_height: bool,
    /// The list pane's width in dp. Left out, the design system's sidebar width.
    #[props(default)]
    value: Option<f32>,
    /// The narrowest the list pane may be dragged, in dp.
    #[props(default)]
    min: Option<f32>,
    /// The widest the list pane may be dragged, in dp.
    #[props(default)]
    max: Option<f32>,
    /// What a screen reader calls the divider.
    #[props(default)]
    label: Option<String>,
) -> Element {
    let size = use_node_size();
    let pane = if selected.is_some() { 1 } else { 0 };
    let measured = size.measured().width_dp;
    let forced_single = collapse_below.is_some_and(|below| measured > 0.0 && measured < below);
    rsx! {
        Column {
            observe_size: collapse_below.map(|_| size.token()),
            weight,
            fill_max_width,
            fill_max_height,
            if forced_single {
                // Going back is the application's own control in the detail, calling
                // on_select_change(None); the system back gesture will go the same way
                // once use_back exists.
                if pane == 1 {
                    {detail}
                } else {
                    {list}
                }
            } else {
                SplitPane {
                    fill_max_width: true,
                    fill_max_height: true,
                    value,
                    min,
                    max,
                    label,
                    selected_index: pane,
                    on_dismiss: move |_| on_select_change.call(None),
                    {list}
                    {detail}
                }
            }
        }
    }
}
