//! An HTML and CSS subtree placed inside a tree of Compose widgets.
//!
//! [`HtmlView`] is a widget like any other as far as the tree around it can tell: it is an
//! `AbsoluteBox` whose size the parent's Compose layout decides. Inside it is an HTML page,
//! its own Dioxus app laid out by the CSS engine at the width the box was given. The
//! renderer reports that width as the box's measured size, and the page is laid out again
//! when it changes. Nothing else in the widget tree is touched when it does.
//!
//! The page is an [`HtmlRuntime`](crate::html::HtmlRuntime) that writes into the batch of
//! the widget tree. It takes its renderer node ids and handler ids from a range of its own,
//! so they never collide with the ones the widget tree hands out, and an event whose
//! handler id falls in that range goes to the page.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::task::Context;

use compose_rust::protocol::{HostEvent, Mutation, ProtocolError};
use compose_rust::{Batch, Runtime};
use dioxus_core::Element;
use dioxus_core_macro::{Props, component, rsx};

use crate as dioxus_elements;
use crate::html::{HtmlConfig, HtmlRuntime};
use crate::renderer::ComposeRenderer;

/// The first renderer node id an island may use. The widget tree numbers its nodes up from
/// one and stays far below this.
const ISLAND_NODE_BASE: u32 = 0x8000_0000;
/// How many renderer nodes one island may create before it reuses ids.
const ISLAND_NODE_SPAN: u32 = 0x0010_0000;
/// The first handler id an island may use.
const ISLAND_HANDLER_BASE: u64 = 1 << 40;
/// How many handlers one island may give out.
const ISLAND_HANDLER_SPAN: u64 = 1 << 24;
/// How many islands one tree can hold at once.
const ISLAND_SLOTS: u32 = 2048;

/// The viewport an island is laid out in before the renderer has measured its box.
const FALLBACK_SIZE: (f32, f32) = (800.0, 600.0);

/// One page and what the host last gave it.
struct Island {
    runtime: HtmlRuntime,
    /// The id range the page writes in, released with the island.
    slot: u32,
    /// The width the box was last measured at, in dp.
    width: f32,
    /// The viewport the page was last laid out in.
    drawn: Option<(f32, f32)>,
    /// The page has work of its own to run before it draws.
    dirty: bool,
}

#[derive(Default)]
struct Registry {
    islands: HashMap<u32, Island>,
    next_slot: u32,
    free_slots: Vec<u32>,
}

impl Registry {
    fn take_slot(&mut self) -> Option<u32> {
        if let Some(slot) = self.free_slots.pop() {
            return Some(slot);
        }
        (self.next_slot < ISLAND_SLOTS).then(|| {
            let slot = self.next_slot;
            self.next_slot += 1;
            slot
        })
    }
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Keeps one island registered for as long as the component that made it lives.
struct IslandHandle {
    token: u32,
}

impl IslandHandle {
    fn register(token: u32, app: fn() -> Element, config: Option<fn() -> HtmlConfig>) -> Self {
        REGISTRY.with_borrow_mut(|registry| {
            if registry.islands.contains_key(&token) {
                return;
            }
            let Some(slot) = registry.take_slot() else {
                return;
            };
            let runtime = HtmlRuntime::embedded(
                app,
                config.map(|config| config()).unwrap_or_default(),
                ISLAND_NODE_BASE + slot * ISLAND_NODE_SPAN,
                ISLAND_HANDLER_BASE + u64::from(slot) * ISLAND_HANDLER_SPAN,
            );
            registry.islands.insert(
                token,
                Island {
                    runtime,
                    slot,
                    width: 0.0,
                    drawn: None,
                    dirty: false,
                },
            );
        });
        Self { token }
    }
}

impl Drop for IslandHandle {
    fn drop(&mut self) {
        // The thread may be shutting down, in which case the registry is already gone.
        let _ = REGISTRY.try_with(|registry| {
            if let Ok(mut registry) = registry.try_borrow_mut() {
                if let Some(island) = registry.islands.remove(&self.token) {
                    registry.free_slots.push(island.slot);
                }
            }
        });
    }
}

/// Records the width the renderer measured for the island's box.
fn set_width(token: u32, width: f32) {
    REGISTRY.with_borrow_mut(|registry| {
        if let Some(island) = registry.islands.get_mut(&token) {
            island.width = width;
        }
    });
}

/// The viewport an island is laid out in: its box's measured width and height, and the
/// window's size for whichever the renderer has not measured yet.
pub(crate) fn viewport_for(measured: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let pick = |measured: f32, window: f32, fallback: f32| {
        if measured > 0.0 {
            measured
        } else if window > 0.0 {
            window
        } else {
            fallback
        }
    };
    (
        pick(measured.0, window.0, FALLBACK_SIZE.0),
        pick(measured.1, window.1, FALLBACK_SIZE.1),
    )
}

/// Writes every island whose box the renderer has, into the batch the widget tree writes.
///
/// An island is laid out and written again only when its viewport changed or its own app
/// had work to run, so a window that changes width costs the pages whose boxes changed and
/// nothing else.
pub(crate) fn draw_islands(renderer: &mut ComposeRenderer) {
    let hosts = renderer.observed_nodes();
    if hosts.is_empty() {
        return;
    }
    REGISTRY.with_borrow_mut(|registry| {
        for (host_node, token) in hosts {
            let Some(island) = registry.islands.get_mut(&token) else {
                continue;
            };
            let measured = compose_rust::window::node_size(token);
            let window = compose_rust::window_size();
            let viewport = viewport_for(
                (island.width, measured.height_dp),
                (window.width_dp, window.height_dp),
            );
            if island.drawn == Some(viewport) && !island.dirty {
                continue;
            }
            let run_dom = std::mem::take(&mut island.dirty);
            island.drawn = Some(viewport);
            let batch: &mut Batch = renderer.batch_mut();
            if let Some(root) = island.runtime.render_into(viewport, run_dom, batch) {
                batch.write(Mutation::Insert {
                    parent_id: host_node,
                    node_id: root,
                    index: 0,
                });
            }
        }
    });
}

/// Asks every island whether its app has work, and marks the ones that do. Returns whether
/// any did. Every island is polled even after one is ready, so each registers the waker.
pub(crate) fn poll_islands(context: &mut Context<'_>) -> bool {
    REGISTRY.with_borrow_mut(|registry| {
        let mut ready = false;
        for island in registry.islands.values_mut() {
            if island.runtime.poll_work(context).is_ready() {
                island.dirty = true;
                ready = true;
            }
        }
        ready
    })
}

/// Hands an event to the island whose handler it names. `None` when the handler is not an
/// island's, which means it belongs to the widget tree.
pub(crate) fn route_event(event: &HostEvent<'_>) -> Option<Result<i64, ProtocolError>> {
    REGISTRY.with_borrow_mut(|registry| {
        registry
            .islands
            .values_mut()
            .find(|island| island.runtime.owns_handler(event.handler_id))
            .map(|island| island.runtime.handle_event(event))
    })
}

/// An HTML and CSS page inside a Compose widget tree.
///
/// `app` is the page: a Dioxus component written with HTML elements and CSS, the same kind
/// of function [`html::launch`](crate::html::launch) takes. It is laid out at the width the
/// parent's layout gives this box, and again when that width changes. By default the box
/// fills the width it is offered; give it a `height`, a `weight` or `fill_max_height` to
/// say how tall it is.
///
/// ```ignore
/// rsx! {
///     Column {
///         Text { text: "Notes" }
///         HtmlView { app: article, height: 320.0 }
///     }
/// }
/// ```
#[component]
pub fn HtmlView(
    app: fn() -> Element,
    /// How the page is set up: stylesheets, base URL, images. The default when left out.
    #[props(default)]
    config: Option<fn() -> HtmlConfig>,
    #[props(default)] weight: Option<f32>,
    #[props(default)] width: Option<f32>,
    #[props(default)] height: Option<f32>,
    #[props(default = true)] fill_max_width: bool,
    #[props(default)] fill_max_height: bool,
) -> Element {
    let size = crate::hooks::use_node_size();
    let token = size.token() as u32;
    dioxus_core::use_hook(|| Rc::new(IslandHandle::register(token, app, config)));
    set_width(token, size.measured().width_dp);
    rsx! {
        absolutebox {
            observe_size: size.token(),
            weight: weight.map(f64::from),
            width: width.map(f64::from),
            height: height.map(f64::from),
            fill_max_width,
            fill_max_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::viewport_for;

    #[test]
    fn fr34_3_viewport_uses_the_measured_box_then_the_window_then_a_default() {
        assert_eq!(viewport_for((300.0, 200.0), (900.0, 700.0)), (300.0, 200.0));
        assert_eq!(viewport_for((0.0, 0.0), (900.0, 700.0)), (900.0, 700.0));
        assert_eq!(viewport_for((300.0, 0.0), (0.0, 0.0)), (300.0, 600.0));
    }
}
