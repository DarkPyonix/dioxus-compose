//! A screen written with HTML elements and CSS, as a compose-rust runtime.
//!
//! The Host drives it like any other runtime: build the tree, write what changed, run the
//! handler an event names. Every frame that renders goes the whole way: the `VirtualDom`
//! writes into the [`HtmlDom`]'s document, the document is laid out at the window's size,
//! the display list becomes a [`Plan`](crate::html::Plan), and the [`PlanBridge`] writes what
//! differs from the last plan into the batch. Events come back through the handlers the
//! bridge gave out and reach the DOM node's Dioxus handler: a click through
//! [`HtmlDom::click_node`], text through [`HtmlDom::input`] and [`HtmlDom::change`], Enter
//! through [`HtmlDom::submit`], a checkbox through [`HtmlDom::check`] and a select through
//! [`HtmlDom::select`]. A key goes through [`HtmlDom::key_down`], from the field it was
//! pressed in or from the box of an element that listens for keys, and a field's focus
//! through [`HtmlDom::focus`] and [`HtmlDom::blur`].
//!
//! compose-rust tells the Host when a field loses the focus, not when it gains it. A
//! field has the focus by the time anything the user does in it arrives, so `focus` is
//! delivered just before the first of those: the first text, key or Enter since the field
//! last lost the focus. A field focused and left without a keystroke gets its `blur` and
//! no `focus`. The keys compose-rust carries are key presses, and of those only Enter, so
//! `keyup` is never delivered from here.
//!
//! `prefers-color-scheme` follows the theme's colour scheme when the application says
//! light or dark. When it follows the system, it follows the light or dark appearance the
//! renderer reports, and the scheme the configuration names until the first report.
//!
//! Text is measured by the configured [`TextMeasurer`](crate::html::TextMeasurer), the
//! renderer's measure call by default ([`RendererMeasurer`](crate::html::RendererMeasurer)),
//! and the renderer draws each run at the width that came out of it. The page asks for all
//! the runs a layout pass needs in one call. Parley answers while there is no renderer to
//! ask. The measure call also sizes the Compose widgets the page holds
//! ([`WidgetView`](crate::html::WidgetView)), whose content only the renderer knows.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll};

use std::cell::RefCell;
use std::rc::Rc;

use compose_rust::animation::{AnimationEventSubscription, on_animation_event};
use compose_rust::protocol::{HostEvent, ProtocolError};
use compose_rust::schema::{
    AnimationEventKind, ColorScheme as ThemeScheme, EventPayload, Key as WireKey,
    PointerKind as WirePointerKind, PointerType as WirePointerType,
    WheelDeltaMode as WireWheelMode,
};
use compose_rust::{AnimationMoment, Batch, Runtime, WindowSize};
use dioxus_core::{Element, ScopeId};
use dioxus_html::{Code, Key, Modifiers};

use crate::dom::{HtmlConfig, HtmlDom, PointerInput, PointerKind, PointerType, WheelMode};
use crate::html::scroll::{Report, ScrollSync};
use crate::html::{ColorScheme, NodeId, PlanKey, Rect};
use crate::paint::animate;
use crate::paint::bridge::{BridgeEvent, HostMomentKind, PlanBridge};
use crate::paint::plan::LiteralColours;

/// The viewport a page is laid out in before the renderer has said how large the window
/// is. The renderer reports the size of the page's box when it first lays it out, and the
/// page is laid out again at that size.
const FALLBACK_VIEWPORT: (f32, f32) = (800.0, 600.0);

/// The factor the page is drawn at: what the renderer reported, kept to a number a page can
/// be laid out at. One before the renderer has said anything.
pub(crate) fn page_zoom() -> f32 {
    let k = compose_rust::zoom().k;
    if k.is_finite() && k > 0.0 { k } else { 1.0 }
}

/// A length the renderer measured, as the page's CSS pixels.
///
/// Window and box sizes arrive in zoomed dp: the units widgets are drawn in, which already
/// carry the application's share of the zoom, `1.2^level`. The page is drawn at
/// `k = os * 1.2^level`, so zoomed dp over the system's text size `os` is the width the
/// page is laid out in and the one a `width` media query is answered with.
pub(crate) fn css_extent(dp: f32) -> f32 {
    let os = compose_rust::zoom().os;
    if os.is_finite() && os > 0.0 {
        dp / os
    } else {
        dp
    }
}

/// The part of the window's title band beside its buttons, in CSS pixels, which
/// `env(titlebar-area-*)` reads: from the window's leading edge past the buttons there to
/// the buttons at its trailing edge, and the band's height from the window's top. `None`
/// for a window with no band, where each `env()` takes its fallback as in a browser that
/// draws no title bar overlay.
pub(crate) fn titlebar_area(window: WindowSize) -> Option<Rect> {
    let safe = window.safe_area;
    if !(safe.top > 0.0 && window.width_dp > 0.0) {
        return None;
    }
    let leading = safe.leading.max(0.0);
    let trailing = safe.trailing.max(0.0);
    let width = (window.width_dp - leading - trailing).max(0.0);
    Some(Rect::new(
        css_extent(leading),
        0.0,
        css_extent(width),
        css_extent(safe.top),
    ))
}

/// One HTML and CSS application, running as the Host's runtime.
pub struct HtmlRuntime {
    /// The name the page's widgets are registered under.
    owner: u32,
    dom: HtmlDom,
    bridge: PlanBridge,
    batch: Batch,
    /// The text each field last delivered to the page.
    typed: HashMap<NodeId, String>,
    /// Fields whose text changed since they last delivered `change`.
    edited: HashSet<NodeId>,
    /// The field that has the focus, as far as the page has been told.
    focused: Option<NodeId>,
    /// The scheme the configuration names, for an application whose theme follows the
    /// system, until the renderer has reported the system's.
    configured_scheme: ColorScheme,
    /// The theme and design system the text sizes were measured under. A change of either
    /// makes the renderer's earlier answers wrong.
    measured_under: Option<(
        compose_rust::schema::Theme,
        compose_rust::schema::DesignSystem,
    )>,
    /// The animation moments the renderer reported since the last frame, which the next
    /// frame delivers to the page before it renders.
    moments: Rc<RefCell<Vec<AnimationMoment>>>,
    /// Keeps the registration for animation events for as long as the runtime lives.
    _moments: AnimationEventSubscription,
    /// Keeps the registration for the renderer's zoom reports for as long as the runtime
    /// lives. A report is answered with a frame only when something reads it, and the page
    /// does: it is laid out in a viewport and a resolution that depend on it.
    _zoom: compose_rust::zoom::ZoomSubscription,
    /// Keeps the registration for the system's light or dark appearance, for the same
    /// reason: a page whose theme follows the system answers `prefers-color-scheme` with it.
    _scheme: compose_rust::theme::SystemColorSchemeSubscription,
    /// The `(layout_revision, viewport_w_bits, viewport_h_bits, zoom_bits)` last applied to
    /// `self.bridge`, so an unchanged frame skips `dom.plan` and `bridge.apply`.
    applied_revision: Option<(u64, u32, u32, u32)>,
    /// The registration under which the renderer reports the size of the box a
    /// whole-screen page is drawn in. `None` for a page that is one island in a tree, whose
    /// viewport the tree hands it.
    island_size: Option<compose_rust::window::NodeSizeSubscription>,
    /// The two reports the viewport is taken from, as they were last read, so the one that
    /// changed most recently is the one that counts.
    seen_island: WindowSize,
    seen_window: WindowSize,
    /// The size of the area the page is drawn in, in zoomed dp, from whichever report said
    /// so last. `None` until the renderer has reported anything.
    content_size: Option<(f32, f32)>,
    /// The page's scroll requests and what the renderer's scroll reports taught it.
    scroll: ScrollSync,
    /// Whether `env(titlebar-area-*)` follows the window's reports. It does for a
    /// whole-screen page whose configuration names no title bar area of its own.
    titlebar_follows_window: bool,
    /// Nodes currently composing with an IME.
    composing: HashSet<NodeId>,
    /// Text waiting to be delivered as a `paste` event to the focused element.
    pending_pastes: Rc<RefCell<Vec<(NodeId, String)>>>,
}

impl HtmlRuntime {
    pub fn new(app: fn() -> Element) -> Self {
        Self::with_config(app, HtmlConfig::default())
    }

    pub fn with_config(app: fn() -> Element, config: HtmlConfig) -> Self {
        // The page is the whole screen, so it names its own factor on a root of its own.
        let mut bridge = PlanBridge::new();
        bridge.wrap_in_island();
        // The renderer is asked to measure the island box, which fills the area the window
        // gives its content. That report comes with the first layout of the box, whatever
        // the window's size class is, so the page is laid out at its real size from the
        // first frame the renderer draws.
        let token = compose_rust::window::next_node_token();
        bridge.observe_island(token);
        let follows = config.titlebar_area.is_none();
        let mut runtime = Self::with_bridge(app, config, bridge);
        runtime.titlebar_follows_window = follows;
        runtime.island_size = Some(compose_rust::window::subscribe_node(
            token,
            std::sync::Arc::new(|| {}),
        ));
        runtime
    }

    /// A runtime whose page is one island inside a tree some other writer owns: its
    /// renderer node ids and handler ids start at the given bases, so they never collide
    /// with that writer's. It is driven with `render_into`, and the events the island's
    /// handlers name go to `Runtime::handle_event`.
    pub(crate) fn embedded(
        app: fn() -> Element,
        config: HtmlConfig,
        first_node: u32,
        first_handler: u64,
    ) -> Self {
        let mut bridge = PlanBridge::with_id_bases(first_node, first_handler);
        bridge.size_scroll_to_viewport();
        Self::with_bridge(app, config, bridge)
    }

    fn with_bridge(app: fn() -> Element, config: HtmlConfig, bridge: PlanBridge) -> Self {
        let configured_scheme = config.color_scheme;
        let moments: Rc<RefCell<Vec<AnimationMoment>>> = Rc::default();
        let sink = moments.clone();
        let subscription = on_animation_event(Rc::new(move |moment: AnimationMoment| {
            sink.borrow_mut().push(moment)
        }));
        let owner = crate::html_widget::new_owner();
        // The page's first render runs inside the constructor, and its widgets belong to it.
        let _owner = crate::html_widget::OwnerScope::enter(owner);
        Self {
            owner,
            moments,
            measured_under: None,
            _moments: subscription,
            _zoom: compose_rust::zoom::subscribe(std::sync::Arc::new(|| {})),
            _scheme: compose_rust::subscribe_system_color_scheme(std::sync::Arc::new(|| {})),
            focused: None,
            configured_scheme,
            dom: HtmlDom::with_config(app, config),
            bridge,
            batch: Batch::new(),
            typed: HashMap::new(),
            edited: HashSet::new(),
            applied_revision: None,
            island_size: None,
            seen_island: WindowSize::default(),
            seen_window: WindowSize::default(),
            content_size: None,
            scroll: ScrollSync::default(),
            titlebar_follows_window: false,
            composing: HashSet::new(),
            pending_pastes: Rc::default(),
        }
    }

    /// The document the application renders into.
    pub fn dom(&self) -> &HtmlDom {
        &self.dom
    }

    /// The viewport the page is laid out in, in CSS pixels: the area the window draws it in.
    ///
    /// Two reports say how large that is. The island box's own is exact: the box fills the
    /// window's content area, which starts below the window's title band when the window
    /// has one. The window's report measures the whole window, title band included, and
    /// carries the band's height as `safe_area.top`, which the renderer pads the page down
    /// by, unless the window draws its content under the title bar, where nothing pads the
    /// page and the band is not taken out of its height. The box is observed on every dp
    /// change, so once it has reported it is the only one that counts. The window's report,
    /// less the band, stands in only until then.
    fn viewport(&mut self) -> (f32, f32) {
        let window = compose_rust::window_size();
        let island = self
            .island_size
            .as_ref()
            .map(|subscription| compose_rust::window::node_size(subscription.token()))
            .unwrap_or_default();
        let island_changed = island != self.seen_island;
        let window_changed = window != self.seen_window;
        self.seen_island = island;
        self.seen_window = window;
        let island_known = island.width_dp > 0.0 && island.height_dp > 0.0;
        if island_known {
            if island_changed || self.content_size.is_none() {
                self.content_size = Some((island.width_dp, island.height_dp));
            }
        } else if window_changed && window.width_dp > 0.0 && window.height_dp > 0.0 {
            let band = window.safe_area.top.max(0.0);
            let height = if window.height_dp > band {
                window.height_dp - band
            } else {
                window.height_dp
            };
            self.content_size = Some((window.width_dp, height));
        }
        match self.content_size {
            Some((width, height)) => (css_extent(width), css_extent(height)),
            None => FALLBACK_VIEWPORT,
        }
    }

    /// Lays the document out at the window's size and writes what changed on screen.
    fn draw(&mut self) {
        let viewport = self.viewport();
        if self.titlebar_follows_window {
            self.dom
                .set_titlebar_area(titlebar_area(compose_rust::window_size()));
        }
        let _owner = crate::html_widget::OwnerScope::enter(self.owner);
        crate::html_widget::render_islands(self.owner, &mut self.batch);
        for place in crate::html_widget::refresh(self.owner, viewport.0) {
            self.dom.mark_node_style_dirty(place);
        }
        if let Some(plan) = self.plan_at(viewport) {
            self.bridge
                .apply(plan, viewport, &self.dom, &mut self.batch);
        }
        self.scroll
            .write(&mut self.dom, &self.bridge, &mut self.batch);
        {
            let dom = &self.dom;
            crate::html_widget::set_boxes(self.owner, &|node| {
                let rect = dom.display_list()?.get(node)?.rect;
                Some((rect.width, rect.height))
            });
        }
        let bridge = &self.bridge;
        crate::html_widget::attach(
            self.owner,
            &|node| bridge.node_id(PlanKey::Node(node)),
            &mut self.batch,
        );
    }

    /// Lays the document out in `viewport`, in dp, and returns what to draw.
    fn plan_at(&mut self, viewport: (f32, f32)) -> Option<crate::html::Plan> {
        // A media query that asks for the scheme is answered with the theme's, so a page
        // switched to dark with the rest of the application is restyled with it.
        let scheme = match compose_rust::theme::current_theme().color_scheme {
            ThemeScheme::Light => ColorScheme::Light,
            ThemeScheme::Dark => ColorScheme::Dark,
            ThemeScheme::FollowSystem => match compose_rust::system_color_scheme() {
                Some(compose_rust::SystemColorScheme::Light) => ColorScheme::Light,
                Some(compose_rust::SystemColorScheme::Dark) => ColorScheme::Dark,
                None => self.configured_scheme,
            },
        };
        self.dom.set_color_scheme(scheme);
        let under = (
            compose_rust::theme::current_theme(),
            compose_rust::design_system(),
        );
        if self.measured_under.replace(under) != Some(under) {
            self.dom.invalidate_text_measurements();
        }
        // The theme's colour roles, as `--dxc-*` custom properties, set before the styles
        // are resolved so a switch restyles the page in the frame that changed the theme.
        self.dom
            .set_root_variables(&crate::theme_variables::theme_variable_declarations(
                &under.0,
                under.1,
                match scheme {
                    ColorScheme::Dark => ThemeScheme::Dark,
                    _ => ThemeScheme::Light,
                },
            ));
        // The page is drawn at `k`, so its text is measured at `k` and the layout is told
        // the resolution it is drawn at: a CSS pixel is one dp of the island's own
        // density, which the renderer scales by `k`. The viewport is already in CSS pixels.
        let zoom = page_zoom();
        self.dom.set_zoom(zoom);
        self.bridge.set_zoom(zoom);
        self.dom.layout(viewport.0, viewport.1, zoom);
        // TODO(DarkPyonix/compose-rust#410): when the renderer exposes a focus request
        // mutation/property, forward `self.dom.focused()` changes from `HtmlDom` to the
        // renderer's focus manager here.
        let rev_key = (
            self.dom.layout_revision(),
            viewport.0.to_bits(),
            viewport.1.to_bits(),
            zoom.to_bits(),
        );
        if self.applied_revision == Some(rev_key) {
            return None;
        }
        self.applied_revision = Some(rev_key);
        self.dom.plan(&mut LiteralColours)
    }

    /// Runs the page's pending work when `run_dom` says so, lays it out in `viewport` and
    /// writes what changed into `batch`, which belongs to the tree the page is an island
    /// of. Returns the node the page hangs from the first time it is written, for the
    /// caller to insert, and `None` on every later call.
    pub(crate) fn render_into(
        &mut self,
        viewport: (f32, f32),
        run_dom: bool,
        batch: &mut Batch,
    ) -> Option<u32> {
        let _owner = crate::html_widget::OwnerScope::enter(self.owner);
        if run_dom {
            self.deliver_moments();
            self.deliver_pastes();
            self.dom.render();
        }
        let first = self.bridge.root_node().is_none();
        let _owner = crate::html_widget::OwnerScope::enter(self.owner);
        crate::html_widget::render_islands(self.owner, batch);
        for place in crate::html_widget::refresh(self.owner, viewport.0) {
            self.dom.mark_node_style_dirty(place);
        }
        if let Some(plan) = self.plan_at(viewport) {
            self.bridge.apply(plan, viewport, &self.dom, batch);
        }
        self.scroll.write(&mut self.dom, &self.bridge, batch);
        self.deliver_host_moments();
        let bridge = &self.bridge;
        crate::html_widget::attach(
            self.owner,
            &|node| bridge.node_id(PlanKey::Node(node)),
            batch,
        );
        if first { self.bridge.root_node() } else { None }
    }

    /// Whether `handler_id` was given out by this page.
    pub(crate) fn owns_handler(&self, handler_id: u64) -> bool {
        self.bridge.handler(handler_id).is_some()
    }

    /// Delivers the animation moments the renderer reported to the page, as the DOM events a
    /// browser sends for them: `animationstart`, `animationiteration` and `animationend`
    /// for a `@keyframes` animation, `transitionend` for a transition. The renderer reports
    /// only the moments the bridge asked for, which are the ones the page listens to.
    fn deliver_moments(&mut self) {
        let moments = std::mem::take(&mut *self.moments.borrow_mut());
        for moment in moments {
            let ended = moment.kind == AnimationEventKind::End;
            let Some(target) =
                self.bridge
                    .animation_target(moment.node_id, moment.animation_id, ended)
            else {
                continue;
            };
            let elapsed = moment.elapsed_ms / 1000.0;
            match (&target.name, moment.kind) {
                (Some(name), AnimationEventKind::Active) => {
                    self.dom
                        .animation_event(target.node, "animationstart", name, elapsed);
                }
                (Some(name), AnimationEventKind::Iteration) => {
                    self.dom
                        .animation_event(target.node, "animationiteration", name, elapsed);
                }
                (Some(name), AnimationEventKind::End) => {
                    self.dom
                        .animation_event(target.node, "animationend", name, elapsed);
                }
                (None, AnimationEventKind::Active) => {
                    let property = animate::css_name(target.property);
                    self.dom
                        .transition_moment(target.node, "transitionstart", property, elapsed);
                }
                (None, AnimationEventKind::End) => {
                    let property = animate::css_name(target.property);
                    self.dom.transition_event(target.node, property, elapsed);
                }
                _ => {}
            }
        }
    }

    /// Delivers the moments only the Host knows, which the last frame produced:
    /// `transitionrun` when it sent a transition, and `transitioncancel` or
    /// `animationcancel` when an animation that had not ended was replaced, cancelled or
    /// removed with its node. The page hears them in the order they happened, and a run
    /// always comes before the renderer's `transitionstart`, which only arrives later.
    fn deliver_host_moments(&mut self) {
        for moment in self.bridge.take_host_moments() {
            let property = animate::css_name(moment.property);
            let node = moment.node;
            let event = match (&moment.name, moment.kind) {
                (None, HostMomentKind::Run) => "transitionrun",
                (None, HostMomentKind::Cancel) => "transitioncancel",
                (Some(_), HostMomentKind::Cancel) => "animationcancel",
                (Some(_), HostMomentKind::Run) => continue,
            };
            // A removed node's handlers are gone with it. Dispatch would climb to the
            // elements around it, so the event is dropped unless the node still has its own.
            if moment.removed && !self.dom.listeners(node).contains(&event) {
                continue;
            }
            let _ = {
                match (&moment.name, moment.kind) {
                    (None, HostMomentKind::Run) => {
                        self.dom
                            .transition_moment(node, "transitionrun", property, moment.elapsed)
                    }
                    (None, HostMomentKind::Cancel) => self.dom.transition_moment(
                        node,
                        "transitioncancel",
                        property,
                        moment.elapsed,
                    ),
                    (Some(name), HostMomentKind::Cancel) => {
                        self.dom
                            .animation_event(node, "animationcancel", name, moment.elapsed)
                    }
                    (Some(_), HostMomentKind::Run) => None,
                }
            };
        }
    }

    /// The user did something in the field `node`, so it has the focus: `focus`, if the
    /// page has not been told yet, after `blur` for a field it still thinks has it.
    fn deliver_focus(&mut self, node: NodeId) {
        if self.focused == Some(node) {
            return;
        }
        if let Some(previous) = self.focused.take() {
            self.dom.blur(previous);
        }
        self.focused = Some(node);
        self.dom.focus(node);
    }

    /// The field `node` lost the focus: `change` if its text changed, then `blur`, in the
    /// order a browser sends them.
    fn deliver_blur(&mut self, node: NodeId) {
        if self.composing.remove(&node) {
            let data = self.typed.get(&node).cloned().unwrap_or_default();
            self.dom.composition_end(node, &data);
        }
        self.deliver_change(node);
        if self.focused == Some(node) {
            self.focused = None;
        }
        self.dom.blur(node);
    }

    /// The text of a field reached the page: `input`, once per distinct text.
    fn deliver_text(&mut self, node: NodeId, text: &str) {
        self.deliver_focus(node);
        self.bridge.user_typed(node, text);
        if self.typed.get(&node).is_some_and(|typed| typed == text) {
            return;
        }
        self.typed.insert(node, text.to_owned());
        self.edited.insert(node);
        if self.composing.contains(&node) {
            self.dom.composition_update(node, text);
        }
        self.dom.input(node, text);
    }

    /// The user is done with a field: `change`, if its text changed since the last one.
    fn deliver_change(&mut self, node: NodeId) {
        if !self.edited.remove(&node) {
            return;
        }
        let text = self.typed.get(&node).cloned().unwrap_or_default();
        self.dom.change(node, &text);
    }

    /// Delivers pending clipboard paste events to their target DOM elements.
    fn deliver_pastes(&mut self) {
        let pastes = self.pending_pastes.borrow_mut().split_off(0);
        for (node, text) in pastes {
            self.dom.paste(node, &text);
        }
    }
}

impl Runtime for HtmlRuntime {
    fn batch(&self) -> &Batch {
        &self.batch
    }

    fn batch_mut(&mut self) -> &mut Batch {
        &mut self.batch
    }

    /// The island box is the one node a whole-screen page asks to have measured, so its
    /// size reaches [`compose_rust::window::node_size`] and the page is laid out again.
    fn size_token(&self, node_id: u32) -> Option<u32> {
        let subscription = self.island_size.as_ref()?;
        (self.bridge.island_node() == Some(node_id)).then_some(subscription.token())
    }

    fn rebuild(&mut self) {
        self.draw();
        self.deliver_host_moments();
    }

    fn render(&mut self, _frame_time_nanos: Option<u64>) {
        let _owner = crate::html_widget::OwnerScope::enter(self.owner);
        self.deliver_moments();
        self.deliver_pastes();
        self.dom.render();
        self.draw();
        self.deliver_host_moments();
    }

    fn handle_event(&mut self, event: &HostEvent<'_>) -> Result<i64, ProtocolError> {
        // A widget answers for itself and does not consume a click, so the renderer offers
        // it to the clickable box around the widget, which arrives here as an ordinary
        // click on the page.
        if let Some(result) = crate::html_widget::route_event(event) {
            return result;
        }
        let Some(handler) = self.bridge.handler(event.handler_id) else {
            return Err(ProtocolError::InvalidValueKind(0));
        };
        if handler.node_id != event.node_id {
            return Err(ProtocolError::InvalidValueKind(0));
        }
        let node = handler.node;
        match (handler.event, &event.payload) {
            (BridgeEvent::Click, EventPayload::Clicked) => {
                self.dom.click_node(node);
            }
            (BridgeEvent::TextChange, EventPayload::TextChanged(text)) => {
                self.deliver_text(node, text);
            }
            // Enter commits the text, ends the edit and submits the form, in the order a
            // browser does them. That is all Enter does in a single-line field, so the key
            // is answered as used and goes no further up the renderer's tree, where the
            // boxes around the field would deliver its `keydown` a second time.
            (BridgeEvent::TextSubmit, EventPayload::TextSubmitted(text)) => {
                self.deliver_text(node, text);
                self.deliver_change(node);
                self.dom.submit(node);
                return Ok(1);
            }
            (BridgeEvent::FocusLost, EventPayload::FocusLost) => self.deliver_blur(node),
            (
                kind @ (BridgeEvent::KeyDown | BridgeEvent::FieldKeyDown),
                EventPayload::KeyDown {
                    key,
                    shift_key,
                    ctrl_key,
                    alt_key,
                    meta_key,
                    text,
                    ..
                },
            ) => {
                // TODO(DarkPyonix/compose-rust#100): map the full W3C `Key`, `Code`,
                // `Location`, `repeat`, and `is_composing` fields once `EventPayload::KeyDown`
                // and `EventPayload::KeyUp` carry them on the wire.
                // TODO(DarkPyonix/compose-rust#410): route renderer `FocusGained` wire
                // payloads to `self.deliver_focus(node)`.
                let (key, code) = match key {
                    WireKey::Enter => (Key::Enter, Code::Enter),
                    WireKey::Escape => (Key::Escape, Code::Escape),
                    WireKey::Tab => (Key::Tab, Code::Tab),
                    WireKey::Backspace => (Key::Backspace, Code::Backspace),
                    WireKey::Delete => (Key::Delete, Code::Delete),
                    WireKey::ArrowUp => (Key::ArrowUp, Code::ArrowUp),
                    WireKey::ArrowDown => (Key::ArrowDown, Code::ArrowDown),
                    WireKey::ArrowLeft => (Key::ArrowLeft, Code::ArrowLeft),
                    WireKey::ArrowRight => (Key::ArrowRight, Code::ArrowRight),
                    WireKey::Home => (Key::Home, Code::Home),
                    WireKey::End => (Key::End, Code::End),
                    WireKey::PageUp => (Key::PageUp, Code::PageUp),
                    WireKey::PageDown => (Key::PageDown, Code::PageDown),
                    WireKey::F1 => (Key::F1, Code::F1),
                    WireKey::F2 => (Key::F2, Code::F2),
                    WireKey::F3 => (Key::F3, Code::F3),
                    WireKey::F4 => (Key::F4, Code::F4),
                    WireKey::F5 => (Key::F5, Code::F5),
                    WireKey::F6 => (Key::F6, Code::F6),
                    WireKey::F7 => (Key::F7, Code::F7),
                    WireKey::F8 => (Key::F8, Code::F8),
                    WireKey::F9 => (Key::F9, Code::F9),
                    WireKey::F10 => (Key::F10, Code::F10),
                    WireKey::F11 => (Key::F11, Code::F11),
                    WireKey::F12 => (Key::F12, Code::F12),
                    WireKey::ContextMenu => (Key::ContextMenu, Code::ContextMenu),
                    // A key the wire enum does not name: its DOM `key` string travels in
                    // `text`, so a printable key reaches the page as that character.
                    WireKey::Other if !text.is_empty() => {
                        (Key::Character(text.to_string()), Code::Unidentified)
                    }
                    WireKey::Other => (Key::Unidentified, Code::Unidentified),
                };
                let mut modifiers = Modifiers::empty();
                modifiers.set(Modifiers::SHIFT, *shift_key);
                modifiers.set(Modifiers::CONTROL, *ctrl_key);
                modifiers.set(Modifiers::ALT, *alt_key);
                modifiers.set(Modifiers::META, *meta_key);
                let from_field = kind == BridgeEvent::FieldKeyDown;
                if from_field {
                    self.deliver_focus(node);
                }
                let prevented = self.dom.key_down(node, key, code, modifiers);
                if !prevented {
                    let is_mac = cfg!(target_os = "macos");
                    let cmd_ctrl = if is_mac {
                        *meta_key && !*ctrl_key
                    } else {
                        *ctrl_key && !*meta_key
                    };
                    if cmd_ctrl && !*alt_key {
                        let is_v = text.eq_ignore_ascii_case("v") || matches!(code, Code::KeyV);
                        let is_c = text.eq_ignore_ascii_case("c") || matches!(code, Code::KeyC);
                        let is_x = text.eq_ignore_ascii_case("x") || matches!(code, Code::KeyX);
                        if is_v {
                            let target = self.focused.unwrap_or(node);
                            let pending = self.pending_pastes.clone();
                            compose_rust::clipboard::read_text(move |result| {
                                if let Ok(Some(clip_text)) = result {
                                    pending.borrow_mut().push((target, clip_text));
                                }
                            });
                        } else if is_c {
                            let target = self.focused.unwrap_or(node);
                            self.dom.copy(target);
                        } else if is_x {
                            let target = self.focused.unwrap_or(node);
                            self.dom.cut(target);
                        }
                    }
                }
                self.deliver_pastes();
                // In a field, a key nobody prevented goes on to the field: Enter submits
                // or starts a line. From a box, the key has been offered to the element
                // and, by bubbling, to every element around it, so the boxes around this
                // one must not offer it again.
                return Ok(i64::from(prevented || !from_field));
            }
            (
                BridgeEvent::Pointer,
                EventPayload::Pointer {
                    kind,
                    pointer_id,
                    pointer_type,
                    x,
                    y,
                    buttons,
                    button,
                    modifiers,
                    click_count,
                    delta_x,
                    delta_y,
                    delta_mode,
                    ..
                },
            ) => {
                let input = PointerInput {
                    kind: match kind {
                        WirePointerKind::Move => PointerKind::Move,
                        WirePointerKind::Enter => PointerKind::Enter,
                        WirePointerKind::Leave => PointerKind::Leave,
                        WirePointerKind::Down => PointerKind::Down,
                        WirePointerKind::Up => PointerKind::Up,
                        WirePointerKind::Cancel => PointerKind::Cancel,
                        WirePointerKind::Wheel => PointerKind::Wheel,
                        WirePointerKind::ContextMenu => PointerKind::ContextMenu,
                    },
                    pointer_id: *pointer_id,
                    pointer_type: match pointer_type {
                        WirePointerType::Mouse => PointerType::Mouse,
                        WirePointerType::Touch => PointerType::Touch,
                        WirePointerType::Pen => PointerType::Pen,
                    },
                    x: *x,
                    y: *y,
                    buttons: *buttons,
                    button: *button,
                    modifiers: *modifiers,
                    click_count: *click_count,
                    delta_x: *delta_x,
                    delta_y: *delta_y,
                    delta_mode: match delta_mode {
                        WireWheelMode::Pixel => WheelMode::Pixel,
                        WireWheelMode::Line => WheelMode::Line,
                        WireWheelMode::Page => WheelMode::Page,
                    },
                };
                // A handler that prevents the default consumes the event, which keeps it
                // from the boxes around this one. The frame that restyles `:hover` and
                // `:active` is asked for by `poll_work`, as a handler's write is.
                return Ok(i64::from(self.dom.pointer_event(node, &input)));
            }
            (
                kind @ (BridgeEvent::VerticalScroll | BridgeEvent::HorizontalScroll),
                EventPayload::ScrollChanged {
                    x_px,
                    y_px,
                    max_x_px,
                    max_y_px,
                    viewport_width_px,
                    viewport_height_px,
                    ..
                },
            ) => {
                self.scroll.report(
                    &mut self.dom,
                    &self.bridge,
                    node,
                    kind == BridgeEvent::HorizontalScroll,
                    Report {
                        x: *x_px,
                        y: *y_px,
                        max_x: *max_x_px,
                        max_y: *max_y_px,
                        width: *viewport_width_px,
                        height: *viewport_height_px,
                    },
                );
            }
            (
                BridgeEvent::CompositionChange,
                EventPayload::CompositionChanged { composing },
            ) => {
                if *composing {
                    if self.composing.insert(node) {
                        let data = self.typed.get(&node).cloned().unwrap_or_default();
                        self.dom.composition_start(node, &data);
                    }
                } else if self.composing.remove(&node) {
                    let data = self.typed.get(&node).cloned().unwrap_or_default();
                    self.dom.composition_end(node, &data);
                }
            }
            (BridgeEvent::Toggle, EventPayload::ValueChanged(value)) => {
                self.dom.check(node, *value != 0.0);
            }
            (BridgeEvent::Choose, EventPayload::ValueChanged(value)) => {
                if *value < 0.0 {
                    return Err(ProtocolError::InvalidValueKind(0));
                }
                self.dom.select(node, *value as usize);
            }
            // A payload the handler was not given out for.
            _ => return Err(ProtocolError::InvalidValueKind(0)),
        }
        Ok(0)
    }

    fn poll_work(&mut self, context: &mut Context<'_>) -> Poll<()> {
        // A pointer that moved `:hover` or `:active` changed no signal, but the page has to
        // be restyled, so it asks for a frame as a handler's write does.
        let restyled = self.dom.take_pointer_restyle();
        // Text measured before the renderer could answer is measured again next frame.
        let relayout = self.dom.wants_relayout();
        let future = self.dom.virtual_dom_mut().wait_for_work();
        let mut future = pin!(future);
        let outer = future.as_mut().poll(context);
        // Every widget tree is polled whether or not the page has work, so each one has
        // registered the waker.
        if crate::html_widget::poll_islands(self.owner, context) {
            return Poll::Ready(());
        }
        if restyled {
            return Poll::Ready(());
        }
        if relayout {
            return Poll::Ready(());
        }
        if !self.pending_pastes.borrow().is_empty() {
            return Poll::Ready(());
        }
        outer
    }

    /// A message's action runs in the root scope, so it can write signals and spawn tasks
    /// the way an event handler can.
    fn run_in_context(&mut self, action: &mut dyn FnMut()) {
        self.dom.virtual_dom_mut().in_scope(ScopeId::ROOT, action);
    }
}

/// The runtime factory for one HTML and CSS application.
///
/// `config` is called for each runtime the Host makes: once at start, and again when the
/// renderer asks for the whole tree after losing it. A function rather than a value,
/// because a configuration holds the measurer and the image resolver, which belong to one
/// document.
pub fn runtime_for(
    app: fn() -> Element,
    config: fn() -> HtmlConfig,
) -> impl Fn() -> Box<dyn Runtime> + Send + Sync + 'static {
    move || Box::new(HtmlRuntime::with_config(app, config())) as Box<dyn Runtime>
}

/// Runs an HTML and CSS application in a window, with the default configuration. Does not
/// return while it is running. [`LaunchBuilder::with_html`](crate::LaunchBuilder::with_html)
/// takes a configuration, a theme or a window.
pub fn launch(app: fn() -> Element) {
    crate::LaunchBuilder::new()
        .with_html(HtmlConfig::default)
        .launch(app);
}
