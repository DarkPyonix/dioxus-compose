//! Compose widgets placed inside an HTML and CSS page.
//!
//! [`WidgetView`] is the page's side of a widget: an element the CSS engine lays out like
//! any other box, whose content blitz-dom knows nothing about. The widget tree itself is a
//! Dioxus app of its own, written with the Compose widgets and kept by a `VirtualDom` and a
//! [`ComposeRenderer`] that write into the same batch the page does, with node ids and
//! handler ids from a range of their own.
//!
//! The box's size is the renderer's to say. The page asks for it with the measure call,
//! once for every widget in one crossing, before it lays the page out, and the answer goes
//! into the box's style as a length. The widget tree is then inserted under the renderer
//! node the page's plan drew for the box, so it sits where the CSS engine placed it.
//!
//! The renderer can only measure a node it has applied, and a node written in the batch
//! being computed is not applied yet. So a widget is measured one call after it was
//! written: the call that writes it asks for one more frame, and that frame lays the page
//! out again with the real size.
//!
//! A click on a widget runs the widget's own handler first and answers that it did not
//! consume the click. The renderer's pointer handling then offers the same press to the
//! clickable box around the widget, so the handlers of the elements around it run after,
//! once each, in the order CSS event flow gives them. The Host does not deliver that second
//! click itself: doing so as well would run the wrapper twice.
//!
//! Every page has its own set of widget trees, named by the owner that was current when the
//! widget's component first ran. A page embedded in a widget tree with `HtmlView` is such a
//! page too, so its widgets are never mixed up with the outer page's.

// The component macro compares its `fn` props for equality. See `html_view`.
#![allow(unpredictable_function_pointer_comparisons)]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::pin::pin;
use std::rc::Rc;
use std::task::Context;

use compose_rust::Batch;
use compose_rust::measure::{MeasureRequests, MeasureStatus, NodeConstraints, measurer};
use compose_rust::protocol::{HostEvent, Mutation, ProtocolError};
use dioxus_core::{Element, VirtualDom};
use dioxus_core_macro::{Props, component, rsx};
use dioxus_html as dioxus_elements;

use crate::host::deliver_event;
use crate::html::NodeId;
use crate::renderer::ComposeRenderer;

/// The first renderer node id a widget tree may use. The page's nodes count up from one and
/// stay far below this, and the pages embedded in widget trees start at `0x8000_0000`.
const WIDGET_NODE_BASE: u32 = 0x4000_0000;
/// How many renderer nodes one widget tree may create.
const WIDGET_NODE_SPAN: u32 = 0x0001_0000;
/// The first handler id a widget tree may use.
const WIDGET_HANDLER_BASE: u64 = 1 << 48;
/// How many handlers one widget tree may give out.
const WIDGET_HANDLER_SPAN: u64 = 1 << 24;
/// How many widget trees one page can hold at once.
const WIDGET_SLOTS: u32 = 2048;

/// One widget tree and what the page knows about its place.
struct Island {
    /// The page this widget belongs to.
    owner: u32,
    dom: VirtualDom,
    renderer: ComposeRenderer,
    /// The id range the tree writes in, released with the island.
    slot: u32,
    /// The tree has been built into a batch.
    built: bool,
    /// The renderer node the tree hangs from.
    root: Option<u32>,
    /// The root has been inserted under the box the page drew.
    attached: bool,
    /// The DOM node of the page's element that stands for this widget.
    place: Option<NodeId>,
    /// What the renderer last measured the root at, in dp.
    size: Option<(f32, f32)>,
    /// The tree's app has work of its own to run.
    dirty: bool,
}

#[derive(Default)]
struct Registry {
    islands: HashMap<u32, Island>,
    next_token: u32,
    next_slot: u32,
    free_slots: Vec<u32>,
    /// The pages that need another frame: a widget was written that the renderer has not
    /// measured, or a widget tree changed after the page was laid out.
    pending: HashSet<u32>,
}

impl Registry {
    fn take_slot(&mut self) -> Option<u32> {
        if let Some(slot) = self.free_slots.pop() {
            return Some(slot);
        }
        (self.next_slot < WIDGET_SLOTS).then(|| {
            let slot = self.next_slot;
            self.next_slot += 1;
            slot
        })
    }
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
    /// The page whose code is running, which a widget registers itself with.
    static CURRENT_OWNER: Cell<u32> = const { Cell::new(0) };
    static NEXT_OWNER: Cell<u32> = const { Cell::new(1) };
}

/// A name for a page, to give its widgets.
pub(crate) fn new_owner() -> u32 {
    NEXT_OWNER.with(|next| {
        let owner = next.get();
        next.set(owner + 1);
        owner
    })
}

/// Makes `owner` the page whose widgets are registered, until it is dropped. The previous
/// owner comes back after, so a page built inside another page's call nests.
pub(crate) struct OwnerScope {
    previous: u32,
}

impl OwnerScope {
    pub(crate) fn enter(owner: u32) -> Self {
        Self {
            previous: CURRENT_OWNER.with(|current| current.replace(owner)),
        }
    }
}

impl Drop for OwnerScope {
    fn drop(&mut self) {
        CURRENT_OWNER.with(|current| current.set(self.previous));
    }
}

/// Keeps one widget tree registered for as long as the component that made it lives.
struct IslandHandle {
    token: u32,
}

impl IslandHandle {
    fn register(app: fn() -> Element) -> Self {
        let token = REGISTRY.with_borrow_mut(|registry| {
            registry.next_token += 1;
            let token = registry.next_token;
            if let Some(slot) = registry.take_slot() {
                let owner = CURRENT_OWNER.with(Cell::get);
                registry.islands.insert(
                    token,
                    Island {
                        owner,
                        dom: VirtualDom::new(app),
                        renderer: ComposeRenderer::with_id_bases(
                            WIDGET_NODE_BASE + slot * WIDGET_NODE_SPAN,
                            WIDGET_HANDLER_BASE + u64::from(slot) * WIDGET_HANDLER_SPAN,
                        ),
                        slot,
                        built: false,
                        root: None,
                        attached: false,
                        place: None,
                        size: None,
                        dirty: false,
                    },
                );
            }
            token
        });
        Self { token }
    }
}

impl Drop for IslandHandle {
    fn drop(&mut self) {
        // The thread may be shutting down, in which case the registry is already gone.
        let _ = REGISTRY.try_with(|registry| {
            if let Ok(mut registry) = registry.try_borrow_mut()
                && let Some(island) = registry.islands.remove(&self.token)
            {
                registry.free_slots.push(island.slot);
            }
        });
    }
}

/// A Compose widget tree inside an HTML and CSS page.
///
/// `app` is the widget tree: a Dioxus component written with the Compose widgets, the same
/// kind of function [`launch`](crate::launch) takes. It is an element of the page like any
/// other, so `style` can place it (`display: inline-block`, margins, `position`). Its size
/// is the one the renderer measures for the tree, unless `style` gives a `width` or a
/// `height`.
///
/// ```ignore
/// rsx! {
///     div { onclick: move |_| clicks += 1,
///         "Name: "
///         WidgetView { app: name_field, style: "display: inline-block" }
///     }
/// }
/// ```
///
/// The CSS of the page does not reach inside the tree. The design system draws it.
#[component]
pub fn WidgetView(app: fn() -> Element, #[props(default)] style: Option<String>) -> Element {
    let handle = dioxus_core::use_hook(|| Rc::new(IslandHandle::register(app)));
    let token = handle.token;
    let style = style.unwrap_or_default();
    rsx! {
        div {
            "data-compose-widget": "{token}",
            style: "{style}",
        }
    }
}

/// The size the renderer measured for the widget `token`, and a record that the page
/// element `place` is where it goes. `None` until the renderer has measured the widget.
pub(crate) fn size_of(token: u32, place: NodeId) -> Option<(f32, f32)> {
    REGISTRY.with_borrow_mut(|registry| {
        let island = registry.islands.get_mut(&token)?;
        island.place = Some(place);
        island.size
    })
}

/// Builds or updates every widget tree, writing into `batch`.
pub(crate) fn render_islands(owner: u32, batch: &mut Batch) {
    REGISTRY.with_borrow_mut(|registry| {
        let mut changed_after_layout = false;
        for island in registry.islands.values_mut() {
            if island.owner != owner {
                continue;
            }
            std::mem::swap(island.renderer.batch_mut(), batch);
            if island.built {
                island.dom.render_immediate(&mut island.renderer);
                // What the tree changed to is measured on the next frame.
                changed_after_layout |= std::mem::take(&mut island.dirty);
            } else {
                island.dom.rebuild(&mut island.renderer);
                island.built = true;
                island.dirty = false;
            }
            std::mem::swap(island.renderer.batch_mut(), batch);
            if island.root.is_none() {
                island.root = island.renderer.root_node();
            }
        }
        if changed_after_layout {
            registry.pending.insert(owner);
        }
    });
}

/// Asks the renderer how big every attached widget is, in one call, with at most `width`
/// dp of room. A widget that was written in an earlier call is applied by now; one that
/// was written in this call is not measured, because the renderer does not know it yet.
pub(crate) fn refresh(owner: u32, width: f32) {
    REGISTRY.with_borrow_mut(|registry| {
        let mut requests = MeasureRequests::new();
        let mut tokens = Vec::new();
        for (&token, island) in &registry.islands {
            if let (true, true, Some(root)) = (island.owner == owner, island.attached, island.root)
            {
                let _ = requests.node(
                    root,
                    NodeConstraints {
                        min_width: 0.0,
                        max_width: width,
                        min_height: 0.0,
                        max_height: f32::INFINITY,
                    },
                );
                tokens.push(token);
            }
        }
        if tokens.is_empty() {
            return;
        }
        // Outside a Host call there is no renderer standing still to answer.
        let Ok(mut asker) = measurer() else {
            return;
        };
        let mut results = Vec::new();
        if asker.measure(&mut requests, &mut results).is_err() {
            return;
        }
        for (token, result) in tokens.iter().zip(results.iter()) {
            if result.status() != MeasureStatus::Ok {
                continue;
            }
            if let Some(island) = registry.islands.get_mut(token) {
                island.size = Some((result.width, result.height));
            }
        }
    });
}

/// Inserts every widget tree that is built and not yet in place under the renderer node
/// `host_of` names for the page element it stands for, and asks for a frame to measure it.
pub(crate) fn attach(owner: u32, host_of: &dyn Fn(NodeId) -> Option<u32>, batch: &mut Batch) {
    REGISTRY.with_borrow_mut(|registry| {
        let mut attached_any = false;
        for island in registry.islands.values_mut() {
            if island.owner != owner || island.attached {
                continue;
            }
            let (Some(root), Some(place)) = (island.root, island.place) else {
                continue;
            };
            let Some(host) = host_of(place) else {
                continue;
            };
            batch.write(Mutation::Insert {
                parent_id: host,
                node_id: root,
                index: 0,
            });
            island.attached = true;
            attached_any = true;
        }
        if attached_any {
            registry.pending.insert(owner);
        }
    });
}

/// Asks every widget tree whether it has work, and says whether a frame is needed: some tree
/// did, or the page is waiting to measure a widget. Every tree is polled even after one is
/// ready, so each registers the waker.
pub(crate) fn poll_islands(owner: u32, context: &mut Context<'_>) -> bool {
    REGISTRY.with_borrow_mut(|registry| {
        let mut ready = registry.pending.remove(&owner);
        for island in registry.islands.values_mut() {
            if island.owner != owner {
                continue;
            }
            let work = island.dom.wait_for_work();
            let mut work = pin!(work);
            if work.as_mut().poll(context).is_ready() {
                island.dirty = true;
                ready = true;
            }
        }
        ready
    })
}

/// Hands an event to the widget tree whose handler it names. `None` when the handler is not
/// a widget tree's, which means it belongs to the page. Handler ids are unique across
/// pages, so the owner does not need to be named.
pub(crate) fn route_event(event: &HostEvent<'_>) -> Option<Result<i64, ProtocolError>> {
    REGISTRY.with_borrow_mut(|registry| {
        let island = registry
            .islands
            .values_mut()
            .find(|island| island.renderer.handler(event.handler_id).is_some())?;
        Some(deliver_event(&mut island.dom, &island.renderer, event))
    })
}
