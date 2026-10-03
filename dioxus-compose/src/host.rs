//! A `VirtualDom` as a compose-rust runtime, and the launch path that registers one.
//!
//! compose-rust owns the boundary and every record that is not the tree. What it asks of an
//! authoring layer is a [`Runtime`]: build the tree, write what changed, run the handler an
//! event names. Here that is a `VirtualDom` diffing into a [`ComposeRenderer`], which is the
//! whole of what this crate adds to the Host.

use crate::html::HtmlConfig;
use crate::renderer::ComposeRenderer;
use compose_rust::protocol::{HostEvent, ProtocolError};
use compose_rust::schema::{EventPayload, LoopMode, Theme, Window};
use compose_rust::{Batch, FileDrop, KeyEvent, RangeRequest, Runtime};
use dioxus_core::{Element, Event, ScopeId, VirtualDom};
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll};

/// One Dioxus application, running as the Host's runtime.
pub struct DioxusRuntime {
    dom: VirtualDom,
    renderer: ComposeRenderer,
}

impl DioxusRuntime {
    pub fn new(app: fn() -> Element) -> Self {
        Self {
            dom: VirtualDom::new(app),
            renderer: ComposeRenderer::new(),
        }
    }
}

impl Runtime for DioxusRuntime {
    fn batch(&self) -> &Batch {
        self.renderer.batch()
    }

    fn batch_mut(&mut self) -> &mut Batch {
        self.renderer.batch_mut()
    }

    fn rebuild(&mut self) {
        self.dom.rebuild(&mut self.renderer);
    }

    fn render(&mut self) {
        self.dom.render_immediate(&mut self.renderer);
    }

    fn handle_event(&mut self, event: &HostEvent<'_>) -> Result<i64, ProtocolError> {
        let Some((element, node_id, name)) = self.renderer.handler(event.handler_id) else {
            return Err(ProtocolError::InvalidValueKind(0));
        };
        if event.node_id != node_id {
            return Err(ProtocolError::InvalidValueKind(0));
        }
        let mut key_event = None;
        let event_data = match event.payload {
            EventPayload::Clicked | EventPayload::FocusLost => {
                Event::new(Rc::new(()), true).into_any()
            }
            EventPayload::TextChanged(text) | EventPayload::TextSubmitted(text) => {
                Event::new(Rc::new(text.to_owned()), true).into_any()
            }
            EventPayload::KeyDown {
                key,
                shift_key,
                ctrl_key,
                alt_key,
                meta_key,
            } => {
                let value = KeyEvent::new(key, shift_key, ctrl_key, alt_key, meta_key);
                key_event = Some(value.clone());
                Event::new(Rc::new(value), true).into_any()
            }
            EventPayload::FilesEntered => Event::new(Rc::new(()), true).into_any(),
            EventPayload::FilesDropped(paths) => {
                Event::new(Rc::new(FileDrop::new(paths)), true).into_any()
            }
            EventPayload::RangeRequested { start, count } => {
                Event::new(Rc::new(RangeRequest::new(start, count)), true).into_any()
            }
            EventPayload::ValueChanged(value) => Event::new(Rc::new(value), true).into_any(),
            // The Host answers these itself and never hands them to a runtime. One that
            // arrives here was addressed to a handler, which none of them can be.
            EventPayload::ProtocolError { .. }
            | EventPayload::WindowSizeChanged { .. }
            | EventPayload::DesignSystemResolved(_)
            | EventPayload::Resync
            | EventPayload::LifecycleStart
            | EventPayload::LifecycleStop
            | EventPayload::NotificationActivated { .. }
            | EventPayload::NotificationPermissionChanged(_) => {
                return Err(ProtocolError::InvalidValueKind(0));
            }
        };
        self.dom.runtime().handle_event(name, event_data, element);
        Ok(i64::from(key_event.is_some_and(|event| event.consumed())))
    }

    fn size_token(&self, node_id: u32) -> Option<u32> {
        self.renderer.size_token(node_id)
    }

    fn poll_work(&mut self, context: &mut Context<'_>) -> Poll<()> {
        let future = self.dom.wait_for_work();
        let mut future = pin!(future);
        future.as_mut().poll(context)
    }

    /// A message's action runs in the root scope, so it can write signals and spawn tasks
    /// the way an event handler can.
    fn run_in_context(&mut self, action: &mut dyn FnMut()) {
        self.dom.in_scope(ScopeId::ROOT, action);
    }
}

/// The runtime factory for one Dioxus application.
///
/// A plain function pointer is all it captures, so the factory is `Send + Sync` and costs
/// nothing to share with the Renderer's UI thread.
pub fn runtime_for(app: fn() -> Element) -> impl Fn() -> Box<dyn Runtime> + Send + Sync + 'static {
    move || Box::new(DioxusRuntime::new(app)) as Box<dyn Runtime>
}

/// A compose-rust [`compose_rust::Host`] driving one Dioxus application.
///
/// Everything the Host does is compose-rust's and reached through `Deref`: `rebuild`,
/// `dispatch`, `render_frame`, assets, streaming. What this adds is the constructor that
/// turns an application's root component into the runtime the Host drives.
pub struct Host(compose_rust::Host);

impl Host {
    pub fn new(app: fn() -> Element) -> Self {
        Self(compose_rust::Host::new(runtime_for(app)))
    }

    /// A Host for an application written with HTML elements and CSS. `config` is called
    /// for each runtime the Host makes; see [`crate::html::runtime_for`].
    pub fn html(app: fn() -> Element, config: fn() -> HtmlConfig) -> Self {
        Self(compose_rust::Host::new(crate::html::runtime_for(
            app, config,
        )))
    }

    /// The theme the application chose. Choosing nothing follows the host platform, with
    /// Material 3 where the platform has no look of its own.
    pub fn with_theme(app: fn() -> Element, theme: Theme) -> Self {
        Self(compose_rust::Host::with_theme(runtime_for(app), theme))
    }

    pub fn into_inner(self) -> compose_rust::Host {
        self.0
    }
}

impl std::ops::Deref for Host {
    type Target = compose_rust::Host;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Host {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// How a Dioxus application starts: compose-rust's launch, given a root component.
///
/// The component is written with the Compose widgets unless [`LaunchBuilder::with_html`]
/// says it is written with HTML elements and CSS.
#[derive(Clone, Copy, Debug, Default)]
pub struct LaunchBuilder {
    core: compose_rust::LaunchBuilder,
    html: Option<fn() -> HtmlConfig>,
}

impl LaunchBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(self, mode: LoopMode) -> Self {
        Self {
            core: self.core.with_mode(mode),
            ..self
        }
    }

    /// `Theme::unified` for one design system everywhere, `Theme::adaptive` to follow the
    /// host platform. Not calling this follows the host platform, falling back to
    /// Material 3.
    pub fn with_theme(self, theme: Theme) -> Self {
        Self {
            core: self.core.with_theme(theme),
            ..self
        }
    }

    /// What the application asks of its own window: its size, and whether it wears the
    /// platform's title bar or has content run into it.
    pub fn with_window(self, window: Window) -> Self {
        Self {
            core: self.core.with_window(window),
            ..self
        }
    }

    /// The root component is written with HTML elements and CSS, laid out with the
    /// configuration `config` returns. It is called for each runtime the Host makes, so a
    /// renderer that asks for the whole tree again gets a fresh document.
    pub fn with_html(self, config: fn() -> HtmlConfig) -> Self {
        Self {
            html: Some(config),
            ..self
        }
    }

    /// Runs the application. Does not return while it is running, and ends the process
    /// with a failing status if the renderer loop could not run at all.
    pub fn launch(self, app: fn() -> Element) {
        self.core.launch_runtime(self.runtime(app));
    }

    /// [`LaunchBuilder::launch`] without the exit: the status the renderer loop ended
    /// with, handed back for a caller that has its own idea of what to do with it.
    pub fn try_launch(self, app: fn() -> Element) -> i32 {
        self.core.try_launch_runtime(self.runtime(app))
    }

    /// The compose-rust builder underneath, for an entry point that takes one.
    pub fn into_core(self) -> compose_rust::LaunchBuilder {
        self.core
    }

    /// The runtime factory for `app`, written the way this builder says it is.
    pub fn runtime(self, app: fn() -> Element) -> Box<dyn Fn() -> Box<dyn Runtime> + Send + Sync> {
        match self.html {
            Some(config) => Box::new(crate::html::runtime_for(app, config)),
            None => Box::new(runtime_for(app)),
        }
    }
}

pub fn launch(app: fn() -> Element) {
    LaunchBuilder::new().launch(app);
}

/// The browser's start, given a root component. `web_main!` exports it under the name the
/// page calls.
#[cfg(target_family = "wasm")]
#[doc(hidden)]
pub fn web_start(builder: LaunchBuilder, app: fn() -> Element) -> u32 {
    compose_rust::__web_start(builder.into_core(), builder.runtime(app))
}
