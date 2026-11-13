//! HTML drag and drop: `dragstart`, `dragenter`, `dragover`, `dragleave`, `drop` and
//! `dragend`, made here from the pointer reports the renderer already sends.
//!
//! A primary-button press on an element with `draggable="true"` (or inside one) arms a
//! drag. Once the pointer has moved more than [`DRAG_THRESHOLD`] CSS pixels from the press,
//! the element receives `dragstart` and the drag is on. From then on every report of that
//! pointer is hit-tested here, against the boxes as they are drawn, and becomes drag
//! events instead of mouse and pointer events, the way a browser stops sending mouse
//! events during a drag:
//! - `dragenter` to the element the pointer came over, then `dragleave` to the one it
//!   left, then `dragover` to the element under the pointer, on every move;
//! - on release, `drop` to the element under the pointer if a handler of its last
//!   `dragover` prevented the default, else `dragleave`; then `dragend` to the element
//!   the drag started on, wherever the pointer is;
//! - Escape or a cancelled pointer ends the drag with `dragleave` and `dragend`.
//!
//! Every one of these bubbles. The press that ended as a drag does not click.
//!
//! The dragged data never leaves this process: it is a list of strings by format, shared
//! by the events of one drag. Nothing is handed to the operating system's drag and drop,
//! so a drag cannot reach another window and behaves the same on every platform.
//!
//! The renderer keeps the pointer with the node that took the press until its release, so
//! the moves arrive wherever the pointer goes. For the press to be reported at all, each
//! `draggable="true"` element and each element listening for `dragstart` subscribes to the
//! press, the moves, the release and the cancel ([`HtmlDom::drag_subscriptions`]).

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use dioxus_html::geometry::{ClientPoint, ElementPoint, PagePoint, ScreenPoint};
use dioxus_html::input_data::{MouseButton, MouseButtonSet, decode_mouse_button_set};
use dioxus_html::point_interaction::{
    InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction,
};
use dioxus_html::{
    DataTransfer, FileData, HasDataTransferData, HasDragData, HasFileData, HasMouseData, Modifiers,
    NativeDataTransfer,
};

use super::HtmlDom;
use super::pointer::{PointerInput, PointerKind};
use crate::html::NodeId;
use crate::layout::local;

/// How far the pointer has to move from the press, in CSS pixels, before the press becomes
/// a drag. A press that stays within it is a click.
const DRAG_THRESHOLD: f32 = 4.0;

/// The reports a drag is made from: the press, the moves, the release and the cancel.
const DRAG_KINDS: u32 = PointerKind::Down.bit()
    | PointerKind::Move.bit()
    | PointerKind::Up.bit()
    | PointerKind::Cancel.bit();

/// What one drag carries: strings by format, and the two effect names a page may set.
struct Transfer {
    /// Format and data, in the order the formats were first set.
    items: Vec<(String, String)>,
    effect_allowed: String,
    drop_effect: String,
    /// Whether the data may be changed. Only `dragstart` handlers may, as in a browser.
    writable: bool,
}

/// The `dataTransfer` of a drag event: a handle on the drag's [`Transfer`].
#[derive(Clone)]
struct TransferHandle(Arc<Mutex<Transfer>>);

impl TransferHandle {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Transfer {
            items: Vec::new(),
            effect_allowed: "uninitialized".to_string(),
            drop_effect: "none".to_string(),
            writable: true,
        })))
    }

    /// The transfer, also after a handler panicked while holding it: the strings are
    /// whole either way.
    fn lock(&self) -> MutexGuard<'_, Transfer> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The name a format is stored under: lower case, with the legacy names `text` and `url`
/// standing for `text/plain` and `text/uri-list`, as in a browser.
fn format_key(format: &str) -> String {
    let lower = format.trim().to_ascii_lowercase();
    match lower.as_str() {
        "text" => "text/plain".to_string(),
        "url" => "text/uri-list".to_string(),
        _ => lower,
    }
}

const READ_ONLY: &str = "the drag's data can only be changed by a dragstart handler; this \
                         event is a later one, so the data is read-only";

impl NativeDataTransfer for TransferHandle {
    fn get_data(&self, format: &str) -> Option<String> {
        let key = format_key(format);
        self.lock()
            .items
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, data)| data.clone())
    }

    fn set_data(&self, format: &str, data: &str) -> Result<(), String> {
        let mut transfer = self.lock();
        if !transfer.writable {
            return Err(READ_ONLY.to_string());
        }
        let key = format_key(format);
        match transfer.items.iter_mut().find(|(name, _)| *name == key) {
            Some(item) => item.1 = data.to_string(),
            None => transfer.items.push((key, data.to_string())),
        }
        Ok(())
    }

    fn clear_data(&self, format: Option<&str>) -> Result<(), String> {
        let mut transfer = self.lock();
        if !transfer.writable {
            return Err(READ_ONLY.to_string());
        }
        match format.map(format_key) {
            Some(key) => transfer.items.retain(|(name, _)| *name != key),
            None => transfer.items.clear(),
        }
        Ok(())
    }

    fn effect_allowed(&self) -> String {
        self.lock().effect_allowed.clone()
    }

    fn set_effect_allowed(&self, effect: &str) {
        let mut transfer = self.lock();
        if transfer.writable {
            transfer.effect_allowed = effect.to_string();
        }
    }

    fn drop_effect(&self) -> String {
        self.lock().drop_effect.clone()
    }

    fn set_drop_effect(&self, effect: &str) {
        self.lock().drop_effect = effect.to_string();
    }

    /// Always empty: a drag made here starts on an element of the page and carries
    /// strings. Files dropped on the window from outside are not this drag.
    fn files(&self) -> Vec<FileData> {
        Vec::new()
    }
}

/// What a drag event this document sends carries.
#[derive(Clone)]
pub(crate) struct DragEventData {
    client: (f64, f64),
    element: (f64, f64),
    modifiers: Modifiers,
    buttons: MouseButtonSet,
    transfer: TransferHandle,
}

impl InteractionLocation for DragEventData {
    fn client_coordinates(&self) -> ClientPoint {
        ClientPoint::new(self.client.0, self.client.1)
    }
    fn screen_coordinates(&self) -> ScreenPoint {
        ScreenPoint::new(self.client.0, self.client.1)
    }
    fn page_coordinates(&self) -> PagePoint {
        PagePoint::new(self.client.0, self.client.1)
    }
}

impl InteractionElementOffset for DragEventData {
    fn element_coordinates(&self) -> ElementPoint {
        ElementPoint::new(self.element.0, self.element.1)
    }
}

impl ModifiersInteraction for DragEventData {
    fn modifiers(&self) -> Modifiers {
        self.modifiers
    }
}

impl PointerInteraction for DragEventData {
    fn held_buttons(&self) -> MouseButtonSet {
        self.buttons
    }
    /// A drag is always the primary button's.
    fn trigger_button(&self) -> Option<MouseButton> {
        Some(MouseButton::Primary)
    }
}

impl HasMouseData for DragEventData {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl HasFileData for DragEventData {
    fn files(&self) -> Vec<FileData> {
        Vec::new()
    }
}

impl HasDataTransferData for DragEventData {
    fn data_transfer(&self) -> DataTransfer {
        DataTransfer::new(self.transfer.clone())
    }
}

impl HasDragData for DragEventData {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The keys and buttons held at a report, which every drag event of that report carries.
#[derive(Clone, Copy, Default)]
struct Held {
    modifiers: u32,
    buttons: u32,
}

impl Held {
    fn of(input: &PointerInput) -> Self {
        Self {
            modifiers: input.modifiers,
            buttons: input.buttons,
        }
    }
}

/// A primary-button press on a draggable element that has not moved far enough yet.
struct Press {
    pointer: u32,
    source: NodeId,
    /// Where the press was, in the page.
    at: (f32, f32),
}

/// A drag that is on.
struct Active {
    pointer: u32,
    /// The element the drag started on, which receives `dragend`.
    source: NodeId,
    transfer: TransferHandle,
    /// The element under the pointer, which the last `dragenter` went to.
    over: Option<NodeId>,
    /// Whether a handler of the last `dragover` prevented the default, which is how an
    /// element says it takes the drop.
    accepted: bool,
    /// Where the pointer last was, in the page, and what was held then.
    at: (f32, f32),
    held: Held,
}

/// The drag state a document keeps between pointer reports.
#[derive(Default)]
pub(crate) struct DragState {
    press: Option<Press>,
    active: Option<Active>,
    /// The element whose drag the last press became, until the next press or the next move
    /// with the button up. A click on it, in it or around it meanwhile is that same press,
    /// and is not delivered.
    dragged: Option<NodeId>,
}

impl HtmlDom {
    /// Adds what a drag needs to the pointer subscriptions in `kinds`: the press, the
    /// moves, the release and the cancel, for every `draggable="true"` element and every
    /// element listening for `dragstart`.
    pub(super) fn drag_subscriptions(&self, kinds: &mut HashMap<NodeId, u32>) {
        for (node, events) in self.state.listening() {
            if events.contains(&"dragstart") {
                *kinds.entry(node).or_insert(0) |= DRAG_KINDS;
            }
        }
        if !self.state.seen_draggable {
            return;
        }
        for (id, node) in self.doc.tree().iter() {
            if node.flags.is_in_document() && is_draggable(node) {
                *kinds.entry(id).or_insert(0) |= DRAG_KINDS;
            }
        }
    }

    /// Whether a drag is on: `dragstart` was delivered and neither `drop` nor `dragend`
    /// has been yet.
    pub fn is_dragging(&self) -> bool {
        self.drag.active.is_some()
    }

    /// Takes one pointer report, already known to be for this page, at `point` of the page
    /// with `hit` the box drawn on top there. Returns whether the report belonged to a
    /// drag: then it has been delivered as drag events, and is not also a mouse or pointer
    /// event.
    pub(super) fn drag_pointer(
        &mut self,
        input: &PointerInput,
        point: (f32, f32),
        hit: Option<NodeId>,
    ) -> bool {
        let pointer = input.pointer_id;
        let dragging = self
            .drag
            .active
            .as_ref()
            .is_some_and(|active| active.pointer == pointer);
        match input.kind {
            PointerKind::Down => {
                if self.drag.active.is_none() {
                    self.drag.dragged = None;
                    self.drag.press = (input.button == 1)
                        .then(|| hit.and_then(|hit| self.draggable_around(hit)))
                        .flatten()
                        .map(|source| Press {
                            pointer,
                            source,
                            at: point,
                        });
                }
                false
            }
            PointerKind::Move | PointerKind::Enter | PointerKind::Leave if dragging => {
                self.drag_to(point, hit, Held::of(input));
                true
            }
            PointerKind::Move => {
                if self.drag.active.is_none() && input.buttons & 1 == 0 {
                    // A move with the button up comes after the release, so a click
                    // after it is a new one.
                    self.drag.dragged = None;
                }
                let far = self.drag.press.as_ref().is_some_and(|press| {
                    press.pointer == pointer && {
                        let (dx, dy) = (point.0 - press.at.0, point.1 - press.at.1);
                        dx * dx + dy * dy > DRAG_THRESHOLD * DRAG_THRESHOLD
                    }
                });
                if !far || self.drag.active.is_some() {
                    return false;
                }
                let press = self.drag.press.take().expect("checked just above");
                self.start_drag(press, point, hit, Held::of(input))
            }
            PointerKind::Up if dragging => {
                self.finish_drag(Some((point, hit)), Held::of(input));
                true
            }
            PointerKind::Cancel if dragging => {
                self.finish_drag(None, Held::of(input));
                true
            }
            PointerKind::Up | PointerKind::Cancel => {
                if self
                    .drag
                    .press
                    .as_ref()
                    .is_some_and(|press| press.pointer == pointer)
                {
                    self.drag.press = None;
                }
                false
            }
            _ => false,
        }
    }

    /// Ends the drag that is on, if one is, without a drop: `dragleave` to the element
    /// under the pointer and `dragend` to the element the drag started on. What Escape
    /// does. Returns whether there was a drag to end.
    pub fn cancel_drag(&mut self) -> bool {
        let Some(held) = self.drag.active.as_ref().map(|active| active.held) else {
            return false;
        };
        self.finish_drag(None, held);
        true
    }

    /// Whether a click on `node` is the tail of a press that became a drag, and so is not
    /// a click: a drag is on, or has just ended, and `node` is the dragged element, inside
    /// it or around it. A click that is held back this way is the only one that press
    /// could make, so the next one is delivered.
    pub(super) fn drag_swallows_click(&mut self, node: NodeId) -> bool {
        let source = match (&self.drag.active, self.drag.dragged) {
            (Some(active), _) => active.source,
            (None, Some(source)) => source,
            (None, None) => return false,
        };
        if !self.is_around(source, node) && !self.is_around(node, source) {
            return false;
        }
        self.drag.dragged = None;
        true
    }

    /// Delivers `dragstart` for `press` and, unless a handler prevented its default, puts
    /// the drag on and delivers the first `dragenter` and `dragover` at `point`. Returns
    /// whether the drag is on.
    fn start_drag(
        &mut self,
        press: Press,
        point: (f32, f32),
        hit: Option<NodeId>,
        held: Held,
    ) -> bool {
        let transfer = TransferHandle::new();
        // As in a browser, `dragstart` is at the place of the press, not at the place the
        // pointer has reached since.
        let prevented = self.send_drag(press.source, "dragstart", press.at, held, &transfer);
        transfer.lock().writable = false;
        if prevented {
            // The page turned the drag down: the press stays an ordinary press.
            return false;
        }
        self.drag.dragged = Some(press.source);
        self.drag.active = Some(Active {
            pointer: press.pointer,
            source: press.source,
            transfer,
            over: None,
            accepted: false,
            at: press.at,
            held,
        });
        self.drag_to(point, hit, held);
        true
    }

    /// Moves the drag that is on to `point`: `dragenter` and `dragleave` when the element
    /// under the pointer changed, then `dragover` to the element under the pointer.
    fn drag_to(&mut self, point: (f32, f32), hit: Option<NodeId>, held: Held) {
        let target = hit.and_then(|hit| self.element_around(hit));
        let Some(active) = self.drag.active.as_mut() else {
            return;
        };
        let previous = active.over;
        let transfer = active.transfer.clone();
        active.over = target;
        active.at = point;
        active.held = held;
        if target != previous {
            if let Some(entered) = target {
                self.send_drag(entered, "dragenter", point, held, &transfer);
            }
            if let Some(left) = previous {
                self.send_drag(left, "dragleave", point, held, &transfer);
            }
        }
        let accepted =
            target.is_some_and(|over| self.send_drag(over, "dragover", point, held, &transfer));
        if let Some(active) = self.drag.active.as_mut() {
            active.accepted = accepted;
        }
    }

    /// Ends the drag that is on. With `release`, the pointer was let go there: the element
    /// under it receives `drop` if it accepted the last `dragover`, else `dragleave`.
    /// Without, the drag was cancelled and the element under the pointer receives
    /// `dragleave`. Either way the element the drag started on then receives `dragend`.
    fn finish_drag(&mut self, release: Option<((f32, f32), Option<NodeId>)>, held: Held) {
        if let Some((point, hit)) = release {
            let moved = self.drag.active.as_ref().is_some_and(|active| {
                active.at != point || active.over != hit.and_then(|hit| self.element_around(hit))
            });
            if moved {
                // The release is somewhere the last move was not: the element there has
                // to be asked before it can be dropped on.
                self.drag_to(point, hit, held);
            }
        }
        let Some(active) = self.drag.active.take() else {
            return;
        };
        let point = release.map_or(active.at, |(point, _)| point);
        if let Some(over) = active.over {
            let name = if release.is_some() && active.accepted {
                "drop"
            } else {
                "dragleave"
            };
            self.send_drag(over, name, point, held, &active.transfer);
        }
        self.send_drag(active.source, "dragend", point, held, &active.transfer);
        self.drag.press = None;
    }

    /// Delivers the drag event `name` to the nearest handler at or around `node`, bubbling
    /// from there, with the pointer at `point` of the page. Returns whether a handler
    /// prevented the default.
    fn send_drag(
        &mut self,
        node: NodeId,
        name: &'static str,
        point: (f32, f32),
        held: Held,
        transfer: &TransferHandle,
    ) -> bool {
        let Some((target, element)) = self.listener_target(node, name) else {
            return false;
        };
        // The point in the coordinates the handler's element was laid out in, from its
        // corner: a transformed element sees the pointer where it would be untransformed.
        let list = self.current.as_ref();
        let local_point = list
            .and_then(|list| list.local_point(target, point.0, point.1))
            .unwrap_or(point);
        let origin = list
            .and_then(|list| list.get(target))
            .map_or((0.0, 0.0), |entry| (entry.rect.x, entry.rect.y));
        let mut modifiers = Modifiers::empty();
        modifiers.set(Modifiers::SHIFT, held.modifiers & 1 != 0);
        modifiers.set(Modifiers::CONTROL, held.modifiers & 2 != 0);
        modifiers.set(Modifiers::ALT, held.modifiers & 4 != 0);
        modifiers.set(Modifiers::META, held.modifiers & 8 != 0);
        let data = DragEventData {
            client: (f64::from(point.0), f64::from(point.1)),
            element: (
                f64::from(local_point.0 - origin.0),
                f64::from(local_point.1 - origin.1),
            ),
            modifiers,
            buttons: decode_mouse_button_set(held.buttons as u16),
            transfer: transfer.clone(),
        };
        self.send(element, name, Box::new(data), true)
    }

    /// The parent of `node`, in the document or, for a box the layout made, in the layout.
    fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        self.doc
            .get_node(node)
            .and_then(|node| node.parent.or(node.layout_parent.get()))
    }

    /// Whether `inner` is `outer` or hangs below it.
    fn is_around(&self, outer: NodeId, inner: NodeId) -> bool {
        let mut current = Some(inner);
        // A document is never this deep; the bound only keeps a broken parent chain from
        // looping.
        for _ in 0..4096 {
            match current {
                Some(node) if node == outer => return true,
                Some(node) => current = self.parent_of(node),
                None => return false,
            }
        }
        false
    }

    /// `node` if it is an element, else the nearest element around it: a drag event is
    /// for an element, and the box on top at a point may be a run of text.
    fn element_around(&self, node: NodeId) -> Option<NodeId> {
        let mut current = Some(node);
        for _ in 0..4096 {
            let node = current?;
            if self
                .doc
                .get_node(node)
                .is_some_and(|node| node.is_element())
            {
                return Some(node);
            }
            current = self.parent_of(node);
        }
        None
    }

    /// The nearest element at or around `node` with `draggable="true"`.
    fn draggable_around(&self, node: NodeId) -> Option<NodeId> {
        let mut current = Some(node);
        for _ in 0..4096 {
            let node = current?;
            if self.doc.get_node(node).is_some_and(is_draggable) {
                return Some(node);
            }
            current = self.parent_of(node);
        }
        None
    }
}

/// Whether `node` is an element whose `draggable` attribute is `true`. Nothing is
/// draggable without it: not a link, not an image, not selected text.
fn is_draggable(node: &blitz_dom::Node) -> bool {
    node.is_element()
        && node
            .attr(local("draggable"))
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
}
