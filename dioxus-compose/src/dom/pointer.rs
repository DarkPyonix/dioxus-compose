//! Pointer input for the HTML path: node-level pointer events in, `:hover` and `:active` on
//! the document, DOM mouse, pointer and wheel events out, and the `cursor` and
//! `user-select` of a node for the renderer.
//!
//! The renderer reports a pointer per subscribed node: the node the event is for, with the
//! position in that node's own space in dp. It carries an event up through the subscribed
//! ancestors until a handler consumes it, so a handler here is only ever called for the
//! node that owns it and never bubbles a second time. [`HtmlDom::pointer_event`] takes one
//! such report.
//!
//! What stays with this side:
//! - which box is under the pointer. A report for a node counts only when the box drawn on
//!   top at that point is the node or inside it, so `pointer-events: none`, an overlay and
//!   a transformed box decide the same way for hover as they do for a click;
//! - `:hover` and `:active`, set on the box under the pointer and every ancestor of it,
//!   the way a browser does, and applied at the next layout;
//! - the pointer's capture: from a press until its release or cancel, the pointer's moves
//!   and its release belong to the node that took the press wherever the pointer goes, and
//!   no other box is hovered meanwhile (dragging a splitter).

use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;

use blitz_dom::NodeData;
use dioxus_core::Event;
use dioxus_html::geometry::{ClientPoint, ElementPoint, PagePoint, ScreenPoint, WheelDelta};
use dioxus_html::input_data::{MouseButton, MouseButtonSet, decode_mouse_button_set};
use dioxus_html::point_interaction::{
    InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction,
};
use dioxus_html::{HasMouseData, HasPointerData, HasWheelData, Modifiers, PlatformEventData};
use style::values::computed::ui::CursorKind;

use super::HtmlDom;
use crate::html::NodeId;
use crate::layout::local;

/// What a pointer report is about. The numbers are the wire values of the `kind` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PointerKind {
    Move = 1,
    Enter = 2,
    Leave = 3,
    Down = 4,
    Up = 5,
    Cancel = 6,
    Wheel = 7,
    ContextMenu = 8,
}

impl PointerKind {
    /// The kind a wire value names, or `None` for a value that names none.
    pub fn from_wire(value: u32) -> Option<Self> {
        Some(match value {
            1 => Self::Move,
            2 => Self::Enter,
            3 => Self::Leave,
            4 => Self::Down,
            5 => Self::Up,
            6 => Self::Cancel,
            7 => Self::Wheel,
            8 => Self::ContextMenu,
            _ => return None,
        })
    }

    /// The bit this kind has in a subscription mask.
    pub const fn bit(self) -> u32 {
        1 << (self as u32 - 1)
    }
}

/// Which device a pointer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerType {
    Mouse,
    Touch,
    Pen,
}

/// The unit of a wheel report's deltas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelMode {
    Pixel,
    Line,
    Page,
}

/// One pointer report for one node.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerInput {
    pub kind: PointerKind,
    /// Tells apart the fingers, pens and mice that are down at once. A mouse is 1.
    pub pointer_id: u32,
    pub pointer_type: PointerType,
    /// The position in the node's own space, in dp, with transforms and zoom taken off.
    pub x: f32,
    pub y: f32,
    /// The buttons held now: primary 1, secondary 2, tertiary 4, back 8, forward 16.
    pub buttons: u32,
    /// The button that caused a press or a release, in the same bits, else 0.
    pub button: u32,
    /// Shift 1, Ctrl 2, Alt 4, Meta 8.
    pub modifiers: u32,
    /// The number of presses in a row, on a press or a release, else 0.
    pub click_count: u32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub delta_mode: WheelMode,
}

impl PointerInput {
    /// A report of a mouse at `(x, y)` with nothing held.
    pub fn new(kind: PointerKind, x: f32, y: f32) -> Self {
        Self {
            kind,
            pointer_id: 1,
            pointer_type: PointerType::Mouse,
            x,
            y,
            buttons: 0,
            button: 0,
            modifiers: 0,
            click_count: 0,
            delta_x: 0.0,
            delta_y: 0.0,
            delta_mode: WheelMode::Pixel,
        }
    }

    /// A press of `button` that leaves it held.
    pub fn press(x: f32, y: f32, button: u32, click_count: u32) -> Self {
        Self {
            buttons: button,
            button,
            click_count,
            ..Self::new(PointerKind::Down, x, y)
        }
    }

    /// The release of `button`, which leaves nothing held.
    pub fn release(x: f32, y: f32, button: u32, click_count: u32) -> Self {
        Self {
            button,
            click_count,
            ..Self::new(PointerKind::Up, x, y)
        }
    }

    /// A wheel turn of `(delta_x, delta_y)` pixels.
    pub fn wheel(x: f32, y: f32, delta_x: f32, delta_y: f32) -> Self {
        Self {
            delta_x,
            delta_y,
            ..Self::new(PointerKind::Wheel, x, y)
        }
    }
}

/// A node the renderer should send pointer reports for, and the kinds it should send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerSubscription {
    pub node: NodeId,
    /// The bits of [`PointerKind::bit`] for each kind the node takes. Never 0.
    pub kinds: u32,
}

/// The CSS `cursor` keywords. The numbers are the wire values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Cursor {
    Auto = 0,
    Default,
    Pointer,
    Text,
    VerticalText,
    Crosshair,
    Move,
    Grab,
    Grabbing,
    NotAllowed,
    NoDrop,
    Wait,
    Progress,
    Help,
    ContextMenu,
    Cell,
    Copy,
    Alias,
    AllScroll,
    ZoomIn,
    ZoomOut,
    ColResize,
    RowResize,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    None,
}

impl Cursor {
    fn from_style(kind: CursorKind) -> Self {
        match kind {
            CursorKind::Auto => Self::Auto,
            CursorKind::Default => Self::Default,
            CursorKind::Pointer => Self::Pointer,
            CursorKind::Text => Self::Text,
            CursorKind::VerticalText => Self::VerticalText,
            CursorKind::Crosshair => Self::Crosshair,
            CursorKind::Move => Self::Move,
            CursorKind::Grab => Self::Grab,
            CursorKind::Grabbing => Self::Grabbing,
            CursorKind::NotAllowed => Self::NotAllowed,
            CursorKind::NoDrop => Self::NoDrop,
            CursorKind::Wait => Self::Wait,
            CursorKind::Progress => Self::Progress,
            CursorKind::Help => Self::Help,
            CursorKind::ContextMenu => Self::ContextMenu,
            CursorKind::Cell => Self::Cell,
            CursorKind::Copy => Self::Copy,
            CursorKind::Alias => Self::Alias,
            CursorKind::AllScroll => Self::AllScroll,
            CursorKind::ZoomIn => Self::ZoomIn,
            CursorKind::ZoomOut => Self::ZoomOut,
            CursorKind::ColResize => Self::ColResize,
            CursorKind::RowResize => Self::RowResize,
            CursorKind::EResize => Self::EResize,
            CursorKind::NResize => Self::NResize,
            CursorKind::NeResize => Self::NeResize,
            CursorKind::NwResize => Self::NwResize,
            CursorKind::SResize => Self::SResize,
            CursorKind::SeResize => Self::SeResize,
            CursorKind::SwResize => Self::SwResize,
            CursorKind::WResize => Self::WResize,
            CursorKind::EwResize => Self::EwResize,
            CursorKind::NsResize => Self::NsResize,
            CursorKind::NeswResize => Self::NeswResize,
            CursorKind::NwseResize => Self::NwseResize,
            CursorKind::None => Self::None,
        }
    }
}

/// The CSS `user-select` keywords. The numbers are the wire values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum UserSelect {
    Auto = 0,
    Text = 1,
    None = 2,
    All = 3,
}

/// The pointer state a document keeps between reports.
#[derive(Default)]
pub(crate) struct PointerState {
    /// The elements that match `:hover`: the box under the pointer and its ancestors.
    hover: Vec<NodeId>,
    /// The elements that match `:active`.
    active: Vec<NodeId>,
    /// The node that took the press of each pointer, until its release.
    captures: HashMap<u32, NodeId>,
    /// The node whose capture the last release ended. The same release still arrives for
    /// the subscribed ancestors of that node, and they take it as the capture did.
    released: Option<(u32, NodeId)>,
    /// Set when `:hover` or `:active` moved to other elements and the page has not been
    /// drawn since.
    restyle: bool,
}

/// The kinds of report a listener for `event` needs.
fn kinds_for(event: &str) -> u32 {
    use PointerKind::*;
    match event {
        "mousemove" | "pointermove" => Move.bit(),
        "mouseenter" | "mouseover" | "pointerenter" | "pointerover" => Enter.bit(),
        "mouseleave" | "mouseout" | "pointerleave" | "pointerout" => Leave.bit(),
        "mousedown" | "pointerdown" => Down.bit(),
        "mouseup" | "pointerup" | "dblclick" => Up.bit(),
        "pointercancel" => Cancel.bit(),
        "wheel" => Wheel.bit(),
        "contextmenu" => ContextMenu.bit(),
        _ => 0,
    }
}

/// The DOM events each kind of report is delivered as, in order.
fn events_for(kind: PointerKind) -> &'static [&'static str] {
    match kind {
        PointerKind::Move => &["pointermove", "mousemove"],
        PointerKind::Enter => &["pointerover", "pointerenter", "mouseover", "mouseenter"],
        PointerKind::Leave => &["pointerout", "pointerleave", "mouseout", "mouseleave"],
        PointerKind::Down => &["pointerdown", "mousedown"],
        PointerKind::Up => &["pointerup", "mouseup"],
        PointerKind::Cancel => &["pointercancel"],
        PointerKind::Wheel => &["wheel"],
        PointerKind::ContextMenu => &["contextmenu"],
    }
}

const HOVER_KINDS: u32 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5);

impl HtmlDom {
    /// The nodes that take pointer reports, with the kinds each takes: a node for every
    /// pointer listener the page has, and the `<html>` element for `:hover` and `:active`
    /// when a stylesheet uses either. A page with neither has none, so the renderer tracks
    /// nothing for it.
    pub fn pointer_subscriptions(&self) -> Vec<PointerSubscription> {
        let mut kinds: HashMap<NodeId, u32> = HashMap::new();
        for (node, events) in self.state.listening() {
            let mask = events.iter().fold(0, |mask, event| mask | kinds_for(event));
            if mask != 0 {
                kinds.insert(node, mask);
            }
        }
        if self.stylesheets_use_pointer_state() {
            *kinds.entry(self.root).or_insert(0) |= HOVER_KINDS;
        }
        let mut subscriptions: Vec<_> = kinds
            .into_iter()
            .map(|(node, kinds)| PointerSubscription { node, kinds })
            .collect();
        subscriptions.sort_by_key(|subscription| subscription.node);
        subscriptions
    }

    /// Whether any `<style>` in the document mentions `:hover` or `:active`.
    fn stylesheets_use_pointer_state(&self) -> bool {
        self.doc.tree().iter().any(|(_, node)| {
            let NodeData::Text(text) = &node.data else {
                return false;
            };
            let in_style = node
                .parent
                .and_then(|parent| self.doc.get_node(parent))
                .is_some_and(|parent| parent.data.is_element_with_tag_name(&local("style")));
            in_style && (text.content.contains(":hover") || text.content.contains(":active"))
        })
    }

    /// The deepest box under the pointer as the last report left it, or `None` when the
    /// pointer is over nothing the page draws.
    pub fn hovered(&self) -> Option<NodeId> {
        self.pointer.hover.first().copied()
    }

    /// Whether a pointer report moved `:hover` or `:active` since the last call, which
    /// means the page needs a frame to be restyled. Reading it clears it.
    pub fn take_pointer_restyle(&mut self) -> bool {
        std::mem::take(&mut self.pointer.restyle)
    }

    /// Whether `node` matches `:hover` now.
    pub fn is_hovered(&self, node: NodeId) -> bool {
        self.pointer.hover.contains(&node)
    }

    /// Whether `node` matches `:active` now.
    pub fn is_active(&self, node: NodeId) -> bool {
        self.pointer.active.contains(&node)
    }

    /// What a renderer calls with a pointer report for `node`. Runs the handlers it names,
    /// updates `:hover` and `:active` (applied at the next layout), and returns whether a
    /// handler consumed the event by preventing its default, which stops the renderer from
    /// carrying it further.
    ///
    /// A report is dropped, and `false` returned, when the box on top at the pointer is not
    /// `node` or inside it, unless the pointer is captured: then the moves and the release
    /// are for the node that took the press.
    pub fn pointer_event(&mut self, node: NodeId, input: &PointerInput) -> bool {
        let Some(list) = self.current.as_ref() else {
            return false;
        };
        let Some(origin) = list.get(node).map(|entry| (entry.rect.x, entry.rect.y)) else {
            return false;
        };
        let Some(point) = list.project(node, origin.0 + input.x, origin.1 + input.y) else {
            return false;
        };
        let hit = list.hit_test(point.0, point.1);

        let kind = input.kind;
        let pointer = input.pointer_id;
        if !matches!(kind, PointerKind::Up | PointerKind::Cancel) {
            self.pointer.released = None;
        }
        let held = self.pointer.captures.get(&pointer).copied();
        // The pointer belongs to the node that took the press, wherever it is. The release
        // that ended the capture still arrives for the ancestors of that node.
        let routed = match kind {
            PointerKind::Move | PointerKind::Enter | PointerKind::Leave => held,
            PointerKind::Up | PointerKind::Cancel => held.or_else(|| {
                self.pointer
                    .released
                    .filter(|(id, _)| *id == pointer)
                    .map(|(_, captured)| captured)
            }),
            _ => None,
        };
        let captured = routed.is_some();
        if let Some(owner) = routed {
            if !self.contains(node, owner) {
                return false;
            }
        } else if !matches!(kind, PointerKind::Leave | PointerKind::Cancel) {
            match hit {
                Some(top) if self.contains(node, top) => {}
                _ => return false,
            }
        }

        match kind {
            PointerKind::Move | PointerKind::Enter if !captured => self.set_hover(hit),
            PointerKind::Leave if !captured => self.set_hover(hit),
            PointerKind::Down => {
                self.set_hover(hit);
                self.set_active(hit);
                self.pointer.captures.entry(pointer).or_insert(node);
            }
            _ => {}
        }

        let data = EventData::new(input, point, (input.x, input.y));
        let mut consumed = false;
        for name in events_for(kind) {
            consumed |= self.send_own(node, name, &data);
        }
        if kind == PointerKind::Up && input.click_count == 2 && input.button == 1 {
            consumed |= self.send_own(node, "dblclick", &data);
        }

        if matches!(kind, PointerKind::Up | PointerKind::Cancel) {
            if let Some(held) = self.pointer.captures.remove(&pointer) {
                self.pointer.released = Some((pointer, held));
            }
            self.set_active(None);
            if kind == PointerKind::Up {
                self.set_hover(hit);
            }
        }
        consumed
    }

    /// Delivers `name` to the handler of `node` itself and no other, and returns whether
    /// it prevented the default.
    fn send_own(&mut self, node: NodeId, name: &'static str, data: &EventData) -> bool {
        if !self.state.listeners(node).contains(&name) {
            return false;
        }
        let Some(element) = self.state.element_of(node) else {
            return false;
        };
        let event = Event::new(
            Rc::new(PlatformEventData::new(Box::new(data.clone()))),
            false,
        );
        let probe = event.clone();
        self.vdom
            .runtime()
            .handle_event(name, event.into_any(), element);
        !probe.default_action_enabled()
    }

    /// Whether `inner` is `outer` or hangs below it.
    fn contains(&self, outer: NodeId, inner: NodeId) -> bool {
        self.ancestors(inner).contains(&outer)
    }

    /// `node` and the nodes it hangs from, nearest first.
    fn ancestors(&self, node: NodeId) -> Vec<NodeId> {
        let mut chain = Vec::new();
        let mut current = Some(node);
        while let Some(id) = current {
            if chain.contains(&id) {
                break;
            }
            chain.push(id);
            current = self
                .doc
                .get_node(id)
                .and_then(|node| node.parent.or(node.layout_parent.get()));
        }
        chain
    }

    /// The elements `:hover` and `:active` reach from `node`: itself and its ancestors.
    fn element_chain(&self, node: Option<NodeId>) -> Vec<NodeId> {
        let Some(node) = node else {
            return Vec::new();
        };
        self.ancestors(node)
            .into_iter()
            .filter(|id| self.doc.get_node(*id).is_some_and(|node| node.is_element()))
            .collect()
    }

    fn set_hover(&mut self, target: Option<NodeId>) {
        let next = self.element_chain(target);
        let previous = std::mem::take(&mut self.pointer.hover);
        for id in previous.iter().copied().filter(|id| !next.contains(id)) {
            self.doc.snapshot_node_and(id, |node| node.unhover());
        }
        for id in next.iter().copied().filter(|id| !previous.contains(id)) {
            self.doc.snapshot_node_and(id, |node| node.hover());
        }
        self.pointer.restyle |= next != previous;
        self.pointer.hover = next;
    }

    fn set_active(&mut self, target: Option<NodeId>) {
        let next = self.element_chain(target);
        let previous = std::mem::take(&mut self.pointer.active);
        for id in previous.iter().copied().filter(|id| !next.contains(id)) {
            self.doc.snapshot_node_and(id, |node| node.unactive());
        }
        for id in next.iter().copied().filter(|id| !previous.contains(id)) {
            self.doc.snapshot_node_and(id, |node| node.active());
        }
        self.pointer.restyle |= next != previous;
        self.pointer.active = next;
    }

    /// The `cursor` the page gives `node`, inherited the way CSS inherits it. [`Cursor::Auto`]
    /// when the page does not choose one, which leaves the shape to the widget under the
    /// pointer.
    pub fn cursor(&self, node: NodeId) -> Cursor {
        self.doc
            .get_node(node)
            .and_then(|node| node.primary_styles())
            .map_or(Cursor::Auto, |style| {
                Cursor::from_style(style.clone_cursor().keyword)
            })
    }

    /// The `user-select` the page gives `node`, inherited: the nearest element from `node`
    /// outwards whose value is not `auto`.
    ///
    /// The style engine this document uses does not know the property, so it is read here:
    /// from the `style` attribute and from the rules of the `<style>` elements, matched
    /// with the engine's selector matching and ordered by importance, specificity and
    /// position. Rules inside `@media` are not read, since their conditions are not
    /// evaluated here.
    pub fn user_select(&self, node: NodeId) -> UserSelect {
        let author = self.author_user_select();
        for id in self.ancestors(node) {
            let inline = self
                .doc
                .get_node(id)
                .and_then(|node| node.attr(local("style")))
                .and_then(declared_user_select);
            let value = match (inline, author.get(&id)) {
                (Some((value, important)), Some((author_important, _, _, author_value))) => {
                    if important || !author_important {
                        value
                    } else {
                        *author_value
                    }
                }
                (Some((value, _)), None) => value,
                (None, Some((_, _, _, value))) => *value,
                (None, None) => continue,
            };
            if value != UserSelect::Auto {
                return value;
            }
        }
        UserSelect::Auto
    }

    /// The author stylesheets' `user-select` for each node a rule reaches, with the key the
    /// cascade orders by: importance, specificity, position.
    fn author_user_select(&self) -> HashMap<NodeId, (bool, u32, usize, UserSelect)> {
        let mut winners: HashMap<NodeId, (bool, u32, usize, UserSelect)> = HashMap::new();
        let mut position = 0;
        for (_, node) in self.doc.tree().iter() {
            let NodeData::Text(text) = &node.data else {
                continue;
            };
            let in_style = node
                .parent
                .and_then(|parent| self.doc.get_node(parent))
                .is_some_and(|parent| parent.data.is_element_with_tag_name(&local("style")));
            if !in_style {
                continue;
            }
            for (selectors, body) in style_rules(&text.content) {
                position += 1;
                let Some((value, important)) = declared_user_select(&body) else {
                    continue;
                };
                for part in split_selectors(&selectors) {
                    if part.is_empty() || part.contains("::") {
                        continue;
                    }
                    let Ok(matched) = self.doc.query_selector_all(&part) else {
                        continue;
                    };
                    let key = (important, specificity(&part), position);
                    for id in matched {
                        let better = winners
                            .get(&id)
                            .is_none_or(|(i, s, p, _)| key > (*i, *s, *p));
                        if better {
                            winners.insert(id, (key.0, key.1, key.2, value));
                        }
                    }
                }
            }
        }
        winners
    }
}

/// The last `user-select` a declaration block sets, and whether it is `!important`.
fn declared_user_select(block: &str) -> Option<(UserSelect, bool)> {
    let mut found = None;
    for declaration in block.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        if !matches!(
            name.trim().to_ascii_lowercase().as_str(),
            "user-select" | "-webkit-user-select" | "-moz-user-select"
        ) {
            continue;
        }
        let value = value.trim().to_ascii_lowercase();
        let important = value.contains("!important");
        let keyword = value.replace("!important", "");
        let parsed = match keyword.trim() {
            "none" | "-moz-none" => UserSelect::None,
            "text" => UserSelect::Text,
            "all" => UserSelect::All,
            "auto" => UserSelect::Auto,
            _ => continue,
        };
        found = Some((parsed, important));
    }
    found
}

/// The style rules of a stylesheet as (selector list, declaration block), in order. Rules
/// inside `@supports` and `@layer` are included; other at-rules are skipped whole.
fn style_rules(css: &str) -> Vec<(String, String)> {
    let mut clean = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        clean.push_str(&rest[..start]);
        rest = match rest[start + 2..].find("*/") {
            Some(end) => &rest[start + 2 + end + 2..],
            None => "",
        };
    }
    clean.push_str(rest);
    let mut rules = Vec::new();
    collect_rules(&clean, &mut rules);
    rules
}

fn collect_rules(css: &str, rules: &mut Vec<(String, String)>) {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let prelude = rest[..open].trim();
        let mut depth = 1;
        let mut close = rest.len();
        for (offset, c) in rest[open + 1..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + 1 + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &rest[open + 1..close];
        if let Some(at) = prelude.strip_prefix('@') {
            if at.starts_with("supports") || at.starts_with("layer") {
                collect_rules(body, rules);
            }
        } else if !prelude.is_empty() {
            // A statement before the rule (`@import url(a.css);`) ends at its semicolon.
            let selectors = prelude.rsplit(';').next().unwrap_or(prelude).trim();
            rules.push((selectors.to_string(), body.to_string()));
        }
        rest = rest.get(close + 1..).unwrap_or("");
    }
}

/// The selectors of a comma-separated list, split outside brackets and parentheses.
fn split_selectors(list: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for c in list.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    parts.push(current.trim().to_string());
    parts
}

/// A selector's specificity as one number: ids, then classes, attributes and
/// pseudo-classes, then tags.
fn specificity(selector: &str) -> u32 {
    let (mut ids, mut classes, mut tags) = (0u32, 0u32, 0u32);
    let chars: Vec<char> = selector.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '#' => ids += 1,
            '.' | '[' | ':' => classes += 1,
            c if c.is_ascii_alphabetic() => {
                let starts_compound =
                    i == 0 || matches!(chars[i - 1], ' ' | '>' | '+' | '~' | '(' | ',');
                if starts_compound {
                    tags += 1;
                }
                while i + 1 < chars.len()
                    && (chars[i + 1].is_ascii_alphanumeric() || matches!(chars[i + 1], '-' | '_'))
                {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    ids.min(255) * 65536 + classes.min(255) * 256 + tags.min(255)
}

/// What a mouse, pointer or wheel event carries to its handler.
#[derive(Clone)]
pub(crate) struct EventData {
    client: (f64, f64),
    element: (f64, f64),
    modifiers: Modifiers,
    buttons: MouseButtonSet,
    trigger: Option<MouseButton>,
    pointer_id: i32,
    pointer_type: &'static str,
    delta: WheelDelta,
}

impl EventData {
    fn new(input: &PointerInput, client: (f32, f32), element: (f32, f32)) -> Self {
        let mut modifiers = Modifiers::empty();
        modifiers.set(Modifiers::SHIFT, input.modifiers & 1 != 0);
        modifiers.set(Modifiers::CONTROL, input.modifiers & 2 != 0);
        modifiers.set(Modifiers::ALT, input.modifiers & 4 != 0);
        modifiers.set(Modifiers::META, input.modifiers & 8 != 0);
        let delta = (f64::from(input.delta_x), f64::from(input.delta_y), 0.0);
        Self {
            client: (f64::from(client.0), f64::from(client.1)),
            element: (f64::from(element.0), f64::from(element.1)),
            modifiers,
            buttons: decode_mouse_button_set(input.buttons as u16),
            trigger: match input.button {
                0 => None,
                1 => Some(MouseButton::Primary),
                2 => Some(MouseButton::Secondary),
                4 => Some(MouseButton::Auxiliary),
                8 => Some(MouseButton::Fourth),
                16 => Some(MouseButton::Fifth),
                _ => Some(MouseButton::Unknown),
            },
            pointer_id: input.pointer_id as i32,
            pointer_type: match input.pointer_type {
                PointerType::Mouse => "mouse",
                PointerType::Touch => "touch",
                PointerType::Pen => "pen",
            },
            delta: match input.delta_mode {
                WheelMode::Pixel => WheelDelta::pixels(delta.0, delta.1, delta.2),
                WheelMode::Line => WheelDelta::lines(delta.0, delta.1, delta.2),
                WheelMode::Page => WheelDelta::pages(delta.0, delta.1, delta.2),
            },
        }
    }
}

impl InteractionLocation for EventData {
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

impl InteractionElementOffset for EventData {
    fn element_coordinates(&self) -> ElementPoint {
        ElementPoint::new(self.element.0, self.element.1)
    }
}

impl ModifiersInteraction for EventData {
    fn modifiers(&self) -> Modifiers {
        self.modifiers
    }
}

impl PointerInteraction for EventData {
    fn held_buttons(&self) -> MouseButtonSet {
        self.buttons
    }

    fn trigger_button(&self) -> Option<MouseButton> {
        self.trigger
    }
}

impl HasMouseData for EventData {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl HasPointerData for EventData {
    fn pointer_id(&self) -> i32 {
        self.pointer_id
    }

    fn width(&self) -> f64 {
        1.0
    }

    fn height(&self) -> f64 {
        1.0
    }

    fn pressure(&self) -> f32 {
        if self.buttons.is_empty() { 0.0 } else { 0.5 }
    }

    fn tangential_pressure(&self) -> f32 {
        0.0
    }

    fn tilt_x(&self) -> i32 {
        0
    }

    fn tilt_y(&self) -> i32 {
        0
    }

    fn twist(&self) -> i32 {
        0
    }

    fn pointer_type(&self) -> String {
        self.pointer_type.to_string()
    }

    fn is_primary(&self) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl HasWheelData for EventData {
    fn delta(&self) -> WheelDelta {
        self.delta
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
