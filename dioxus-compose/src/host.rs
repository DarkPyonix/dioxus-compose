//! A `VirtualDom` as a compose-rust runtime, and the launch path that registers one.
//!
//! compose-rust owns the boundary and every record that is not the tree. What it asks of an
//! authoring layer is a [`Runtime`]: build the tree, write what changed, run the handler an
//! event names. Here that is a `VirtualDom` diffing into a [`ComposeRenderer`], which is the
//! whole of what this crate adds to the Host.

use crate::html::HtmlConfig;
use crate::renderer::ComposeRenderer;
use compose_rust::code::{CodeChange, CodeHover, CodeRange, EditRejected, Position, SaveRequest};
use compose_rust::protocol::{HostEvent, ProtocolError};
use compose_rust::schema::{EventPayload, LoopMode, Theme, Window};
use compose_rust::{Batch, FileDrop, KeyEvent, PointerEvent, RangeRequest, Runtime};
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
        // Edits queued against an editor of a runtime that is going away name nothing in
        // this one.
        crate::hooks::reset_edits();
        crate::hooks::clear_scope_windows();
        Self {
            dom: VirtualDom::new(app),
            renderer: ComposeRenderer::new(),
        }
    }
}

impl DioxusRuntime {
    /// Writes the edits asked of code editor handles, after the tree, so an editor created
    /// in this same call already exists when its first edit arrives. A handle no editor
    /// carries names nothing, and its edit is dropped here rather than sent to a node the
    /// Renderer would have to report as unknown.
    fn flush_code_edits(&mut self) {
        let renderer = &mut self.renderer;
        crate::hooks::drain_edits(|request| renderer.apply_editor_request(request));
    }

    fn bind_window_scopes(&mut self) {
        let unresolved = crate::hooks::take_unresolved_scopes();
        if unresolved.is_empty() || !self.renderer.has_windows() {
            return;
        }
        let mut bound_any = false;
        for scope in unresolved {
            if let Some(window) = find_scope_window(&self.dom, &self.renderer, scope)
                && window != 0
            {
                crate::hooks::record_scope_window(scope, window);
                self.dom.mark_dirty(scope);
                bound_any = true;
            }
        }
        if bound_any {
            self.dom.render_immediate(&mut self.renderer);
            let _ = crate::hooks::take_unresolved_scopes();
        }
    }

    fn flush_window_commands(&mut self) {
        let renderer = &mut self.renderer;
        crate::hooks::drain_window_commands(|cmd| {
            renderer.apply_window_command(cmd);
        });
    }

    fn finalize_frame(&mut self) {
        crate::hooks::set_active_window(0);
        self.bind_window_scopes();
        self.flush_code_edits();
        self.flush_window_commands();
        crate::html_view::draw_islands(&mut self.renderer);
        self.renderer.check_windows();
        let _ = self.renderer.take_forgotten_windows();
    }
}

fn find_scope_window(dom: &VirtualDom, renderer: &ComposeRenderer, scope: ScopeId) -> Option<u32> {
    let vnode = dom.get_scope(scope)?.try_root_node()?;
    find_vnode_window(dom, renderer, vnode)
}

fn find_vnode_window(
    dom: &VirtualDom,
    renderer: &ComposeRenderer,
    vnode: &dioxus_core::VNode,
) -> Option<u32> {
    for root_idx in 0..vnode.template.roots.len() {
        if let Some(el) = vnode.mounted_root(root_idx, dom)
            && let Some(node_id) = renderer.node_id_of(el)
        {
            let win = renderer.window_of(node_id);
            if win != 0 {
                return Some(win);
            }
        }
        if let Some(dyn_id) = vnode.template.roots[root_idx].dynamic_id() {
            match &vnode.dynamic_nodes[dyn_id] {
                dioxus_core::DynamicNode::Component(comp) => {
                    if let Some(child_scope) = comp.mounted_scope_id(dyn_id, vnode, dom)
                        && let Some(win) = find_scope_window(dom, renderer, child_scope)
                    {
                        return Some(win);
                    }
                }
                dioxus_core::DynamicNode::Fragment(children) => {
                    for child in children {
                        if let Some(win) = find_vnode_window(dom, renderer, child) {
                            return Some(win);
                        }
                    }
                }
                dioxus_core::DynamicNode::Text(_) | dioxus_core::DynamicNode::Placeholder(_) => {
                    if let Some(el) = vnode.mounted_dynamic_node(dyn_id, dom)
                        && let Some(node_id) = renderer.node_id_of(el)
                    {
                        let win = renderer.window_of(node_id);
                        if win != 0 {
                            return Some(win);
                        }
                    }
                }
            }
        }
    }
    None
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
        self.finalize_frame();
    }

    fn render(&mut self, _frame_time_nanos: Option<u64>) {
        self.dom.render_immediate(&mut self.renderer);
        self.finalize_frame();
    }

    fn handle_event(&mut self, event: &HostEvent<'_>) -> Result<i64, ProtocolError> {
        if let Some(result) = crate::html_view::route_event(event) {
            return result;
        }
        deliver_event(&mut self.dom, &self.renderer, event)
    }

    fn size_token(&self, node_id: u32) -> Option<u32> {
        self.renderer.size_token(node_id)
    }

    fn is_window(&self, node_id: u32) -> bool {
        self.renderer.is_window(node_id)
    }

    fn poll_work(&mut self, context: &mut Context<'_>) -> Poll<()> {
        let future = self.dom.wait_for_work();
        let mut future = pin!(future);
        let outer = future.as_mut().poll(context);
        // Every island is polled whether or not the widget tree has work, so each one has
        // registered the waker.
        if crate::html_view::poll_islands(context) {
            return Poll::Ready(());
        }
        outer
    }

    /// A message's action runs in the root scope, so it can write signals and spawn tasks
    /// the way an event handler can.
    fn run_in_context(&mut self, action: &mut dyn FnMut()) {
        self.dom.in_scope(ScopeId::ROOT, action);
    }
}

/// Delivers a renderer event to the Dioxus handler it names, in the tree `renderer` wrote.
///
/// Shared by the runtime that owns a widget tree and by a widget tree placed inside an HTML
/// page, which keeps its own `VirtualDom` and `ComposeRenderer`.
pub(crate) fn deliver_event(
    dom: &mut VirtualDom,
    renderer: &ComposeRenderer,
    event: &HostEvent<'_>,
) -> Result<i64, ProtocolError> {
    let Some((element, node_id, name)) = renderer.handler(event.handler_id) else {
        return Err(ProtocolError::InvalidValueKind(0));
    };
    if event.node_id != node_id {
        return Err(ProtocolError::InvalidValueKind(0));
    }
    // A pointer handler takes pointer payloads and nothing else, and no other handler
    // takes one: the listener downcasts what it is given, so a payload of the wrong shape
    // is refused here instead of reaching it.
    let pointer_handler = matches!(name, "pointer" | "onpointer");
    if pointer_handler != matches!(event.payload, EventPayload::Pointer { .. }) {
        return Err(ProtocolError::InvalidValueKind(0));
    }
    let mut key_event = None;
    let mut pointer_event = None;
    // A press on a link run arrives as a click on the handler the run was given, and
    // reaches the `Text`'s link listener with the value the application named it by.
    let link = renderer.link_value(event.handler_id);
    let event_data = match event.payload {
        EventPayload::Clicked if link.is_some() => {
            Event::new(Rc::new(link.unwrap_or_default()), false).into_any()
        }
        EventPayload::Clicked | EventPayload::FocusLost | EventPayload::FocusGained => {
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
            ..
        }
        | EventPayload::KeyUp {
            key,
            shift_key,
            ctrl_key,
            alt_key,
            meta_key,
            ..
        } => {
            let value = KeyEvent::new(key, shift_key, ctrl_key, alt_key, meta_key);
            key_event = Some(value.clone());
            Event::new(Rc::new(value), true).into_any()
        }
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
            timestamp_nanos,
        } => {
            let value = PointerEvent::new(
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
                timestamp_nanos,
            );
            pointer_event = Some(value.clone());
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
        EventPayload::CodeChanged {
            version,
            start_line,
            start_column,
            end_line,
            end_column,
            text,
        } => Event::new(
            Rc::new(CodeChange {
                version,
                range: CodeRange::of(start_line, start_column, end_line, end_column),
                text: text.to_owned(),
            }),
            true,
        )
        .into_any(),
        EventPayload::CodeEditRejected {
            request_id,
            base_version,
            current_version,
            start_line,
            start_column,
            end_line,
            end_column,
        } => Event::new(
            Rc::new(EditRejected {
                request_id,
                base_version,
                current_version,
                range: CodeRange::of(start_line, start_column, end_line, end_column),
            }),
            true,
        )
        .into_any(),
        EventPayload::CodeHovered {
            decoration,
            line,
            column,
            phase,
        } => Event::new(
            Rc::new(CodeHover {
                decoration: (decoration != 0).then_some(decoration),
                position: Position::new(line, column),
                phase,
            }),
            true,
        )
        .into_any(),
        EventPayload::CodeSaveRequested { version } => {
            Event::new(Rc::new(SaveRequest { version }), true).into_any()
        }
        EventPayload::DecorationActivated { decoration } => {
            Event::new(Rc::new(decoration), true).into_any()
        }
        EventPayload::SelectionChanged {
            version,
            anchor_line,
            anchor_column,
            active_line,
            active_column,
        } => Event::new(
            Rc::new(compose_rust::code::SelectionChange {
                version,
                anchor: Position::new(anchor_line, anchor_column),
                active: Position::new(active_line, active_column),
            }),
            true,
        )
        .into_any(),
        EventPayload::VisibleRangeChanged {
            version,
            first_line,
            last_line,
        } => Event::new(
            Rc::new(compose_rust::code::VisibleRange {
                version,
                first_line,
                last_line,
            }),
            true,
        )
        .into_any(),
        EventPayload::EditorFocusChanged { focused } => {
            Event::new(Rc::new(focused), true).into_any()
        }
        EventPayload::FoldChanged {
            start_line,
            collapsed,
        } => Event::new(
            Rc::new(compose_rust::code::FoldChange {
                start_line,
                collapsed,
            }),
            true,
        )
        .into_any(),
        EventPayload::CodeMetrics { line_height_px } => {
            Event::new(Rc::new(line_height_px), true).into_any()
        }
        EventPayload::CodeScrolled { offset_px } => Event::new(Rc::new(offset_px), true).into_any(),
        EventPayload::CompositionChanged { composing } => {
            Event::new(Rc::new(composing), true).into_any()
        }
        EventPayload::WindowCloseRequested
        | EventPayload::WindowUnavailable
        | EventPayload::WindowClosed => Event::new(Rc::new(()), true).into_any(),
        EventPayload::WindowFocusChanged { focused } => {
            Event::new(Rc::new(focused), true).into_any()
        }
        EventPayload::WindowMoved {
            x,
            y,
            width,
            height,
        } => Event::new(
            Rc::new(compose_rust::windows::WindowFrame {
                x,
                y,
                width,
                height,
            }),
            true,
        )
        .into_any(),
        EventPayload::WindowDrag {
            phase,
            screen_x,
            screen_y,
            over_window,
        } => {
            let phase = match phase {
                compose_rust::protocol::WINDOW_DRAG_MOVE => {
                    compose_rust::windows::WindowDragPhase::Move
                }
                compose_rust::protocol::WINDOW_DRAG_END => {
                    compose_rust::windows::WindowDragPhase::End
                }
                compose_rust::protocol::WINDOW_DRAG_CANCEL => {
                    compose_rust::windows::WindowDragPhase::Cancel
                }
                _ => return Err(ProtocolError::InvalidValueKind(0)),
            };
            Event::new(
                Rc::new(compose_rust::windows::WindowDragEvent {
                    phase,
                    screen_x,
                    screen_y,
                    over_window,
                }),
                true,
            )
            .into_any()
        }
        EventPayload::WindowDragOver { kind, window, x, y } => {
            let kind = match kind {
                compose_rust::protocol::WINDOW_DRAG_OVER_ENTER => {
                    compose_rust::windows::WindowDragOverKind::Enter
                }
                compose_rust::protocol::WINDOW_DRAG_OVER_MOVE => {
                    compose_rust::windows::WindowDragOverKind::Move
                }
                compose_rust::protocol::WINDOW_DRAG_OVER_LEAVE => {
                    compose_rust::windows::WindowDragOverKind::Leave
                }
                compose_rust::protocol::WINDOW_DRAG_OVER_DROP => {
                    compose_rust::windows::WindowDragOverKind::Drop
                }
                _ => return Err(ProtocolError::InvalidValueKind(0)),
            };
            Event::new(
                Rc::new(compose_rust::windows::WindowDragOver { kind, window, x, y }),
                true,
            )
            .into_any()
        }
        // The Host answers these itself and never hands them to a runtime: a zoom report
        // updates the state `use_zoom` and every island read, and wakes them, before any
        // runtime is asked. One that arrives here was addressed to a handler, which none
        // of them can be.
        EventPayload::ProtocolError { .. }
        | EventPayload::WindowSizeChanged { .. }
        | EventPayload::DesignSystemResolved(_)
        | EventPayload::Resync
        | EventPayload::LifecycleStart
        | EventPayload::LifecycleStop
        | EventPayload::NotificationActivated { .. }
        | EventPayload::NotificationPermissionChanged(_)
        | EventPayload::AnimationEvent { .. }
        | EventPayload::ReducedMotionChanged(_)
        | EventPayload::ZoomChanged { .. }
        | EventPayload::WindowRequested { .. }
        | EventPayload::ApplicationReopen
        | EventPayload::FileDialogResult { .. }
        | EventPayload::ClipboardText { .. }
        | EventPayload::IoAnswer { .. }
        | EventPayload::FileHandles { .. }
        | EventPayload::StorageLocation(_)
        | EventPayload::ScrollChanged { .. } => {
            return Err(ProtocolError::InvalidValueKind(0));
        }
    };
    let _window_guard = crate::hooks::ActiveWindowGuard::enter(renderer.window_of(node_id));
    dom.runtime().handle_event(name, event_data, element);
    Ok(i64::from(
        key_event.is_some_and(|event| event.consumed())
            || pointer_event.is_some_and(|event| event.consumed()),
    ))
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

    /// Sets the window record the Host emits on rebuild.
    pub fn with_window(mut self, window: Window) -> Self {
        self.0.set_window(window);
        self
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

    /// Runs the application in multi-window mode, where every direct child of the root is
    /// a `window` element.
    pub fn windows(self) -> Self {
        Self {
            core: self.core.windows(),
            ..self
        }
    }

    /// Sets what the Renderer loop does when the last open window closes.
    pub fn on_last_window_closed(self, policy: compose_rust::windows::LastWindowClosed) -> Self {
        Self {
            core: self.core.on_last_window_closed(policy),
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
