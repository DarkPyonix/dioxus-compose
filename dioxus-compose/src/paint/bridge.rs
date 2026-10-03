//! A plan, drawn by compose-rust.
//!
//! [`PlanBridge`] keeps the renderer's tree in step with the latest [`Plan`]. The first plan
//! is written whole; after that the bridge asks [`diff`] what changed and writes only that:
//! a node created, moved or removed, a property or a modifier whose value is new. A node
//! nothing happened to costs no record.
//!
//! Every plan node becomes one compose-rust node of the widget its kind names
//! ([`widget_for`]). A few need nodes of their own that the plan does not list:
//!
//! - the page sits in a vertical scroll that fills the window, so a page taller than the
//!   window scrolls the way a browser's viewport does;
//! - an image is a box of the image's content-box size, clipped where the plan says so,
//!   holding an `Image` placed where `object-fit` put the picture;
//! - a dropdown's options are `Text` children, which is how compose-rust's `Dropdown` takes
//!   them;
//! - each background image or gradient is a box of its painting area, clipped, holding one
//!   box per tile painted with the brush. They come before the node's own children, so the
//!   content is drawn over them.
//!
//! Lengths are CSS pixels, which compose-rust draws as dp. Sides and corners are physical
//! (top, right, bottom, left), and an `Offset` is absolute, so a right-to-left page is not
//! mirrored a second time.

use std::collections::{BTreeMap, HashMap, HashSet};

use compose_rust::Batch;
use compose_rust::brush::{Brush as WireBrush, Stop, brush as register_brush};
use compose_rust::protocol::{Mutation, PropertyValue};
use compose_rust::schema::{
    Color, Modifier, Paint, PropertyKind, TextAlign as WireAlign, TextOverflow, TileMode,
    WidgetKind,
};

use crate::html::NodeId;
use crate::layout::measure::{TextLineHeight, TextStyle};
use crate::paint::display_list::{GradientStop, Rect, Rgba, TextAlign, TileRepeat};
use crate::paint::plan::{
    Brush, BrushId, Plan, PlanChange, PlanDropdown, PlanImage, PlanKey, PlanKind, PlanModifier,
    PlanNode, PlanTextField, diff,
};

/// The most tiles one background layer is drawn with. A tiny `background-size` repeated
/// over a large box would otherwise be thousands of nodes; past this many, the rest of the
/// layer is left unpainted.
const MAX_TILES: usize = 1024;

/// The line height `line-height: normal` is sent as, in multiples of the font size. The
/// renderer would otherwise use its design system's line height for the text, which is
/// not the height the page was laid out with.
const NORMAL_LINE_HEIGHT: f32 = 1.2;

/// What a letter spacing of zero is sent as. The renderer reads zero as "not set" and
/// falls back to its design system's spacing, which would make every run wider than the
/// width it was measured at. The smallest positive float is zero on screen and not zero
/// on the wire.
const ZERO_LETTER_SPACING: f32 = f32::MIN_POSITIVE;

/// What the page says about clicks and keys, which a plan does not carry.
pub trait ClickTargets {
    /// Whether a click on the box drawn for `node` does anything of its own.
    fn takes_clicks(&self, node: NodeId) -> bool;

    /// The element a click on a run of text is for: `owner`, the element the text belongs
    /// to, or the nearest element around it that takes clicks, short of `container`, the
    /// element whose box the run is drawn in. `None` when there is none, so a click on the
    /// run falls through to the box.
    fn run_target(&self, owner: NodeId, container: NodeId) -> Option<NodeId>;

    /// Whether a key pressed in the box drawn for `node`, or in anything inside it that
    /// has the focus, is for the page. A page that says nothing takes no keys there.
    fn takes_keys(&self, node: NodeId) -> bool {
        let _ = node;
        false
    }
}

/// What a renderer event on a node the bridge made means for the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeEvent {
    /// The user clicked a box that has a click handler, or that a click activates (a
    /// `<label>`, a `<button>`, a submit `<input>`).
    Click,
    /// The user changed the text of an `<input>` or `<textarea>`.
    TextChange,
    /// The user pressed Enter in a single-line field.
    TextSubmit,
    /// A field lost focus.
    FocusLost,
    /// The user pressed a key in a box whose element listens for keys, or in something
    /// inside it that has the focus.
    KeyDown,
    /// The user pressed a key in an `<input>` or `<textarea>`.
    FieldKeyDown,
    /// The user ticked or cleared a checkbox, or picked a radio button.
    Toggle,
    /// The user picked an option of a `<select>`.
    Choose,
}

/// One handler the bridge gave the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BridgeHandler {
    /// The renderer node the handler was set on. An event naming the handler from any
    /// other node is refused.
    pub node_id: u32,
    /// The DOM node the event is for.
    pub node: NodeId,
    pub event: BridgeEvent,
}

/// A property value as last sent, to tell whether the next one is new.
#[derive(Clone, Debug, PartialEq)]
enum Prop {
    None,
    Str(String),
    Bool(bool),
    Int(i64),
    Float(f32),
}

impl Prop {
    fn wire(&self) -> PropertyValue<'_> {
        match self {
            Prop::None => PropertyValue::None,
            Prop::Str(value) => PropertyValue::String(value),
            Prop::Bool(value) => PropertyValue::Bool(*value),
            Prop::Int(value) => PropertyValue::Integer(*value),
            Prop::Float(value) => PropertyValue::Float(*value),
        }
    }
}

/// One background layer as it is drawn: the painting area from the node's top-left
/// corner, the corners it is cut to, and each tile from the area's top-left corner.
#[derive(Clone, Debug, PartialEq)]
struct LayerSpec {
    area: Rect,
    corners: Option<[f32; 4]>,
    tiles: Vec<Rect>,
    paint: Paint,
}

/// A plan node the renderer has.
struct Placed {
    id: u32,
    parent: Option<PlanKey>,
    /// The kind it was last given.
    kind: PlanKind,
    /// The plan children, in the renderer's order. They come after `layers` and `extras`.
    children: Vec<PlanKey>,
    /// The modifier list the renderer holds, by index.
    modifiers: Vec<Modifier>,
    props: Vec<(PropertyKind, Prop)>,
    /// The background layer boxes, first among the node's renderer children.
    layers: Vec<u32>,
    layer_specs: Vec<LayerSpec>,
    /// Nodes the kind needs that the plan does not list: a dropdown's options, an image's
    /// picture. After the layers, before the plan children.
    extras: Vec<u32>,
    /// Every handler set on this node, the click handler included.
    handlers: Vec<u64>,
    /// The click handler, and the DOM node it is for.
    click: Option<(u64, NodeId)>,
    /// The key handler of a box whose element listens for keys, and that element. A
    /// field's key handler is set once, with its other handlers, and is not this.
    keys: Option<(u64, NodeId)>,
    /// What a text field shows: the last value the Host sent or the user typed, whichever
    /// came last.
    shown: String,
}

impl Placed {
    fn first_child_index(&self) -> usize {
        self.layers.len() + self.extras.len()
    }
}

/// Keeps a compose-rust tree in step with a sequence of plans.
pub struct PlanBridge {
    next_node_id: u32,
    next_handler_id: u64,
    /// The scroll the page is placed in.
    viewport_node: Option<u32>,
    nodes: HashMap<PlanKey, Placed>,
    handlers: HashMap<u64, BridgeHandler>,
    previous: Option<Plan>,
    /// The size the page is given: the window's width, and the larger of the window's
    /// height and the page's content.
    page_size: (f32, f32),
    /// Each brush as compose-rust paints it, for the tile size it fills. Gradient geometry
    /// is sent as fractions of the area filled, so one plan brush on tiles of two sizes is
    /// two compose-rust brushes.
    paints: HashMap<(BrushId, u32, u32), Paint>,
    /// The brushes of the plan being applied.
    brushes: BTreeMap<BrushId, Brush>,
}

impl Default for PlanBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanBridge {
    pub fn new() -> Self {
        Self {
            next_node_id: 1,
            next_handler_id: 1,
            viewport_node: None,
            nodes: HashMap::new(),
            handlers: HashMap::new(),
            previous: None,
            page_size: (0.0, 0.0),
            paints: HashMap::new(),
            brushes: BTreeMap::new(),
        }
    }

    /// The handler a renderer event names, if the bridge gave it out.
    pub fn handler(&self, handler_id: u64) -> Option<BridgeHandler> {
        self.handlers.get(&handler_id).copied()
    }

    /// The renderer node drawn for a plan node.
    pub fn node_id(&self, key: PlanKey) -> Option<u32> {
        self.nodes.get(&key).map(|placed| placed.id)
    }

    /// Records the text the user typed into the field of `node`, so that the same text
    /// coming back from the page is not sent to the renderer again. The field owns its
    /// text while the user edits it.
    pub fn user_typed(&mut self, node: NodeId, text: &str) {
        if let Some(placed) = self.nodes.get_mut(&PlanKey::Field(node)) {
            placed.shown.clear();
            placed.shown.push_str(text);
        }
    }

    /// Writes into `batch` what turns the renderer's tree into `plan`: everything on the
    /// first call, what changed since the previous plan after that.
    ///
    /// `viewport` is the window's size in dp. `clicks` says which nodes take clicks; the
    /// box drawn for one, or a run of text it owns, is given a `Clickable` modifier whose
    /// handler names it.
    pub fn apply(
        &mut self,
        plan: Plan,
        viewport: (f32, f32),
        clicks: &dyn ClickTargets,
        batch: &mut Batch,
    ) {
        self.page_size = (viewport.0, viewport.1.max(page_extent(&plan.root)));
        self.brushes.clone_from(&plan.brushes);
        match self.previous.take() {
            None => {
                let viewport_node = self.create(batch, WidgetKind::ScrollColumn);
                batch.write(Mutation::SetModifier {
                    node_id: viewport_node,
                    index: 0,
                    modifier: Modifier::FillMaxWidth,
                });
                batch.write(Mutation::SetModifier {
                    node_id: viewport_node,
                    index: 1,
                    modifier: Modifier::FillMaxHeight,
                });
                self.viewport_node = Some(viewport_node);
                let page = self.build(batch, &plan.root, None, clicks);
                batch.write(Mutation::Insert {
                    parent_id: viewport_node,
                    node_id: page,
                    index: 0,
                });
            }
            Some(previous) => {
                self.apply_changes(batch, &previous, &plan, clicks);
                // Brushes the plan no longer uses. compose-rust keeps a registration for
                // the life of the process and sends it once, so only the bridge's own
                // record of them goes.
                let gone: HashSet<BrushId> = previous
                    .brushes_not_in(&plan)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect();
                if !gone.is_empty() {
                    self.paints.retain(|(id, _, _), _| !gone.contains(id));
                }
            }
        }
        self.previous = Some(plan);
    }

    fn apply_changes(
        &mut self,
        batch: &mut Batch,
        previous: &Plan,
        plan: &Plan,
        clicks: &dyn ClickTargets,
    ) {
        let mut dirty: HashSet<PlanKey> = HashSet::new();
        for change in diff(previous, plan) {
            match change {
                PlanChange::Removed { key } => self.remove(batch, key),
                PlanChange::Reordered { parent, order } => self.reorder(batch, parent, &order),
                PlanChange::Inserted {
                    parent,
                    index,
                    node,
                } => {
                    if node.key == parent {
                        // A different root: nothing carries over.
                        self.replace_page(batch, &node, clicks);
                    } else {
                        self.insert(batch, parent, index, &node, clicks);
                    }
                }
                PlanChange::KindChanged { key, kind } => self.set_kind(batch, key, &kind, false),
                PlanChange::ModifierSet { key, .. } | PlanChange::ModifierRemoved { key, .. } => {
                    dirty.insert(key);
                }
            }
        }

        // A handler added to or taken off a box changes nothing in the plan, and the page's
        // size follows the window.
        let nodes = plan.nodes();
        for node in &nodes {
            if let Some(placed) = self.nodes.get(&node.key)
                && (placed.click.map(|(_, target)| target) != click_target(node, clicks)
                    || placed.keys.map(|(_, target)| target) != key_target(node, clicks))
            {
                dirty.insert(node.key);
            }
        }
        dirty.insert(PlanKey::Page);

        for node in nodes {
            if dirty.contains(&node.key) {
                self.decorate(batch, node, clicks);
            }
        }
    }

    fn create(&mut self, batch: &mut Batch, widget: WidgetKind) -> u32 {
        let node_id = self.next_node_id;
        self.next_node_id = self.next_node_id.checked_add(1).unwrap_or(1);
        batch.write(Mutation::Create { node_id, widget });
        node_id
    }

    /// Creates the renderer nodes for `node` and everything inside it, and returns the
    /// node's id. The caller inserts it.
    fn build(
        &mut self,
        batch: &mut Batch,
        node: &PlanNode,
        parent: Option<PlanKey>,
        clicks: &dyn ClickTargets,
    ) -> u32 {
        let id = self.create(batch, widget_for(&node.kind));
        self.nodes.insert(
            node.key,
            Placed {
                id,
                parent,
                kind: node.kind.clone(),
                children: Vec::new(),
                modifiers: Vec::new(),
                props: Vec::new(),
                layers: Vec::new(),
                layer_specs: Vec::new(),
                extras: Vec::new(),
                handlers: Vec::new(),
                click: None,
                keys: None,
                shown: String::new(),
            },
        );
        self.set_kind(batch, node.key, &node.kind, true);
        self.decorate(batch, node, clicks);
        for child in &node.children {
            let child_id = self.build(batch, child, Some(node.key), clicks);
            let placed = self
                .nodes
                .get_mut(&node.key)
                .expect("the node was placed above");
            let index = placed.first_child_index() + placed.children.len();
            placed.children.push(child.key);
            batch.write(Mutation::Insert {
                parent_id: id,
                node_id: child_id,
                index: index as u32,
            });
        }
        id
    }

    fn insert(
        &mut self,
        batch: &mut Batch,
        parent: PlanKey,
        index: usize,
        node: &PlanNode,
        clicks: &dyn ClickTargets,
    ) {
        if !self.nodes.contains_key(&parent) {
            return;
        }
        let child_id = self.build(batch, node, Some(parent), clicks);
        let placed = self
            .nodes
            .get_mut(&parent)
            .expect("checked at the top of this function");
        let index = index.min(placed.children.len());
        placed.children.insert(index, node.key);
        let parent_id = placed.id;
        let position = placed.first_child_index() + index;
        batch.write(Mutation::Insert {
            parent_id,
            node_id: child_id,
            index: position as u32,
        });
    }

    fn replace_page(&mut self, batch: &mut Batch, page: &PlanNode, clicks: &dyn ClickTargets) {
        let old: Vec<PlanKey> = self
            .nodes
            .iter()
            .filter(|(_, placed)| placed.parent.is_none())
            .map(|(key, _)| *key)
            .collect();
        for key in old {
            self.remove(batch, key);
        }
        let Some(viewport_node) = self.viewport_node else {
            return;
        };
        let id = self.build(batch, page, None, clicks);
        batch.write(Mutation::Insert {
            parent_id: viewport_node,
            node_id: id,
            index: 0,
        });
    }

    /// Removes the node and everything inside it, on both sides.
    fn remove(&mut self, batch: &mut Batch, key: PlanKey) {
        let Some(placed) = self.nodes.get(&key) else {
            return;
        };
        let (id, parent) = (placed.id, placed.parent);
        batch.write(Mutation::Remove { node_id: id });
        if let Some(parent) = parent.and_then(|parent| self.nodes.get_mut(&parent)) {
            parent.children.retain(|child| *child != key);
        }
        self.forget(key);
    }

    fn forget(&mut self, key: PlanKey) {
        let Some(placed) = self.nodes.remove(&key) else {
            return;
        };
        for handler in placed.handlers {
            self.handlers.remove(&handler);
        }
        for child in placed.children {
            self.forget(child);
        }
    }

    /// Puts the children `parent` kept into `order`, moving as few as it takes.
    fn reorder(&mut self, batch: &mut Batch, parent: PlanKey, order: &[PlanKey]) {
        let Some(placed) = self.nodes.get(&parent) else {
            return;
        };
        let parent_id = placed.id;
        let base = placed.first_child_index();
        let mut current = placed.children.clone();
        current.retain(|key| order.contains(key));
        let mut moves = Vec::new();
        for (index, key) in order.iter().enumerate() {
            if current.get(index) == Some(key) {
                continue;
            }
            let Some(from) = current.iter().position(|child| child == key) else {
                continue;
            };
            let moved = current.remove(from);
            current.insert(index, moved);
            if let Some(child) = self.nodes.get(key) {
                moves.push((child.id, base + index));
            }
        }
        for (node_id, index) in moves {
            batch.write(Mutation::Move {
                parent_id,
                node_id,
                index: index as u32,
            });
        }
        if let Some(placed) = self.nodes.get_mut(&parent) {
            placed.children = current;
        }
    }

    /// Sets a property, unless the renderer already has that value.
    fn set_prop(&mut self, batch: &mut Batch, key: PlanKey, property: PropertyKind, value: Prop) {
        let Some(placed) = self.nodes.get_mut(&key) else {
            return;
        };
        match placed.props.iter().position(|(kind, _)| *kind == property) {
            Some(at) if placed.props[at].1 == value => return,
            Some(at) => placed.props[at].1 = value.clone(),
            // A property never set reads as unset on the other side already.
            None if value == Prop::None => return,
            None => placed.props.push((property, value.clone())),
        }
        batch.write(Mutation::SetProp {
            node_id: placed.id,
            property,
            value: value.wire(),
        });
    }

    fn add_handler(
        &mut self,
        batch: &mut Batch,
        key: PlanKey,
        property: PropertyKind,
        event: BridgeEvent,
    ) {
        let Some(node) = key.node() else {
            return;
        };
        let Some(placed) = self.nodes.get_mut(&key) else {
            return;
        };
        let handler_id = self.next_handler_id;
        self.next_handler_id += 1;
        placed.handlers.push(handler_id);
        self.handlers.insert(
            handler_id,
            BridgeHandler {
                node_id: placed.id,
                node,
                event,
            },
        );
        batch.write(Mutation::SetProp {
            node_id: placed.id,
            property,
            value: PropertyValue::Integer(handler_id as i64),
        });
    }

    /// Sends what a node's kind carries: a run's text and style, a field's value, whether
    /// a box is ticked, the options of a dropdown, the picture of an image.
    fn set_kind(&mut self, batch: &mut Batch, key: PlanKey, kind: &PlanKind, created: bool) {
        let Some(placed) = self.nodes.get(&key) else {
            return;
        };
        let previous = placed.kind.clone();
        match kind {
            PlanKind::AbsoluteBox
            | PlanKind::Box
            | PlanKind::ScrollColumn
            | PlanKind::ScrollRow => {}
            PlanKind::Text(text) => {
                self.set_prop(batch, key, PropertyKind::Text, Prop::Str(text.text.clone()));
                self.set_text_style(batch, key, &text.style, text.color);
                self.set_prop(
                    batch,
                    key,
                    PropertyKind::TextAlign,
                    Prop::Int(wire_align(text.align) as i64),
                );
                // A run laid out as unconstrained lines is drawn without wrapping: as many
                // lines as it has forced breaks, overflowing its width rather than wrapping
                // where the renderer's own measurement comes out a little wider.
                let max_lines = if text.soft_wrap {
                    Prop::None
                } else {
                    Prop::Int(text.text.matches('\n').count() as i64 + 1)
                };
                self.set_prop(batch, key, PropertyKind::MaxLines, max_lines);
                self.set_prop(
                    batch,
                    key,
                    PropertyKind::Overflow,
                    Prop::Int(TextOverflow::Visible as i64),
                );
            }
            PlanKind::TextField(field) => self.set_field(batch, key, field, &previous, created),
            PlanKind::Checkbox { checked } | PlanKind::RadioButton { selected: checked } => {
                self.set_prop(batch, key, PropertyKind::Checked, Prop::Bool(*checked));
                if created {
                    self.add_handler(batch, key, PropertyKind::OnValueChange, BridgeEvent::Toggle);
                }
            }
            PlanKind::Dropdown(dropdown) => {
                self.set_dropdown(batch, key, dropdown, &previous, created)
            }
            PlanKind::Image(image) => self.set_image(batch, key, image, &previous, created),
        }
        if let Some(placed) = self.nodes.get_mut(&key) {
            placed.kind = kind.clone();
        }
    }

    fn set_text_style(&mut self, batch: &mut Batch, key: PlanKey, style: &TextStyle, color: Rgba) {
        for (property, value) in text_style(style, color) {
            self.set_prop(batch, key, property, value);
        }
    }

    fn set_field(
        &mut self,
        batch: &mut Batch,
        key: PlanKey,
        field: &PlanTextField,
        previous: &PlanKind,
        created: bool,
    ) {
        if created {
            self.set_prop(
                batch,
                key,
                PropertyKind::Text,
                Prop::Str(field.value.clone()),
            );
            if let Some(placed) = self.nodes.get_mut(&key) {
                placed.shown = field.value.clone();
            }
            self.add_handler(
                batch,
                key,
                PropertyKind::OnValueChange,
                BridgeEvent::TextChange,
            );
            self.add_handler(
                batch,
                key,
                PropertyKind::OnFocusLost,
                BridgeEvent::FocusLost,
            );
            if !field.multiline {
                self.add_handler(batch, key, PropertyKind::OnSubmit, BridgeEvent::TextSubmit);
            }
            // Keys pressed in a field go to the page whether or not anything listens yet:
            // the renderer asks before Enter submits or starts a line, and the answer
            // depends on handlers that may be added later.
            self.add_handler(
                batch,
                key,
                PropertyKind::OnKeyDown,
                BridgeEvent::FieldKeyDown,
            );
        } else if let Some(placed) = self.nodes.get_mut(&key) {
            // The renderer owns the text while the user edits it. The Host's value goes out
            // only when the app set a new one, and not when it is what the field already
            // shows, which is what an app that mirrors its field into its state sends back.
            let host_changed =
                !matches!(previous, PlanKind::TextField(before) if before.value == field.value);
            if host_changed && placed.shown != field.value {
                placed.shown = field.value.clone();
                batch.write(Mutation::SetText {
                    node_id: placed.id,
                    text: &field.value,
                    selection: None,
                });
            }
        }
        let placeholder = field
            .placeholder
            .as_ref()
            .map_or(Prop::None, |text| Prop::Str(text.clone()));
        self.set_prop(batch, key, PropertyKind::Placeholder, placeholder);
        self.set_prop(
            batch,
            key,
            PropertyKind::Multiline,
            Prop::Bool(field.multiline),
        );
        self.set_text_style(batch, key, &field.style, field.color);
    }

    fn set_dropdown(
        &mut self,
        batch: &mut Batch,
        key: PlanKey,
        dropdown: &PlanDropdown,
        previous: &PlanKind,
        created: bool,
    ) {
        if created {
            self.add_handler(batch, key, PropertyKind::OnValueChange, BridgeEvent::Choose);
        }
        let selected = dropdown
            .selected
            .map_or(Prop::None, |index| Prop::Int(index as i64));
        self.set_prop(batch, key, PropertyKind::SelectedIndex, selected);

        let same_options = matches!(previous, PlanKind::Dropdown(before)
            if before.options == dropdown.options
                && before.style == dropdown.style
                && before.color == dropdown.color);
        if same_options && !created {
            return;
        }
        let Some(placed) = self.nodes.get_mut(&key) else {
            return;
        };
        let parent_id = placed.id;
        let base = placed.layers.len();
        let old = std::mem::take(&mut placed.extras);
        for option in old {
            batch.write(Mutation::Remove { node_id: option });
        }
        let mut extras = Vec::with_capacity(dropdown.options.len());
        for (index, label) in dropdown.options.iter().enumerate() {
            let option = self.create(batch, WidgetKind::Text);
            batch.write(Mutation::SetProp {
                node_id: option,
                property: PropertyKind::Text,
                value: PropertyValue::String(label),
            });
            for (property, value) in text_style(&dropdown.style, dropdown.color) {
                batch.write(Mutation::SetProp {
                    node_id: option,
                    property,
                    value: value.wire(),
                });
            }
            batch.write(Mutation::Insert {
                parent_id,
                node_id: option,
                index: (base + index) as u32,
            });
            extras.push(option);
        }
        if let Some(placed) = self.nodes.get_mut(&key) {
            placed.extras = extras;
        }
    }

    fn set_image(
        &mut self,
        batch: &mut Batch,
        key: PlanKey,
        image: &PlanImage,
        previous: &PlanKind,
        created: bool,
    ) {
        let Some(placed) = self.nodes.get(&key) else {
            return;
        };
        let parent_id = placed.id;
        let base = placed.layers.len();
        let existing = placed.extras.first().copied();
        let picture = match existing {
            Some(picture) if !created => picture,
            _ => {
                let picture = self.create(batch, WidgetKind::Image);
                batch.write(Mutation::Insert {
                    parent_id,
                    node_id: picture,
                    index: base as u32,
                });
                if let Some(placed) = self.nodes.get_mut(&key) {
                    placed.extras = vec![picture];
                }
                picture
            }
        };
        let before = match previous {
            PlanKind::Image(before) if !created => Some(before),
            _ => None,
        };
        if before.is_none_or(|before| before.asset != image.asset) {
            batch.write(Mutation::SetProp {
                node_id: picture,
                property: PropertyKind::Asset,
                value: PropertyValue::Integer(i64::from(image.asset.0)),
            });
        }
        // The renderer draws a picture whole and centred in its node, so the node is the
        // rectangle `object-fit` gave the picture, and the box around it clips.
        if before.is_none_or(|before| before.draw != image.draw) {
            batch.write(Mutation::SetModifier {
                node_id: picture,
                index: 0,
                modifier: Modifier::Offset {
                    x: image.draw.x,
                    y: image.draw.y,
                },
            });
            batch.write(Mutation::SetModifier {
                node_id: picture,
                index: 1,
                modifier: Modifier::RequiredSize {
                    width: image.draw.width,
                    height: image.draw.height,
                },
            });
        }
    }

    /// Brings a node's modifiers and background layers up to date with the plan.
    fn decorate(&mut self, batch: &mut Batch, node: &PlanNode, clicks: &dyn ClickTargets) {
        let key = node.key;
        let target = click_target(node, clicks);
        let Some(placed) = self.nodes.get_mut(&key) else {
            return;
        };
        let click = match (placed.click, target) {
            (Some((handler_id, current)), Some(target)) if current == target => Some(handler_id),
            (old, target) => {
                if let Some((handler_id, _)) = old {
                    placed.handlers.retain(|handler| *handler != handler_id);
                    self.handlers.remove(&handler_id);
                }
                placed.click = None;
                if let Some(target) = target {
                    let handler_id = self.next_handler_id;
                    self.next_handler_id += 1;
                    placed.handlers.push(handler_id);
                    placed.click = Some((handler_id, target));
                    self.handlers.insert(
                        handler_id,
                        BridgeHandler {
                            node_id: placed.id,
                            node: target,
                            event: BridgeEvent::Click,
                        },
                    );
                }
                placed.click.map(|(handler_id, _)| handler_id)
            }
        };

        let keys = key_target(node, clicks);
        if placed.keys.map(|(_, target)| target) != keys {
            if let Some((handler_id, _)) = placed.keys.take() {
                placed.handlers.retain(|handler| *handler != handler_id);
                self.handlers.remove(&handler_id);
            }
            let value = match keys {
                Some(target) => {
                    let handler_id = self.next_handler_id;
                    self.next_handler_id += 1;
                    placed.handlers.push(handler_id);
                    placed.keys = Some((handler_id, target));
                    self.handlers.insert(
                        handler_id,
                        BridgeHandler {
                            node_id: placed.id,
                            node: target,
                            event: BridgeEvent::KeyDown,
                        },
                    );
                    PropertyValue::Integer(handler_id as i64)
                }
                None => PropertyValue::None,
            };
            batch.write(Mutation::SetProp {
                node_id: placed.id,
                property: PropertyKind::OnKeyDown,
                value,
            });
        }

        let mut modifiers = wire_modifiers(node);
        if key == PlanKey::Page {
            modifiers.insert(
                0,
                Modifier::RequiredSize {
                    width: self.page_size.0,
                    height: self.page_size.1,
                },
            );
        }
        if let Some(handler_id) = click {
            modifiers.push(Modifier::Clickable { handler_id });
        }
        let node_id = placed.id;
        for (index, modifier) in modifiers.iter().enumerate() {
            if placed.modifiers.get(index) != Some(modifier) {
                batch.write(Mutation::SetModifier {
                    node_id,
                    index: index as u16,
                    modifier: modifier.clone(),
                });
            }
        }
        for (index, old) in placed.modifiers.iter().enumerate().skip(modifiers.len()) {
            if *old != Modifier::Empty {
                batch.write(Mutation::SetModifier {
                    node_id,
                    index: index as u16,
                    modifier: Modifier::Empty,
                });
            }
        }
        placed.modifiers = modifiers;

        let specs = self.layer_specs(node);
        let Some(placed) = self.nodes.get_mut(&key) else {
            return;
        };
        if placed.layer_specs == specs {
            return;
        }
        for layer in std::mem::take(&mut placed.layers) {
            batch.write(Mutation::Remove { node_id: layer });
        }
        let mut layers = Vec::with_capacity(specs.len());
        for (index, spec) in specs.iter().enumerate() {
            let layer = self.build_layer(batch, spec);
            batch.write(Mutation::Insert {
                parent_id: node_id,
                node_id: layer,
                index: index as u32,
            });
            layers.push(layer);
        }
        if let Some(placed) = self.nodes.get_mut(&key) {
            placed.layers = layers;
            placed.layer_specs = specs;
        }
    }

    /// The background layers of a node, back to front, with each brush registered.
    fn layer_specs(&mut self, node: &PlanNode) -> Vec<LayerSpec> {
        let mut specs = Vec::new();
        let size = node_size(node);
        let corners = node_corners(node);
        for modifier in &node.modifiers {
            let PlanModifier::BackgroundBrush {
                brush,
                area,
                tile,
                repeat_x,
                repeat_y,
            } = modifier
            else {
                continue;
            };
            let Some(source) = self.brushes.get(brush) else {
                continue;
            };
            if tile.width <= 0.0 || tile.height <= 0.0 || area.width <= 0.0 || area.height <= 0.0 {
                continue;
            }
            let paint = *self
                .paints
                .entry((*brush, tile.width.to_bits(), tile.height.to_bits()))
                .or_insert_with(|| register_brush(wire_brush(source, tile.width, tile.height)));
            let covers_node = size.is_some_and(|(width, height)| {
                close(area.x, 0.0)
                    && close(area.y, 0.0)
                    && close(area.width, width)
                    && close(area.height, height)
            });
            specs.push(LayerSpec {
                area: *area,
                corners: corners.filter(|_| covers_node),
                tiles: tiles(area, tile, *repeat_x, *repeat_y),
                paint,
            });
        }
        specs
    }

    fn build_layer(&mut self, batch: &mut Batch, spec: &LayerSpec) -> u32 {
        let layer = self.create(batch, WidgetKind::AbsoluteBox);
        let mut modifiers = vec![
            Modifier::Offset {
                x: spec.area.x,
                y: spec.area.y,
            },
            Modifier::RequiredSize {
                width: spec.area.width,
                height: spec.area.height,
            },
        ];
        if let Some([top_left, top_right, bottom_right, bottom_left]) = spec.corners {
            modifiers.push(Modifier::CornerEach {
                top_left,
                top_right,
                bottom_right,
                bottom_left,
            });
        }
        modifiers.push(Modifier::Clip(true));
        for (index, modifier) in modifiers.into_iter().enumerate() {
            batch.write(Mutation::SetModifier {
                node_id: layer,
                index: index as u16,
                modifier,
            });
        }
        for (index, tile) in spec.tiles.iter().enumerate() {
            let tile_node = self.create(batch, WidgetKind::Box);
            let modifiers = [
                Modifier::Offset {
                    x: tile.x,
                    y: tile.y,
                },
                Modifier::RequiredSize {
                    width: tile.width,
                    height: tile.height,
                },
                Modifier::Background(spec.paint),
            ];
            for (slot, modifier) in modifiers.into_iter().enumerate() {
                batch.write(Mutation::SetModifier {
                    node_id: tile_node,
                    index: slot as u16,
                    modifier,
                });
            }
            batch.write(Mutation::Insert {
                parent_id: layer,
                node_id: tile_node,
                index: index as u32,
            });
        }
        layer
    }
}

/// How close two lengths must be to count as equal, in CSS pixels.
const EPSILON: f32 = 0.01;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < EPSILON
}

/// The DOM node a click on a plan node is for, if a click there does anything.
fn click_target(node: &PlanNode, clicks: &dyn ClickTargets) -> Option<NodeId> {
    match (node.key, &node.kind) {
        (PlanKey::Node(dom), _) => clicks.takes_clicks(dom).then_some(dom),
        (
            PlanKey::Text {
                node: container, ..
            },
            PlanKind::Text(text),
        ) => clicks.run_target(text.owner, container),
        _ => None,
    }
}

/// The DOM node a key pressed in the box of a plan node is for, if the page takes keys
/// there. Only an element's own box, and not the box of an `<input>` or `<textarea>`
/// drawn as a text field: the field routes its keys itself, and a second handler around
/// it would offer the page the same key twice. A run of text cannot hold the focus.
fn key_target(node: &PlanNode, clicks: &dyn ClickTargets) -> Option<NodeId> {
    match node.key {
        PlanKey::Node(dom) if !draws_text_field(node, dom) => clicks.takes_keys(dom).then_some(dom),
        _ => None,
    }
}

/// Whether the text field of the element `dom` is inside `node`.
fn draws_text_field(node: &PlanNode, dom: NodeId) -> bool {
    node.children.iter().any(|child| {
        (child.key == PlanKey::Field(dom) && matches!(child.kind, PlanKind::TextField(_)))
            || draws_text_field(child, dom)
    })
}

/// The widget a plan node is drawn with.
pub fn widget_for(kind: &PlanKind) -> WidgetKind {
    match kind {
        PlanKind::AbsoluteBox => WidgetKind::AbsoluteBox,
        PlanKind::Box => WidgetKind::Box,
        PlanKind::Text(_) => WidgetKind::Text,
        // The box the picture is clipped to. The picture is an `Image` inside it.
        PlanKind::Image(_) => WidgetKind::AbsoluteBox,
        PlanKind::ScrollColumn => WidgetKind::ScrollColumn,
        PlanKind::ScrollRow => WidgetKind::ScrollRow,
        PlanKind::TextField(_) => WidgetKind::TextField,
        PlanKind::Checkbox { .. } => WidgetKind::Checkbox,
        PlanKind::RadioButton { .. } => WidgetKind::RadioButton,
        PlanKind::Dropdown(_) => WidgetKind::Dropdown,
    }
}

/// A colour as compose-rust's `0xAARRGGBB`.
fn argb(color: Rgba) -> Color {
    Color::argb(
        (u32::from(color.a) << 24)
            | (u32::from(color.r) << 16)
            | (u32::from(color.g) << 8)
            | u32::from(color.b),
    )
}

/// A colour as compose-rust paints it.
fn literal(color: Rgba) -> Paint {
    Paint::Literal(argb(color))
}

/// A node's modifiers as compose-rust takes them, in the plan's order, which is the order
/// they apply in: where the node goes and how big it is, its opacity, its corners, its
/// shadows, its background, its border, and its clip.
///
/// Background images and gradients are not here: they are boxes inside the node.
fn wire_modifiers(node: &PlanNode) -> Vec<Modifier> {
    let mut out = Vec::with_capacity(node.modifiers.len() + 1);
    for modifier in &node.modifiers {
        let wire = match modifier {
            PlanModifier::Offset { x, y } => Modifier::Offset { x: *x, y: *y },
            PlanModifier::RequiredSize { width, height } => Modifier::RequiredSize {
                width: *width,
                height: *height,
            },
            PlanModifier::Width(width) => Modifier::Width(*width),
            PlanModifier::Alpha(alpha) => Modifier::Alpha(*alpha),
            // Corners always go per corner, even when all four are the same. compose-rust's
            // `Shape` is cut the way the design system cuts corners and clips the content
            // to it; CSS `border-radius` is a plain arc and clips nothing unless `overflow`
            // says so, which is what `CornerEach` and a separate `Clip` are.
            PlanModifier::Shape { radius } => Modifier::CornerEach {
                top_left: *radius,
                top_right: *radius,
                bottom_right: *radius,
                bottom_left: *radius,
            },
            PlanModifier::CornerEach(corners) => Modifier::CornerEach {
                top_left: corners.top_left,
                top_right: corners.top_right,
                bottom_right: corners.bottom_right,
                bottom_left: corners.bottom_left,
            },
            PlanModifier::Shadow {
                x,
                y,
                blur,
                spread,
                color,
            } => Modifier::Shadow {
                x: *x,
                y: *y,
                blur: *blur,
                spread: *spread,
                paint: literal(*color),
            },
            PlanModifier::Background(color) => Modifier::Background(literal(*color)),
            PlanModifier::BackgroundBrush { .. } => continue,
            PlanModifier::Border { width, color } => Modifier::Border {
                width: *width,
                paint: literal(*color),
            },
            PlanModifier::BorderEach(sides) => Modifier::BorderEach {
                top: sides.top.width,
                right: sides.right.width,
                bottom: sides.bottom.width,
                left: sides.left.width,
                top_paint: literal(sides.top.color),
                right_paint: literal(sides.right.color),
                bottom_paint: literal(sides.bottom.color),
                left_paint: literal(sides.left.color),
            },
            PlanModifier::Clip => Modifier::Clip(true),
        };
        out.push(wire);
    }
    // The plan lists a box's shadows in CSS order, where the first is drawn on top. A
    // modifier list draws in its own order, so they go last to first.
    let shadows: Vec<usize> = out
        .iter()
        .enumerate()
        .filter(|(_, modifier)| matches!(modifier, Modifier::Shadow { .. }))
        .map(|(index, _)| index)
        .collect();
    let reversed: Vec<Modifier> = shadows
        .iter()
        .rev()
        .map(|&index| out[index].clone())
        .collect();
    for (&index, modifier) in shadows.iter().zip(reversed) {
        out[index] = modifier;
    }
    out
}

/// A node's `RequiredSize`, if it has one.
fn node_size(node: &PlanNode) -> Option<(f32, f32)> {
    node.modifiers.iter().find_map(|modifier| match modifier {
        PlanModifier::RequiredSize { width, height } => Some((*width, *height)),
        _ => None,
    })
}

/// A node's corner radii (top left, top right, bottom right, bottom left), if it has any.
fn node_corners(node: &PlanNode) -> Option<[f32; 4]> {
    node.modifiers.iter().find_map(|modifier| match modifier {
        PlanModifier::Shape { radius } => Some([*radius; 4]),
        PlanModifier::CornerEach(corners) => Some([
            corners.top_left,
            corners.top_right,
            corners.bottom_right,
            corners.bottom_left,
        ]),
        _ => None,
    })
}

/// How far down the page's boxes reach.
fn page_extent(page: &PlanNode) -> f32 {
    page.children
        .iter()
        .filter_map(|child| {
            let (_, y) = child.offset();
            node_size(child).map(|(_, height)| y + height)
        })
        .fold(0.0, f32::max)
}

/// The properties a run of text, a field or a dropdown option is styled with.
///
/// Every one is sent, because the renderer fills an unsent one from its design system,
/// and text drawn in the design system's size, height or spacing would not fit the box
/// the page laid out for it. The family and the slant are not among them: compose-rust's
/// text takes neither, so the renderer draws the design system's face.
fn text_style(style: &TextStyle, color: Rgba) -> [(PropertyKind, Prop); 5] {
    let line_height = match style.line_height {
        TextLineHeight::Px(px) => px,
        TextLineHeight::Normal => style.font_size * NORMAL_LINE_HEIGHT,
    };
    let letter_spacing = if style.letter_spacing == 0.0 {
        ZERO_LETTER_SPACING
    } else {
        style.letter_spacing
    };
    [
        (PropertyKind::FontSize, Prop::Float(style.font_size)),
        (
            PropertyKind::FontWeight,
            Prop::Int(style.font_weight.round() as i64),
        ),
        (PropertyKind::LineHeight, Prop::Float(line_height)),
        (PropertyKind::LetterSpacing, Prop::Float(letter_spacing)),
        (
            PropertyKind::Color,
            Prop::Int(literal(color).to_bits() as i64),
        ),
    ]
}

/// compose-rust aligns by reading direction only. `left` and `right` are sent as start and
/// end, which is where they are on a left-to-right page.
fn wire_align(align: TextAlign) -> WireAlign {
    match align {
        TextAlign::Start | TextAlign::Left => WireAlign::Start,
        TextAlign::End | TextAlign::Right => WireAlign::End,
        TextAlign::Center => WireAlign::Center,
        TextAlign::Justify => WireAlign::Justify,
    }
}

/// A plan brush as compose-rust's, for a tile `width` by `height`.
///
/// The plan measures gradient geometry in pixels from the tile's top-left corner;
/// compose-rust measures it in fractions of the area it fills. A radial gradient's radius
/// is a fraction of the larger side there, and it is round: an elliptical CSS gradient is
/// drawn as the circle of its horizontal radius, which is the radius its stop offsets are
/// fractions of.
fn wire_brush(brush: &Brush, width: f32, height: f32) -> WireBrush {
    let stops = |stops: &[GradientStop]| -> Vec<Stop> {
        stops
            .iter()
            .map(|stop| Stop::new(stop.offset, argb(stop.color)))
            .collect()
    };
    let tile = |repeating: bool| {
        if repeating {
            TileMode::Repeat
        } else {
            TileMode::Clamp
        }
    };
    match brush {
        Brush::LinearGradient {
            start,
            end,
            stops: list,
            repeating,
        } => WireBrush::Linear {
            start: (start.0 / width, start.1 / height),
            end: (end.0 / width, end.1 / height),
            stops: stops(list),
            tile: tile(*repeating),
        },
        Brush::RadialGradient {
            center,
            radius_x,
            radius_y: _,
            stops: list,
            repeating,
        } => WireBrush::Radial {
            center: (center.0 / width, center.1 / height),
            radius: radius_x / width.max(height),
            stops: stops(list),
            tile: tile(*repeating),
        },
        Brush::Image(asset) => WireBrush::Image {
            asset: asset.0,
            tile: TileMode::Clamp,
        },
    }
}

/// Where each tile of a background layer goes, from the painting area's top-left corner.
fn tiles(area: &Rect, tile: &Rect, repeat_x: TileRepeat, repeat_y: TileRepeat) -> Vec<Rect> {
    let xs = positions(tile.x, tile.width, area.x, area.right(), repeat_x);
    let ys = positions(tile.y, tile.height, area.y, area.bottom(), repeat_y);
    let mut out = Vec::new();
    'rows: for &y in &ys {
        for &x in &xs {
            if out.len() == MAX_TILES {
                break 'rows;
            }
            out.push(Rect::new(x - area.x, y - area.y, tile.width, tile.height));
        }
    }
    out
}

/// Where tiles of `size` start along one axis, for a tile placed at `start` in an area
/// from `from` to `to`.
fn positions(start: f32, size: f32, from: f32, to: f32, repeat: TileRepeat) -> Vec<f32> {
    let mut out = Vec::new();
    match repeat {
        TileRepeat::NoRepeat => out.push(start),
        TileRepeat::Repeat => {
            // Back from the placed tile to the first one that reaches into the area.
            let mut at = start - ((start - from) / size).ceil() * size;
            while at < to - EPSILON && out.len() < MAX_TILES {
                out.push(at);
                at += size;
            }
        }
        TileRepeat::Space { gap } => {
            let step = size + gap.max(0.0);
            let mut at = start;
            while at + size <= to + EPSILON && out.len() < MAX_TILES {
                out.push(at);
                at += step;
            }
            if out.is_empty() {
                out.push(start);
            }
        }
    }
    out
}
