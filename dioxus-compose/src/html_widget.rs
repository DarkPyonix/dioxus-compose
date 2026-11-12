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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::pin::pin;
use std::rc::Rc;
use std::task::Context;

use compose_rust::Batch;
use compose_rust::measure::{MeasureRequests, MeasureStatus, NodeConstraints, measurer};
use compose_rust::protocol::{HostEvent, Mutation, ProtocolError};
use dioxus_core::{ComponentFunction, Element, Properties, ScopeId, SuperInto, VirtualDom};
use dioxus_core_macro::rsx;
use dioxus_html as dioxus_elements;
use dioxus_signals::{ReadableExt, Signal, WritableExt};

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
    /// The size the page's layout gave the widget's box, as the tree reads it with
    /// [`use_widget_box`].
    box_size: Signal<(f32, f32)>,
    /// Counts the props the tree's root was handed. The root reads it, so a new count draws
    /// the root again.
    props_drawn: Signal<u64>,
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
    /// `root` draws the tree's root component with the props the widget holds now.
    fn register(root: impl Fn() -> Element + Clone + 'static) -> Self {
        let token = REGISTRY.with_borrow_mut(|registry| {
            registry.next_token += 1;
            let token = registry.next_token;
            if let Some(slot) = registry.take_slot() {
                let owner = CURRENT_OWNER.with(Cell::get);
                let dom = VirtualDom::new_with_props(root, ());
                let box_size = dom.in_scope(ScopeId::ROOT, || {
                    Signal::new_in_scope((0.0, 0.0), ScopeId::ROOT)
                });
                dom.provide_root_context(WidgetBox(box_size));
                let props_drawn =
                    dom.in_scope(ScopeId::ROOT, || Signal::new_in_scope(0, ScopeId::ROOT));
                dom.provide_root_context(PropsDrawn(props_drawn));
                registry.islands.insert(
                    token,
                    Island {
                        owner,
                        dom,
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
                        box_size,
                        props_drawn,
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

/// Says that the props of the widget `token` changed, so its tree draws again with them on
/// the next frame, in the same scope, keeping its hook state and its nodes.
fn props_changed(token: u32) {
    REGISTRY.with_borrow_mut(|registry| {
        let Some(island) = registry.islands.get_mut(&token) else {
            return;
        };
        let mut signal = island.props_drawn;
        island.dom.in_scope(ScopeId::ROOT, || signal += 1);
        island.dirty = true;
        let owner = island.owner;
        registry.pending.insert(owner);
    });
}

/// The props of [`WidgetView`].
///
/// `P` is the props type of the widget tree's root component, `()` for a root that takes
/// none. The props are compared with `PartialEq`: the tree draws again only when they
/// differ from the last ones it drew with.
pub struct WidgetViewProps<P: 'static = ()> {
    app: Rc<dyn Fn(P) -> Element>,
    props: P,
    style: Option<String>,
}

impl<P: Clone> Clone for WidgetViewProps<P> {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            props: self.props.clone(),
            style: self.style.clone(),
        }
    }
}

/// The tree's root component is read once, when the widget is first drawn, the way a
/// `VirtualDom` keeps the root it was made with. Only the props and the style decide
/// whether the widget changed.
impl<P: PartialEq> PartialEq for WidgetViewProps<P> {
    fn eq(&self, other: &Self) -> bool {
        self.props == other.props && self.style == other.style
    }
}

impl<P: Clone + PartialEq + 'static> Properties for WidgetViewProps<P> {
    type Builder = WidgetViewPropsBuilder<P, NoApp, NoProps>;

    fn builder() -> Self::Builder {
        WidgetViewPropsBuilder {
            app: None,
            props: None,
            style: None,
            state: PhantomData,
        }
    }

    fn memoize(&mut self, other: &Self) -> bool {
        let equal = self == other;
        if !equal {
            *self = other.clone();
        }
        equal
    }
}

#[doc(hidden)]
pub struct NoApp;
#[doc(hidden)]
pub struct WithApp;
#[doc(hidden)]
pub struct NoProps;
#[doc(hidden)]
pub struct WithProps;

/// Builds [`WidgetViewProps`]. `rsx!` uses it; `app` is required, and `props` is required
/// unless the root component takes no props.
pub struct WidgetViewPropsBuilder<P: 'static, A, Q> {
    app: Option<Rc<dyn Fn(P) -> Element>>,
    props: Option<P>,
    style: Option<String>,
    state: PhantomData<(A, Q)>,
}

impl<P: 'static, Q> WidgetViewPropsBuilder<P, NoApp, Q> {
    /// The widget tree's root component: a `fn() -> Element`, or a component that takes
    /// props `P`.
    pub fn app<M: 'static>(
        self,
        app: impl ComponentFunction<P, M>,
    ) -> WidgetViewPropsBuilder<P, WithApp, Q> {
        WidgetViewPropsBuilder {
            app: Some(Rc::new(move |props| app.rebuild(props))),
            props: self.props,
            style: self.style,
            state: PhantomData,
        }
    }
}

impl<P: 'static, A> WidgetViewPropsBuilder<P, A, NoProps> {
    /// The props the root component is drawn with.
    pub fn props(self, props: P) -> WidgetViewPropsBuilder<P, A, WithProps> {
        WidgetViewPropsBuilder {
            app: self.app,
            props: Some(props),
            style: self.style,
            state: PhantomData,
        }
    }
}

impl<P: 'static, A, Q> WidgetViewPropsBuilder<P, A, Q> {
    /// The CSS of the element that stands for the widget in the page.
    pub fn style<M>(mut self, style: impl SuperInto<Option<String>, M>) -> Self {
        self.style = style.super_into();
        self
    }
}

impl WidgetViewPropsBuilder<(), WithApp, NoProps> {
    /// The props of a widget whose root takes none.
    pub fn build(self) -> WidgetViewProps<()> {
        WidgetViewProps {
            app: self.app.expect("the app was set"),
            props: (),
            style: self.style,
        }
    }
}

impl<P: 'static> WidgetViewPropsBuilder<P, WithApp, WithProps> {
    /// The props of a widget whose root takes `props`.
    pub fn build(self) -> WidgetViewProps<P> {
        WidgetViewProps {
            app: self.app.expect("the app was set"),
            props: self.props.expect("the props were set"),
            style: self.style,
        }
    }
}

/// The count of props a widget tree's root was handed, through its root context.
#[derive(Clone, Copy)]
struct PropsDrawn(Signal<u64>);

/// What a [`WidgetView`] keeps across its renders.
struct WidgetState<P> {
    handle: IslandHandle,
    /// The props the tree's root draws with, shared with the tree's root.
    props: Rc<RefCell<P>>,
}

/// A Compose widget tree inside an HTML and CSS page.
///
/// `app` is the widget tree: a Dioxus component written with the Compose widgets, the same
/// kind of function [`launch`](crate::launch) takes, or a component that takes props, with
/// the props given in `props`, the way `VirtualDom::new_with_props` takes them. It is an
/// element of the page like any other, so `style` can place it (`display: inline-block`,
/// margins, `position`). Its size is the one the renderer measures for the tree, unless
/// `style` gives a `width` or a `height`.
///
/// ```ignore
/// #[component]
/// fn Editor(path: String) -> Element {
///     rsx! { Text { text: "{path}" } }
/// }
///
/// rsx! {
///     div { onclick: move |_| clicks += 1,
///         "Name: "
///         WidgetView { app: name_field, style: "display: inline-block" }
///         WidgetView { app: Editor, props: EditorProps { path: path() } }
///     }
/// }
/// ```
///
/// When the page draws again with props that differ (`PartialEq`), the same tree draws
/// again with them: its hook state and its nodes stay. Equal props do not draw it again.
/// `app` is read once, when the widget is first drawn.
///
/// The CSS of the page does not reach inside the tree. The design system draws it.
#[allow(non_snake_case)]
pub fn WidgetView<P: Clone + PartialEq + 'static>(props: WidgetViewProps<P>) -> Element {
    let WidgetViewProps {
        app,
        props: values,
        style,
    } = props;
    let state = dioxus_core::use_hook(|| {
        let shared = Rc::new(RefCell::new(values.clone()));
        let root = {
            let shared = shared.clone();
            move || {
                let PropsDrawn(drawn) = dioxus_core::consume_context::<PropsDrawn>();
                // Read only to subscribe the root to new props.
                drawn();
                let values = shared.borrow().clone();
                app(values)
            }
        };
        Rc::new(WidgetState {
            handle: IslandHandle::register(root),
            props: shared,
        })
    });
    let token = state.handle.token;
    if *state.props.borrow() != values {
        *state.props.borrow_mut() = values;
        props_changed(token);
    }
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
        // What the application asked of its code editor handles goes to the editor that carries
        // each handle: a handle in a widget tree names a node of that tree's renderer, which the
        // page's own flush does not know. A handle no tree carries is dropped here.
        crate::hooks::drain_edits(|request| {
            let token = request.token();
            if let Some(island) = registry.islands.values_mut().find(|island| {
                island.owner == owner && island.renderer.editor_node(token).is_some()
            }) {
                std::mem::swap(island.renderer.batch_mut(), batch);
                island.renderer.apply_editor_request(request);
                std::mem::swap(island.renderer.batch_mut(), batch);
            }
        });
        if changed_after_layout {
            registry.pending.insert(owner);
        }
    });
}

/// Asks the renderer how big every attached widget is, in one call, with at most `width`
/// dp of room, and returns the page nodes whose measured size changed. A widget that was
/// written in an earlier call is applied by now; one that was written in this call is not
/// measured, because the renderer does not know it yet.
pub(crate) fn refresh(owner: u32, width: f32) -> Vec<NodeId> {
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
            return Vec::new();
        }
        // Outside a Host call there is no renderer standing still to answer.
        let Ok(mut asker) = measurer() else {
            return Vec::new();
        };
        let mut results = Vec::new();
        if asker.measure(&mut requests, &mut results).is_err() {
            return Vec::new();
        }
        let mut changed = Vec::new();
        for (token, result) in tokens.iter().zip(results.iter()) {
            if result.status() != MeasureStatus::Ok {
                continue;
            }
            if let Some(island) = registry.islands.get_mut(token) {
                let new_size = Some((result.width, result.height));
                if island.size != new_size {
                    island.size = new_size;
                    if let Some(place) = island.place {
                        changed.push(place);
                    }
                }
            }
        }
        changed
    })
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

/// The widget's box as the page's layout placed it, handed to the tree through its root
/// context.
#[derive(Clone, Copy)]
struct WidgetBox(Signal<(f32, f32)>);

/// The size, in dp, that the page's layout gave the box this widget tree is in: `(0, 0)`
/// until the page has been laid out once. A widget that should fill a box a stylesheet
/// sizes (`flex: 1`, `height: 100%`) gives that size to its own root, because the renderer
/// measures a tree without any limit, so a `fill_max_*` root has nothing to fill. The tree
/// is drawn again when the box changes size.
///
/// Call it from inside a [`WidgetView`] tree only.
pub fn use_widget_box() -> (f32, f32) {
    let WidgetBox(size) = dioxus_core::consume_context::<WidgetBox>();
    size()
}

/// Gives every widget tree of `owner` the size `box_of` says its page element has, and asks
/// for a frame when any tree's size changed.
pub(crate) fn set_boxes(owner: u32, box_of: &dyn Fn(NodeId) -> Option<(f32, f32)>) {
    REGISTRY.with_borrow_mut(|registry| {
        let mut changed = false;
        for island in registry.islands.values_mut() {
            if island.owner != owner {
                continue;
            }
            let Some(size) = island.place.and_then(box_of) else {
                continue;
            };
            let mut signal = island.box_size;
            if *signal.peek() != size {
                island.dom.in_scope(ScopeId::ROOT, || signal.set(size));
                changed = true;
            }
        }
        if changed {
            registry.pending.insert(owner);
        }
    });
}
