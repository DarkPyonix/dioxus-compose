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

/// The insets, in dp, that content drawn from the top of this component's window has to
/// leave free for the window's own buttons.
///
/// Under `Chrome::FullSize` the content starts at the very top of the window and the
/// window buttons are drawn over it, so the application decides where to make room.
/// `leading` is the width the macOS traffic lights take, `trailing` the width the
/// Windows caption buttons take, and `top` the height of that band. All three are zero
/// for a window with no buttons to avoid and on platforms whose window is not ours
/// (iOS, Android, the browser), and they are zero until the renderer first reports.
///
/// The window is chosen the way [`use_window_size`] chooses it. Unlike that hook's
/// size class, a change of the safe area alone (entering or leaving full screen, say)
/// redraws every component that called this, and only those.
///
/// ```ignore
/// let safe = use_window_safe_area();
/// rsx! { Row { padding: (0.0, safe.leading, 0.0, 0.0), Toolbar {} } }
/// ```
pub fn use_window_safe_area() -> compose_rust::window::WindowSafeArea {
    use_window_size().safe_area
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

/// One request asked of a code editor, waiting for the batch it goes out in.
pub(crate) enum EditorRequest {
    Edit(PendingEdit),
    Select {
        token: u32,
        base_version: u32,
        anchor: compose_rust::code::Position,
        active: compose_rust::code::Position,
    },
    Reveal {
        token: u32,
        base_version: u32,
        range: compose_rust::code::CodeRange,
        align: compose_rust::schema::RevealAlign,
    },
    Fold {
        token: u32,
        start_line: u32,
        collapsed: bool,
    },
    /// Where a caret at `position` would be drawn. The callback is numbered only when the
    /// query is written to an editor, so a handle no editor carries leaves nothing waiting.
    QueryPosition {
        token: u32,
        position: compose_rust::code::Position,
        on_answer: PositionCallback,
    },
    ScrollTo {
        token: u32,
        offset_px: f64,
    },
    EditGroup {
        token: u32,
        request_id: u32,
        base_version: u32,
        edits: Vec<(compose_rust::code::CodeRange, String)>,
    },
    SetSelections {
        token: u32,
        base_version: u32,
        selections: Vec<(compose_rust::code::Position, compose_rust::code::Position)>,
    },
    InsertSnippet {
        token: u32,
        base_version: u32,
        range: compose_rust::code::CodeRange,
        snippet: String,
    },
}

/// The callback a position query waits with, taken out once when the query is written.
pub(crate) type PositionCallback =
    std::cell::RefCell<Option<Box<dyn FnOnce(Option<compose_rust::code::CodeRect>)>>>;

impl EditorRequest {
    /// The handle the request is addressed to.
    pub(crate) fn token(&self) -> u32 {
        match self {
            Self::Edit(edit) => edit.token,
            Self::Select { token, .. }
            | Self::Reveal { token, .. }
            | Self::Fold { token, .. }
            | Self::QueryPosition { token, .. }
            | Self::ScrollTo { token, .. }
            | Self::EditGroup { token, .. }
            | Self::SetSelections { token, .. }
            | Self::InsertSnippet { token, .. } => *token,
        }
    }
}

thread_local! {
    static EDITS: std::cell::RefCell<Vec<EditorRequest>> = const { std::cell::RefCell::new(Vec::new()) };
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
        EDITS.with_borrow_mut(|edits| edits.push(EditorRequest::Edit(edit)));
    }

    /// Puts the selection between `anchor` and `active`, places in the document as it stood at
    /// `base_version`. The Renderer moves both ends past what the reader typed since, and refuses
    /// the request, with a protocol error, where an end is not a place in the document. An
    /// application uses it to take the reader to a match or a problem. Like [`edit`](Self::edit)
    /// it goes out with the batch the current call produces.
    pub fn select(
        &self,
        base_version: u32,
        anchor: compose_rust::code::Position,
        active: compose_rust::code::Position,
    ) {
        EDITS.with_borrow_mut(|edits| {
            edits.push(EditorRequest::Select {
                token: self.token,
                base_version,
                anchor,
                active,
            });
        });
    }

    /// Scrolls the editor until `range`, as the document stood at `base_version`, is on screen,
    /// placed as `align` says. A range inside a folded stretch unfolds it.
    pub fn reveal(
        &self,
        base_version: u32,
        range: compose_rust::code::CodeRange,
        align: compose_rust::schema::RevealAlign,
    ) {
        EDITS.with_borrow_mut(|edits| {
            edits.push(EditorRequest::Reveal {
                token: self.token,
                base_version,
                range,
                align,
            });
        });
    }

    /// Folds or unfolds the stretch of the editor that starts on `start_line`.
    pub fn fold(&self, start_line: u32, collapsed: bool) {
        EDITS.with_borrow_mut(|edits| {
            edits.push(EditorRequest::Fold {
                token: self.token,
                start_line,
                collapsed,
            });
        });
    }

    /// Asks where a caret at `position` would be drawn, for a hover card, a rename box or a
    /// parameter hint placed next to a word that is not where the caret is.
    ///
    /// The question goes out with the batch the current call produces, like
    /// [`edit`](Self::edit). The Renderer answers it exactly once, and `on_answer` then runs
    /// on the thread the application runs on, inside the call that carried the answer, with
    /// the rectangle in the window's dp (the editor's scroll and the window's zoom already in
    /// it), or with `None` where the position is not on screen: scrolled away, folded,
    /// outside the document, or in an editor that is gone. Anything it changes is in the
    /// batch that call returns.
    ///
    /// A handle no editor carries asks nothing, as its edits are dropped, and `on_answer` is
    /// dropped with the question without being called.
    pub fn query_position_rect(
        &self,
        position: compose_rust::code::Position,
        on_answer: impl FnOnce(Option<compose_rust::code::CodeRect>) + 'static,
    ) {
        EDITS.with_borrow_mut(|edits| {
            edits.push(EditorRequest::QueryPosition {
                token: self.token,
                position,
                on_answer: std::cell::RefCell::new(Some(Box::new(on_answer))),
            });
        });
    }

    /// Scrolls the editor so that `offset_px` physical pixels of the document are above the
    /// top of its view: the unit `on_code_scrolled` reports in and `on_code_metrics`
    /// measures a row in. The Renderer keeps the offset inside the document. Like
    /// [`edit`](Self::edit) it goes out with the batch the current call produces.
    pub fn set_scroll_offset(&self, offset_px: f64) {
        EDITS.with_borrow_mut(|edits| {
            edits.push(EditorRequest::ScrollTo {
                token: self.token,
                offset_px,
            });
        });
    }

    /// Replaces every range in `edits` with its text, all of them or none, as one undo step.
    ///
    /// Every range is in the document as it stood at `base_version`, so an edit never moves
    /// the ranges of the others, and they may be listed in any order; they must not overlap.
    /// Where the reader has typed since, the Renderer moves every range past what they
    /// typed, and keeps the whole group out where any range meets it. The editor's
    /// `on_edit_group_result` then hears exactly once what became of the group, named by
    /// `request_id`, and an edit that went in also comes back as an ordinary change. A
    /// group with no edits sends nothing. Like [`edit`](Self::edit) it goes out with the
    /// batch the current call produces.
    pub fn edit_group<T: Into<String>>(
        &self,
        request_id: u32,
        base_version: u32,
        edits: impl IntoIterator<Item = (compose_rust::code::CodeRange, T)>,
    ) {
        let edits: Vec<_> = edits
            .into_iter()
            .map(|(range, text)| (range, text.into()))
            .collect();
        if edits.is_empty() {
            return;
        }
        EDITS.with_borrow_mut(|queue| {
            queue.push(EditorRequest::EditGroup {
                token: self.token,
                request_id,
                base_version,
                edits,
            });
        });
    }

    /// Sets every selection of the editor, each an anchor and an active end in the document
    /// as it stood at `base_version`, the first the primary one.
    ///
    /// Typing, deleting and pasting then happen at every selection as one step, and Escape
    /// goes back to the primary one. The Renderer moves every end past what the reader typed
    /// since and refuses the request, with a protocol error, where an end is not a place in
    /// the document or the list is empty. Like [`edit`](Self::edit) it goes out with the
    /// batch the current call produces.
    pub fn set_selections(
        &self,
        base_version: u32,
        selections: impl IntoIterator<
            Item = (compose_rust::code::Position, compose_rust::code::Position),
        >,
    ) {
        EDITS.with_borrow_mut(|edits| {
            edits.push(EditorRequest::SetSelections {
                token: self.token,
                base_version,
                selections: selections.into_iter().collect(),
            });
        });
    }

    /// Replaces `range`, in the document as it stood at `base_version`, with `snippet`, and
    /// opens its tab stops.
    ///
    /// The snippet is written in the part of VS Code's syntax the editor reads: `$1`,
    /// `${1:placeholder}`, `${1|one,two|}`, `$0`, and `\$` for a dollar sign
    /// ([`crate::escape_snippet_text`] escapes plain text). The caret selects the
    /// first stop, Tab and Shift+Tab move between the stops, and the session ends at `$0`,
    /// on Escape or when the caret leaves the snippet. What it leaves arrives as ordinary
    /// changes and undoes as one step. Like [`edit`](Self::edit) it goes out with the batch
    /// the current call produces.
    pub fn insert_snippet(
        &self,
        base_version: u32,
        range: compose_rust::code::CodeRange,
        snippet: impl Into<String>,
    ) {
        EDITS.with_borrow_mut(|edits| {
            edits.push(EditorRequest::InsertSnippet {
                token: self.token,
                base_version,
                range,
                snippet: snippet.into(),
            });
        });
    }
}

/// A handle to the editor this component draws, the same one on every render.
pub fn use_code_editor() -> CodeEditorHandle {
    dioxus_core::use_hook(CodeEditorHandle::new)
}

/// Hands every queued edit to `emit` and empties the queue.
pub(crate) fn drain_edits(mut emit: impl FnMut(&EditorRequest)) {
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

/// Whether the system appearance is light or dark, and a re-render when that changes.
///
/// The Renderer reports it once before its first frame and again on every change, whatever
/// the theme's colour scheme is, so this is the system's own setting. It is `None` until the
/// first report. Native widgets already follow the theme without it; it is for what an
/// application draws itself. [`compose_rust::resolved_color_scheme`] gives the scheme the
/// theme is drawn in.
pub fn use_system_color_scheme() -> Option<compose_rust::SystemColorScheme> {
    dioxus_core::use_hook(|| {
        Rc::new(compose_rust::subscribe_system_color_scheme(
            dioxus_core::schedule_update(),
        ))
    });
    compose_rust::system_color_scheme()
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
