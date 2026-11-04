//! The hooks a component reads the Renderer's reports through.
//!
//! The values are compose-rust's: the window's size class, a node's measured size, the
//! design system the theme resolved to, the notification permission. Each is recorded by
//! the Host when the Renderer reports it, and each keeps a list of readers to wake. A hook
//! here is one such reader, registered for as long as the component holds its hook state,
//! woken by marking the component dirty.

use compose_rust::notification::NotificationActivation;
use compose_rust::schema::{DesignSystem, NotificationPermission, Theme};
use compose_rust::theme::ThemeHandle;
use compose_rust::window::{NodeSize, WindowSize};
use compose_rust::windows::WindowRequest;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    /// The window node currently enclosing an event handler or a component render.
    static ACTIVE_WINDOW: Cell<u32> = const { Cell::new(0) };
    /// For each Dioxus scope that has rendered inside a window, the `Window` node id it
    /// belongs to. Allows re-renders of a child component (`render_immediate`) to know
    /// which window encloses them even when the `Window` template itself is not on the
    /// renderer stack.
    static SCOPE_WINDOWS: RefCell<HashMap<dioxus_core::ScopeId, u32>> =
        RefCell::new(HashMap::new());
    /// Scopes that ran during `SuspenseBoundary::create`'s dry-run pass (`to: None`)
    /// before any `Window` node was created. Re-rendered right after `VirtualDom::rebuild`
    /// once the real `Window` nodes and mounts are in place.
    static UNRESOLVED_SCOPES: RefCell<Vec<dioxus_core::ScopeId>> =
        const { RefCell::new(Vec::new()) };
    /// Commands requested against a specific `Window` node (`focus`, `set_theme`,
    /// `set_zoom_level`) waiting to be written into the current batch.
    static WINDOW_COMMANDS: RefCell<Vec<PendingWindowCommand>> =
        const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PendingWindowCommand {
    Focus(u32),
    Theme(u32, Theme),
    ZoomLevel(u32, i8),
}

pub(crate) fn queue_window_command(cmd: PendingWindowCommand) {
    WINDOW_COMMANDS.with_borrow_mut(|cmds| cmds.push(cmd));
}

pub(crate) fn drain_window_commands(mut emit: impl FnMut(PendingWindowCommand)) {
    let mut taken = WINDOW_COMMANDS.with_borrow_mut(std::mem::take);
    if taken.is_empty() {
        return;
    }
    for cmd in taken.drain(..) {
        emit(cmd);
    }
    WINDOW_COMMANDS.with_borrow_mut(|cmds| {
        if cmds.is_empty() {
            *cmds = taken;
        }
    });
}

fn pending_window_zoom_level(window: u32) -> Option<i8> {
    WINDOW_COMMANDS.with_borrow(|cmds| {
        cmds.iter().rev().find_map(|cmd| match *cmd {
            PendingWindowCommand::ZoomLevel(id, level) if id == window => Some(level),
            _ => None,
        })
    })
}

/// Guard that restores the previous active window when dropped.
pub(crate) struct ActiveWindowGuard {
    previous: u32,
}

impl ActiveWindowGuard {
    pub(crate) fn enter(window: u32) -> Self {
        let previous = ACTIVE_WINDOW.replace(window);
        Self { previous }
    }
}

impl Drop for ActiveWindowGuard {
    fn drop(&mut self) {
        ACTIVE_WINDOW.set(self.previous);
    }
}

pub(crate) fn set_active_window(window: u32) {
    ACTIVE_WINDOW.set(window);
}

pub(crate) fn clear_scope_windows() {
    ACTIVE_WINDOW.set(0);
    SCOPE_WINDOWS.with_borrow_mut(HashMap::clear);
    UNRESOLVED_SCOPES.with_borrow_mut(Vec::clear);
    WINDOW_COMMANDS.with_borrow_mut(Vec::clear);
}

pub(crate) fn take_unresolved_scopes() -> Vec<dioxus_core::ScopeId> {
    UNRESOLVED_SCOPES.with_borrow_mut(std::mem::take)
}

pub(crate) fn record_scope_window(scope: dioxus_core::ScopeId, window: u32) {
    if window != 0 {
        SCOPE_WINDOWS.with_borrow_mut(|map| {
            map.insert(scope, window);
        });
    }
}

pub(crate) fn remove_window_scopes(window: u32) {
    SCOPE_WINDOWS.with_borrow_mut(|map| map.retain(|_, w| *w != window));
    WINDOW_COMMANDS.with_borrow_mut(|cmds| {
        cmds.retain(|cmd| match *cmd {
            PendingWindowCommand::Focus(id)
            | PendingWindowCommand::Theme(id, _)
            | PendingWindowCommand::ZoomLevel(id, _) => id != window,
        });
    });
}

/// Resolves the `Window` node id that encloses the current component or event handler.
pub(crate) fn resolve_current_window() -> u32 {
    if let Some(scope) = dioxus_core::Runtime::try_current()
        .as_ref()
        .and_then(|rt| rt.try_current_scope_id())
    {
        if let Some(known) = SCOPE_WINDOWS.with_borrow(|map| map.get(&scope).copied()) {
            return known;
        }
        let parent_id = dioxus_core::parent_scope();
        if let Some(parent_id) = parent_id
            && let Some(known) = SCOPE_WINDOWS.with_borrow(|map| map.get(&parent_id).copied())
        {
            SCOPE_WINDOWS.with_borrow_mut(|map| {
                map.insert(scope, known);
            });
            return known;
        }
        let active = ACTIVE_WINDOW.get();
        if active != 0 {
            SCOPE_WINDOWS.with_borrow_mut(|map| {
                map.insert(scope, active);
            });
            return active;
        }
        UNRESOLVED_SCOPES.with_borrow_mut(|list| {
            if !list.contains(&scope) {
                list.push(scope);
            }
        });
        return 0;
    }
    ACTIVE_WINDOW.get()
}

/// A handle to one `Window` node, returned by [`current_window`].
///
/// In a single-window application its id is `0`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct WindowHandle {
    node_id: u32,
}

impl WindowHandle {
    /// Creates a handle for the given `Window` node id (`0` for the single-window root).
    pub const fn from_raw(node_id: u32) -> Self {
        Self { node_id }
    }

    /// The `Window` node's id, or `0` in a single-window application.
    pub const fn id(self) -> u32 {
        self.node_id
    }

    /// Asks the Renderer to bring this window to the front and make it the key window.
    ///
    /// Emits one `FocusWindow` record in the batch the current call produces.
    pub fn focus(self) {
        if self.node_id != 0 {
            queue_window_command(PendingWindowCommand::Focus(self.node_id));
        }
    }

    /// Overrides the application theme for this window alone (`SetWindowTheme`).
    ///
    /// On the single-window root (`id == 0`), sets the application theme instead.
    pub fn set_theme(self, theme: Theme) {
        if self.node_id == 0 {
            ThemeHandle::default().set(theme);
        } else {
            queue_window_command(PendingWindowCommand::Theme(self.node_id, theme));
        }
    }

    /// Asks the Renderer to draw this window at `level` (`-8..=8`).
    pub fn set_zoom_level(self, level: i8) {
        ZoomHandle::for_window(self.node_id).set_level(level);
    }

    /// This window's size class and dimensions as last reported by the Renderer.
    pub fn size(self) -> WindowSize {
        if self.node_id == 0 {
            compose_rust::window::window_size()
        } else {
            compose_rust::window::window_size_of(self.node_id)
        }
    }

    /// This window's zoom factor and level as last reported by the Renderer.
    pub fn zoom(self) -> compose_rust::Zoom {
        if self.node_id == 0 {
            compose_rust::zoom::zoom()
        } else {
            compose_rust::zoom::zoom_of(self.node_id)
        }
    }
}

/// The window the calling component or event handler belongs to.
///
/// Inside a `window { ... }` element this returns that window's [`WindowHandle`]; in a
/// single-window application it returns the root handle (`id() == 0`).
pub fn current_window() -> WindowHandle {
    WindowHandle::from_raw(resolve_current_window())
}

/// Runs `handler` when the system or user asks for a new or restored window
/// (`WindowRequested`).
pub fn use_window_requested(handler: impl FnMut(WindowRequest) + 'static) {
    let callback = dioxus_hooks::use_callback(handler);
    dioxus_core::use_hook(|| {
        Rc::new(compose_rust::windows::on_window_request(Rc::new(
            move |request: WindowRequest| callback.call(request),
        )))
    });
}

/// Runs `handler` when the user reopens the application while it has no open windows
/// (such as clicking the Dock icon on macOS under `LastWindowClosed::KeepRunning`).
pub fn use_application_reopen(mut handler: impl FnMut() + 'static) {
    let callback = dioxus_hooks::use_callback(move |(): ()| handler());
    dioxus_core::use_hook(|| {
        Rc::new(compose_rust::windows::on_application_reopen_handler(
            Rc::new(move || callback.call(())),
        ))
    });
}

/// Reads the window's size class inside a component, and re-renders it when the class
/// changes.
///
/// ```ignore
/// let window = use_window_size();
/// rsx! {
///     if window.is_expanded() {
///         Row { Sidebar {} Content {} }
///     } else {
///         Content {}
///     }
/// }
/// ```
///
/// Only components that call this are woken, and only when the class actually changes.
/// Resizing inside one class re-renders nothing.
pub fn use_window_size() -> WindowSize {
    let window = resolve_current_window();
    let sub: Rc<RefCell<(u32, Option<EitherWindowSub>)>> =
        dioxus_core::use_hook(|| Rc::new(RefCell::new((u32::MAX, None))));
    let schedule = dioxus_core::schedule_update();
    {
        let mut state = sub.borrow_mut();
        if state.0 != window || state.1.is_none() {
            let new_sub = if window == 0 {
                EitherWindowSub::Single(Rc::new(compose_rust::window::subscribe(schedule)))
            } else {
                EitherWindowSub::Multi(Rc::new(compose_rust::window::subscribe_window(
                    window, schedule,
                )))
            };
            *state = (window, Some(new_sub));
        }
    }
    if window == 0 {
        compose_rust::window::window_size()
    } else {
        compose_rust::window::window_size_of(window)
    }
}

#[derive(Clone)]
enum EitherWindowSub {
    Single(#[allow(dead_code)] Rc<compose_rust::window::WindowSizeSubscription>),
    Multi(#[allow(dead_code)] Rc<compose_rust::window::WindowSubscription>),
}

/// A handle on one window's zoom (or the single-window root's when `window == 0`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ZoomHandle {
    window: u32,
}

impl ZoomHandle {
    /// A handle for the given `Window` node id (`0` for the single-window root).
    pub const fn for_window(window: u32) -> Self {
        Self { window }
    }

    /// What the Renderer last reported for this window.
    pub fn get(&self) -> compose_rust::Zoom {
        if self.window == 0 {
            compose_rust::zoom::zoom()
        } else {
            compose_rust::zoom::zoom_of(self.window)
        }
    }

    /// The zoom level (`-8..=8`) last reported for this window.
    pub fn level(&self) -> i8 {
        self.get().level
    }

    /// Sets the application zoom level for this window, kept inside its range.
    pub fn set_level(&self, level: i8) {
        let clamped = compose_rust::zoom::clamp_level(level);
        if self.window == 0 {
            compose_rust::ZoomHandle::default().set_level(clamped);
        } else {
            queue_window_command(PendingWindowCommand::ZoomLevel(self.window, clamped));
        }
    }

    /// One step larger, as a "Zoom In" menu item does.
    pub fn zoom_in(&self) {
        if self.window == 0 {
            compose_rust::ZoomHandle::default().zoom_in();
        } else {
            let cur = pending_window_zoom_level(self.window)
                .unwrap_or_else(|| compose_rust::zoom::zoom_of(self.window).level);
            self.set_level(cur.saturating_add(1));
        }
    }

    /// One step smaller.
    pub fn zoom_out(&self) {
        if self.window == 0 {
            compose_rust::ZoomHandle::default().zoom_out();
        } else {
            let cur = pending_window_zoom_level(self.window)
                .unwrap_or_else(|| compose_rust::zoom::zoom_of(self.window).level);
            self.set_level(cur.saturating_sub(1));
        }
    }

    /// Back to the system's own size.
    pub fn reset(&self) {
        if self.window == 0 {
            compose_rust::ZoomHandle::default().reset();
        } else {
            self.set_level(0);
        }
    }
}

#[derive(Clone)]
enum EitherZoomSub {
    Single(#[allow(dead_code)] Rc<compose_rust::zoom::ZoomSubscription>),
    Multi(#[allow(dead_code)] Rc<compose_rust::zoom::WindowZoomSubscription>),
}

/// How large the screen is drawn, to read and to change while the app runs.
///
/// The factor is `k = os * 1.2^level`: the system's text size times the application's zoom
/// level, which steps from -8 to 8 the way an editor's zoom does. The renderer reads the
/// system's size, answers the zoom keys and keeps the level between runs. A page written
/// with HTML elements is laid out in a viewport of the window's size divided by `k` and
/// drawn at `k`, so this hook is for what the page cannot know about itself: a menu's
/// "Zoom In", a status bar that shows the level.
///
/// ```ignore
/// let zoom = use_zoom();
/// rsx! {
///     button { onclick: move |_| zoom.zoom_in(), "Zoom in" }
///     span { "{zoom.get().k:.2}x, level {zoom.get().level}" }
/// }
/// ```
///
/// The component re-renders when the renderer reports a different `k`, `os` or `level`.
/// `set_level` asks the renderer to draw at a level; it rides out on the window record in
/// the batch the handler produces, and the answer is a new report. Nothing is enlarged
/// before the renderer answers, so reading right after a set still gives the old value.
pub fn use_zoom() -> ZoomHandle {
    let window = resolve_current_window();
    let sub: Rc<RefCell<(u32, Option<EitherZoomSub>)>> =
        dioxus_core::use_hook(|| Rc::new(RefCell::new((u32::MAX, None))));
    let schedule = dioxus_core::schedule_update();
    {
        let mut state = sub.borrow_mut();
        if state.0 != window || state.1.is_none() {
            let new_sub = if window == 0 {
                EitherZoomSub::Single(Rc::new(compose_rust::zoom::subscribe(schedule)))
            } else {
                EitherZoomSub::Multi(Rc::new(compose_rust::zoom::subscribe_window(
                    window, schedule,
                )))
            };
            *state = (window, Some(new_sub));
        }
    }
    ZoomHandle::for_window(window)
}

/// Follows one node's size, and re-renders this component when its class changes.
///
/// The token is the name the screen gives the node, because a node id belongs to the
/// Renderer and never crosses back as something the Host chose. Attach it with
/// `observe_size` and read the size here.
///
/// ```ignore
/// let panel = use_node_size();
/// rsx! {
///     Card {
///         observe_size: panel.token(),
///         if panel.is_expanded() { Row { Left {} Right {} } } else { Left {} }
///     }
/// }
/// ```
///
/// A component that never calls this costs nothing: the modifier is the only thing that
/// makes the Renderer measure, and the modifier comes from here.
pub fn use_node_size() -> NodeSize {
    let subscription = dioxus_core::use_hook(|| {
        let token = compose_rust::window::next_node_token();
        Rc::new(compose_rust::window::subscribe_node(
            token,
            dioxus_core::schedule_update(),
        ))
    });
    NodeSize::of(subscription.token())
}

/// Reads the resolved design system inside a component, and re-renders it when the answer
/// changes.
///
/// ```ignore
/// let design = use_design_system();
/// if design.is_apple() {
///     // AC, and no memory row
/// } else {
///     // C, and a memory row
/// }
/// ```
///
/// **This is not how an application chooses a design system.** Choosing is
/// `Theme::unified`; this reads an answer already given. And it is not a way to paint
/// colours by hand: what a role resolves to is still the design system's, and reaching
/// for a literal because this told you which system is running is the thing the role
/// vocabulary exists to prevent.
pub fn use_design_system() -> DesignSystem {
    // Rc for the same reason the window size hook uses one: hook state has to be `Clone`,
    // and the registration must be dropped exactly once, with the last handle.
    dioxus_core::use_hook(|| {
        Rc::new(compose_rust::design::subscribe(
            dioxus_core::schedule_update(),
        ))
    });
    compose_rust::design::design_system()
}

/// The application's theme, to read and to change while it runs.
///
/// ```ignore
/// let theme = use_theme();
/// rsx! {
///     Button {
///         text: "Ember",
///         on_click: move |_| theme.set_palette(
///             Palette::new().with(ColorRole::Primary, Color::rgb(0xE8590C), Color::rgb(0xFF8A4C)),
///         ),
///     }
/// }
/// ```
///
/// A change is one `SetTheme` record in the batch the handler produces. Turning dark
/// because the platform did is not this: that is the Renderer's, and the Host never hears
/// about it.
pub fn use_theme() -> ThemeHandle {
    ThemeHandle::default()
}

/// Reads whether notifications may be shown, and re-renders when that changes.
///
/// ```ignore
/// match use_notification_permission() {
///     NotificationPermission::Denied => rsx! { Text { text: "Notifications are off in Settings" } },
///     _ => rsx! { Switch { checked: wants, onchange: move |_| request_notification_permission() } },
/// }
/// ```
///
/// What to show instead of a notification nobody will see is the application's decision.
/// The Renderer never swaps one for a message inside the window: the moment a notification
/// is for is the moment nobody is looking at the window.
pub fn use_notification_permission() -> NotificationPermission {
    dioxus_core::use_hook(|| {
        Rc::new(compose_rust::notification::subscribe_permission(
            dioxus_core::schedule_update(),
        ))
    });
    compose_rust::notification::notification_permission()
}

/// Runs `handler` when the user presses a notification this application posted.
///
/// One hook for the whole application rather than a callback per notification. A
/// notification stays in the notification centre for hours, long after the component that
/// posted it is gone, so what it carries back is the key it was posted under, which the
/// application can read at any time:
///
/// ```ignore
/// use_notification_activated(move |activation| {
///     if let Some(id) = activation.key.strip_prefix("session/") {
///         open_session(id);
///     }
/// });
/// ```
///
/// With no component holding this hook, an activation is dropped.
pub fn use_notification_activated(handler: impl FnMut(NotificationActivation) + 'static) {
    // A Callback rather than the closure itself, so the handler runs inside the scope of the
    // component that registered it and can write that component's signals.
    let callback = dioxus_hooks::use_callback(handler);
    dioxus_core::use_hook(|| {
        Rc::new(compose_rust::notification::on_activation(Rc::new(
            move |activation: NotificationActivation| callback.call(activation),
        )))
    });
}

/// One edit asked of a code editor, waiting for the batch it goes out in.
pub(crate) struct PendingEdit {
    pub(crate) token: u32,
    pub(crate) request_id: u32,
    pub(crate) base_version: u32,
    pub(crate) range: compose_rust::code::CodeRange,
    pub(crate) text: String,
}

thread_local! {
    static EDITS: std::cell::RefCell<Vec<PendingEdit>> = const { std::cell::RefCell::new(Vec::new()) };
    static NEXT_EDITOR_TOKEN: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
}

/// A way to change the document a code editor holds, for formatting, accepting a
/// completion or reloading a file that changed underneath it.
///
/// Created with [`use_code_editor`] and handed to the editor as its `handle`. An edit is
/// written against a version, the one the application last heard of. Where the reader has
/// typed since, the Renderer moves the range along with what they typed; where they typed
/// in the same place, it refuses the edit and says so through `on_edit_rejected`, because
/// an application must never type over its reader. An edit that is applied comes back as
/// an ordinary change, so the version stays one sequence whoever made the edit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CodeEditorHandle {
    token: u32,
}

impl CodeEditorHandle {
    /// A handle attached to no editor yet. [`use_code_editor`] is the usual way to get one,
    /// because it keeps the same handle across renders.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let token = NEXT_EDITOR_TOKEN.with(|next| {
            let token = next.get();
            next.set(token.checked_add(1).unwrap_or(1));
            token
        });
        Self { token }
    }

    pub(crate) fn token(self) -> u32 {
        self.token
    }

    /// Replaces `range` of the document as it stood at `base_version` with `text`.
    ///
    /// It goes out with the batch the current call produces, so call it from a component,
    /// an event handler or a task on the thread the application runs on. A worker updates
    /// a signal instead, and the component that reads the signal calls this.
    ///
    /// `request_id` is the application's own name for the edit, returned if it is refused.
    pub fn edit(
        &self,
        request_id: u32,
        base_version: u32,
        range: compose_rust::code::CodeRange,
        text: impl Into<String>,
    ) {
        let edit = PendingEdit {
            token: self.token,
            request_id,
            base_version,
            range,
            text: text.into(),
        };
        EDITS.with_borrow_mut(|edits| edits.push(edit));
    }
}

/// A handle to the editor this component draws, the same one on every render.
pub fn use_code_editor() -> CodeEditorHandle {
    dioxus_core::use_hook(CodeEditorHandle::new)
}

/// Hands every queued edit to `emit` and empties the queue.
pub(crate) fn drain_edits(mut emit: impl FnMut(&PendingEdit)) {
    let mut taken = EDITS.with_borrow_mut(std::mem::take);
    if taken.is_empty() {
        return;
    }
    for edit in &taken {
        emit(edit);
    }
    taken.clear();
    EDITS.with_borrow_mut(|edits| {
        if edits.is_empty() {
            *edits = taken;
        }
    });
}

/// Forgets every queued edit, for a new runtime taking over the thread: an edit addressed
/// to the old one's editor names nothing in the new one.
pub(crate) fn reset_edits() {
    EDITS.with_borrow_mut(Vec::clear);
}

/// Whether the platform asks for reduced motion, and a re-render when that changes.
///
/// The Renderer reports it once after start and again only when the setting changes.
/// Nothing is reduced on the application's behalf: an animation is played as it was
/// described, so a screen that should hold still when asked reads this and asks for less.
pub fn use_reduced_motion() -> compose_rust::ReducedMotion {
    dioxus_core::use_hook(|| {
        Rc::new(compose_rust::animation::subscribe_reduced_motion(
            dioxus_core::schedule_update(),
        ))
    });
    compose_rust::reduced_motion()
}

/// Runs `handler` for every event the Renderer reports about an animation it is playing.
///
/// Only the kinds an animation asked for are ever reported, so an application that asks
/// for none hears nothing and pays nothing while its animations play.
pub fn use_animation_event(handler: impl FnMut(compose_rust::AnimationMoment) + 'static) {
    let callback = dioxus_hooks::use_callback(handler);
    dioxus_core::use_hook(|| {
        Rc::new(compose_rust::animation::on_animation_event(Rc::new(
            move |moment: compose_rust::AnimationMoment| callback.call(moment),
        )))
    });
}

/// A value the Renderer animates to whenever it changes: what
/// [`animate_float_as_state`] returns. [`value`](Self::value) is the target, because the
/// value in between is the Renderer's and is never sent back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animated<T> {
    pub target: T,
    pub spec: compose_rust::AnimationSpec,
    /// Where a node that appears with this value starts, for the node's first value only.
    /// Without it a node starts at its target and nothing is played.
    pub(crate) enter_from: Option<T>,
    /// Non-zero asks the Renderer to report when the animation of a change ends, under
    /// this key. Zero reports nothing.
    pub(crate) watch: u64,
}

impl<T: Copy> Animated<T> {
    pub fn value(&self) -> T {
        self.target
    }
}

/// An opacity the Renderer animates to whenever `target` changes, given to an
/// `AbsoluteBox` as `animated_alpha`. A change is one `StartAnimation` and the Renderer
/// plays it on its own clock: this component is not rendered again while it plays.
pub fn animate_float_as_state(target: f32, spec: compose_rust::AnimationSpec) -> Animated<f32> {
    Animated {
        target,
        spec,
        enter_from: None,
        watch: 0,
    }
}

/// A background the Renderer animates to whenever `target` changes, given to an
/// `AbsoluteBox` as `animated_background`.
pub fn animate_color_as_state(
    target: compose_rust::Paint,
    spec: compose_rust::AnimationSpec,
) -> Animated<compose_rust::Paint> {
    Animated {
        target,
        spec,
        enter_from: None,
        watch: 0,
    }
}

/// A translation in dp the Renderer animates to whenever `target` changes, given to an
/// `AbsoluteBox` as `animated_offset`. Drawn as a transform, so the layout does not move.
pub fn animate_offset_as_state(
    target: (f32, f32),
    spec: compose_rust::AnimationSpec,
) -> Animated<(f32, f32)> {
    Animated {
        target,
        spec,
        enter_from: None,
        watch: 0,
    }
}
