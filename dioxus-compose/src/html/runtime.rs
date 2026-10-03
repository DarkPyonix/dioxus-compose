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
//! Text is measured by the configured [`TextMeasurer`](crate::html::TextMeasurer), Parley by
//! default. compose-rust has no call that asks the renderer to measure a string, so the
//! page is laid out with Parley's answers and the renderer draws each run at the width
//! that came out of them.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll};

use compose_rust::protocol::{HostEvent, ProtocolError};
use compose_rust::schema::{EventPayload, Key as WireKey};
use compose_rust::{Batch, Runtime};
use dioxus_core::{Element, ScopeId};
use dioxus_html::{Code, Key, Modifiers};

use crate::dom::{HtmlConfig, HtmlDom};
use crate::html::NodeId;
use crate::paint::bridge::{BridgeEvent, PlanBridge};
use crate::paint::plan::LiteralColours;

/// The viewport a page is laid out in before the renderer has said how large the window
/// is. The renderer reports the window's size as soon as it has one, and the page is laid
/// out again at that size.
const FALLBACK_VIEWPORT: (f32, f32) = (800.0, 600.0);

/// One HTML and CSS application, running as the Host's runtime.
pub struct HtmlRuntime {
    dom: HtmlDom,
    bridge: PlanBridge,
    batch: Batch,
    /// The text each field last delivered to the page.
    typed: HashMap<NodeId, String>,
    /// Fields whose text changed since they last delivered `change`.
    edited: HashSet<NodeId>,
    /// The field that has the focus, as far as the page has been told.
    focused: Option<NodeId>,
}

impl HtmlRuntime {
    pub fn new(app: fn() -> Element) -> Self {
        Self::with_config(app, HtmlConfig::default())
    }

    pub fn with_config(app: fn() -> Element, config: HtmlConfig) -> Self {
        Self {
            focused: None,
            dom: HtmlDom::with_config(app, config),
            bridge: PlanBridge::new(),
            batch: Batch::new(),
            typed: HashMap::new(),
            edited: HashSet::new(),
        }
    }

    /// The document the application renders into.
    pub fn dom(&self) -> &HtmlDom {
        &self.dom
    }

    /// Lays the document out at the window's size and writes what changed on screen.
    fn draw(&mut self) {
        let size = compose_rust::window_size();
        let viewport = if size.width_dp > 0.0 && size.height_dp > 0.0 {
            (size.width_dp, size.height_dp)
        } else {
            FALLBACK_VIEWPORT
        };
        // One CSS pixel is one dp, so the page is laid out at a scale of one. The renderer
        // turns dp into device pixels.
        self.dom.layout(viewport.0, viewport.1, 1.0);
        let Some(plan) = self.dom.plan(&mut LiteralColours) else {
            return;
        };
        self.bridge
            .apply(plan, viewport, &self.dom, &mut self.batch);
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
}

impl Runtime for HtmlRuntime {
    fn batch(&self) -> &Batch {
        &self.batch
    }

    fn batch_mut(&mut self) -> &mut Batch {
        &mut self.batch
    }

    fn rebuild(&mut self) {
        self.draw();
    }

    fn render(&mut self) {
        self.dom.render();
        self.draw();
    }

    fn handle_event(&mut self, event: &HostEvent<'_>) -> Result<i64, ProtocolError> {
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
                },
            ) => {
                let (key, code) = match key {
                    WireKey::Enter => (Key::Enter, Code::Enter),
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
                // In a field, a key nobody prevented goes on to the field: Enter submits
                // or starts a line. From a box, the key has been offered to the element
                // and, by bubbling, to every element around it, so the boxes around this
                // one must not offer it again.
                return Ok(i64::from(prevented || !from_field));
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
        let future = self.dom.virtual_dom_mut().wait_for_work();
        let mut future = pin!(future);
        future.as_mut().poll(context)
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
