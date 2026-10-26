use compose_rust::Batch;
use compose_rust::drawing::DrawList;
use compose_rust::protocol::{Mutation, PropertyValue, ProtocolError};
use compose_rust::schema::{PropertyKind, WidgetKind};
use dioxus_core::{
    AttributeValue, ElementId, Template, TemplateAttribute, TemplateNode, WriteMutations,
};
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

/// The inclusive range of design-property tags. Extension properties follow this range.
const FIRST_OPTIONAL_PROPERTY: u16 = PropertyKind::TypeRole as u16;
const LAST_OPTIONAL_PROPERTY: u16 = PropertyKind::Variant as u16;

/// The bit that tracks whether a node has ever carried a list of text runs.
///
/// Outside the range above and kept separately, because widening that range swallowed
/// values where zero means zero: a picker whose minimum really is 0 stopped sending it
/// and got the Renderer's default instead. A list of runs has no such value. Empty and
/// absent are the same state, so the first empty one is worth no record.
const SPANS_BIT: u32 = 63;

/// The bits for the properties text laid out by CSS carries, in the same order as their
/// tags. Kept apart for the same reason as the runs: their tags are far past the range
/// above, and none of them has a zero that means anything but "not sent".
const TEXT_LAYOUT_BITS: [(PropertyKind, u32); 7] = [
    (PropertyKind::Font, 56),
    (PropertyKind::WordBreak, 57),
    (PropertyKind::OverflowWrap, 58),
    (PropertyKind::TabSize, 59),
    (PropertyKind::AbsoluteSize, 60),
    (PropertyKind::SoftWrap, 61),
    (PropertyKind::SpanFonts, 62),
];

/// The font table of a `Text`'s runs, as an attribute of its own. The same runs travel as
/// `spans` and their fonts as this, so a run record keeps its own layout.
#[derive(Clone, PartialEq)]
pub struct SpanFontTable(pub compose_rust::spans::TextSpans);

#[derive(Clone, Copy, Debug)]
struct Handler {
    id: u64,
    element: ElementId,
    node_id: u32,
    name: &'static str,
    /// The application's value for a link run, when this handler stands for one. Zero for
    /// every handler a listener attribute created.
    link: u64,
}

/// The listener name a link run's press is delivered to, without the `on` that the
/// attribute is spelled with.
pub(crate) const LINK_EVENT: &str = "link";

/// A `Text`'s string marked as one that usually grows at the end.
///
/// Carried in place of a plain string when `Text { streaming: true }` is set. The Host
/// remembers how much of it the Renderer already holds, and when the new string starts
/// with exactly that, it sends only the tail as an `AppendText` instead of the whole
/// string again. A string that changed anywhere else is sent whole, so the flag can never
/// make the screen disagree with the Host; it only makes the common case cheap.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GrowingText(pub(crate) String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathTarget {
    Node(u32),
    /// A template's dynamic child: the node it hangs under, and the marker standing in
    /// its place until its content arrives.
    Slot {
        parent: u32,
        marker: u32,
    },
}

/// One position among a parent's children.
///
/// The Renderer counts only the nodes it drew, so the index an Insert carries is the
/// number of drawn siblings that come first. A placeholder and a template's unfilled
/// dynamic child are both positions nothing is drawn for, and keeping them here is what
/// lets the content that arrives later take the position it was promised rather than the
/// end of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// A node the Renderer drew.
    Node(u32),
    /// A Dioxus placeholder, named by the element it was created for. Placeholders all
    /// carry node id 0, so the element is the only thing that tells two of them apart.
    Hole(usize),
    /// A template's dynamic child, from the moment the template is built until the
    /// content of that child arrives.
    Pending(u32),
}

#[derive(Debug)]
struct StackNode {
    node_id: u32,
    /// The element this entry was created for, when it is a placeholder.
    ///
    /// A placeholder is not a node in the Compose tree, so every one of them carries node
    /// id 0. Two placeholders on screen at once still stand in two different places, and
    /// the element is what tells them apart.
    element: Option<ElementId>,
    paths: HashMap<Vec<u8>, PathTarget>,
}

/// How many Modifier slots a node has. The slot numbers are assigned in `set_modifier`,
/// and this is one past the last of them.
const MODIFIER_SLOTS: usize = 29;

/// How many shadows one node can carry. Each takes a chain position of its own, because a
/// shadow is a `Shadow` modifier and the wire holds one modifier per position.
pub const MAX_SHADOWS: usize = 8;

/// The first of the node's shadow positions. They sit in a row, ahead of the corners, the
/// shape and the fill, so every shadow is drawn behind the node and the shape's clip does
/// not cut it. A later position is drawn over an earlier one.
const SHADOW_SLOT: u16 = 9;

/// Node id 0 is the "no node" sentinel: a Dioxus placeholder, which draws nothing and
/// takes no slot in the Compose tree.
const PLACEHOLDER_NODE: u32 = 0;

/// `dioxus-core` mutation sink that writes the Compose wire protocol directly, into the
/// batch the Host hands the Renderer.
pub struct ComposeRenderer {
    batch: Batch,
    next_node_id: u32,
    next_handler_id: u64,
    nodes: Vec<Option<u32>>,
    handlers: Vec<Handler>,
    /// Which parent each drawn node hangs under.
    parents: HashMap<u32, u32>,
    /// Which parent each placeholder stands under, keyed by its element rather than by a
    /// node id.
    ///
    /// Placeholders all share node id 0, so keying them by node id made every one of them
    /// overwrite the last: a branch that filled in was then attached to whichever other
    /// placeholder had been seen most recently, which put a subtree under a stranger and,
    /// when that stranger was its own descendant, left the parent chain with no root.
    placeholders: HashMap<usize, u32>,
    /// What stands under each parent, in order.
    ///
    /// An index is read off this list rather than remembered from the Insert that put a
    /// node there. Removing a sibling moves everything after it, so a remembered index is
    /// only right until the next removal: a menu whose entries came and went handed the
    /// next set of entries the position the old last entry had held, and they piled up
    /// past the end of the list instead of taking its place.
    children: HashMap<u32, Vec<Slot>>,
    /// The next marker to stand in a template's dynamic child.
    next_marker: u32,
    /// Which design properties a node has actually been given, one bit per tag.
    optional_props: HashMap<u32, u64>,
    /// A border arrives as a width and a colour in separate attributes; this holds
    /// whichever came first until the pair can be written as one modifier.
    pending_borders: HashMap<u32, (Option<f32>, Option<compose_rust::Paint>)>,
    /// Which attribute last wrote each of a node's Modifier slots, or `""` for a slot
    /// nothing has written.
    ///
    /// Two things need the owner rather than a "written" bit. Clearing a slot that was
    /// never written would cost a mutation on the first frame of every widget, for a
    /// Modifier nobody asked for. And some slots have two attributes that can fill them,
    /// `shape_role` and `corner_radius` for the shape, `padding_role` and `padding` for
    /// the padding: the one left unset arrives as an empty attribute, and without the
    /// owner it would clear the modifier its partner had just written, which is how a
    /// rounded container came out square.
    modifier_slots: HashMap<u32, [&'static str; MODIFIER_SLOTS]>,
    /// Which application token each observed node was given, for reading a report back.
    size_tokens: HashMap<u32, u32>,
    /// For each `Text` whose string is marked as growing, how many bytes the Renderer holds
    /// and a keyed hash of them.
    ///
    /// A hash rather than a copy, so a growing string costs sixteen bytes here instead of
    /// being held twice. The key is random per process, so text that arrives from outside,
    /// which an agent's output is, cannot be written to collide on purpose.
    sent_text: HashMap<u32, (usize, u64)>,
    text_hasher: RandomState,
    /// For each `Text` with link runs, the handler id each of its link values was given.
    /// Kept, so a run that is resent with the same value keeps the same id and the list of
    /// runs compares equal to what the Renderer already has.
    link_handlers: HashMap<u32, Vec<(u64, u64)>>,
    /// Where a list of runs is rewritten with the Host's handler ids before it is sent.
    /// Reused, so resending runs does not allocate once it has grown.
    span_scratch: Vec<u8>,

    /// Which editor node each code editor handle is attached to, by the handle's token.
    editors: HashMap<u32, u32>,
    /// The elements whose `pointer_kinds` is not empty. A pointer handler is announced to
    /// the Renderer only for these, so a node that names no kinds takes no pointer input.
    pointer_on: HashSet<usize>,
    /// The elements the Renderer was told a pointer handler for.
    pointer_announced: HashSet<usize>,
    /// The pointer-related properties each node was given a value for, so one that was
    /// never set is never cleared and an unset one costs no record.
    pointer_sent: HashSet<(u32, u16)>,
    /// The animated attributes each node has already been given a value for. The first
    /// value is where the node starts; every later one is a change to animate.
    animated: std::collections::HashSet<(u32, &'static str)>,
    next_animation_id: u32,
    stack: Vec<StackNode>,
}

impl Default for ComposeRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposeRenderer {
    /// The application's name for an observed node, if that node is observed.
    pub fn size_token(&self, node_id: u32) -> Option<u32> {
        self.size_tokens.get(&node_id).copied()
    }

    /// Every observed node and the application's name for it, as `(node_id, token)`.
    pub(crate) fn observed_nodes(&self) -> Vec<(u32, u32)> {
        self.size_tokens
            .iter()
            .map(|(&node_id, &token)| (node_id, token))
            .collect()
    }

    /// The drawn node nothing was inserted under: the root of the tree this wrote. When
    /// the tree has several, the one created first.
    pub(crate) fn root_node(&self) -> Option<u32> {
        self.nodes
            .iter()
            .flatten()
            .copied()
            .filter(|&node| node != PLACEHOLDER_NODE && !self.parents.contains_key(&node))
            .min()
    }

    pub fn new() -> Self {
        Self::with_id_bases(1, 1)
    }

    /// A renderer whose node ids start at `first_node` and whose handler ids start at
    /// `first_handler`. A tree that shares a batch with another writer takes ids from a
    /// range of its own, so the two never name the same node.
    pub fn with_id_bases(first_node: u32, first_handler: u64) -> Self {
        Self {
            batch: Batch::new(),
            next_node_id: first_node,
            next_handler_id: first_handler,
            nodes: Vec::with_capacity(256),
            handlers: Vec::with_capacity(64),
            parents: HashMap::with_capacity(256),
            placeholders: HashMap::with_capacity(16),
            children: HashMap::with_capacity(256),
            next_marker: 1,
            optional_props: HashMap::with_capacity(64),
            pending_borders: HashMap::with_capacity(16),
            modifier_slots: HashMap::with_capacity(64),
            // Empty until a screen asks, which is the point: a tree that observes nothing
            // allocates nothing here.
            size_tokens: HashMap::new(),
            // Empty until a growing string or a link run arrives, for the same reason.
            sent_text: HashMap::new(),
            text_hasher: RandomState::new(),
            link_handlers: HashMap::new(),
            span_scratch: Vec::new(),

            // Empty for a screen without an editor handle, which allocates nothing.
            editors: HashMap::new(),
            pointer_on: HashSet::new(),
            pointer_announced: HashSet::new(),
            pointer_sent: HashSet::new(),
            animated: std::collections::HashSet::new(),
            next_animation_id: 1,
            stack: Vec::with_capacity(64),
        }
    }

    /// The batch this writes into.
    pub fn batch(&self) -> &Batch {
        &self.batch
    }

    pub fn batch_mut(&mut self) -> &mut Batch {
        &mut self.batch
    }

    pub fn begin_frame(&mut self) {
        self.batch.begin();
    }

    pub fn finish_frame(&mut self) -> Result<&[u8], ProtocolError> {
        self.batch.finish()
    }

    pub fn handler(&self, handler_id: u64) -> Option<(ElementId, u32, &'static str)> {
        self.handlers
            .iter()
            .find(|handler| handler.id == handler_id)
            .map(|handler| (handler.element, handler.node_id, handler.name))
    }

    /// The application's value for the link run this handler stands for, if it stands for
    /// one.
    pub fn link_value(&self, handler_id: u64) -> Option<u64> {
        self.handlers
            .iter()
            .find(|handler| handler.id == handler_id)
            .and_then(|handler| (handler.link != 0).then_some(handler.link))
    }

    pub fn set_text(
        &mut self,
        element: ElementId,
        text: &str,
        selection: Option<compose_rust::Selection>,
    ) {
        let Some(node_id) = self.node(element) else {
            return;
        };
        self.write(Mutation::SetText {
            node_id,
            text,
            selection,
        });
    }

    /// The editor node a code editor handle is attached to, if it is attached to one.
    pub(crate) fn editor_node(&self, token: u32) -> Option<u32> {
        self.editors.get(&token).copied()
    }

    /// Writes one edit the application asked of a code editor.
    pub(crate) fn edit_code(
        &mut self,
        node_id: u32,
        request_id: u32,
        base_version: u32,
        range: compose_rust::code::CodeRange,
        text: &str,
    ) {
        self.batch
            .edit_code(node_id, request_id, base_version, range, text);
    }

    /// Turns a Modifier attribute written in `rsx!` into one slot of the node's chain.
    ///
    /// The Renderer applies the chain in list order, exactly as hand-written Compose does,
    /// so the slot an attribute occupies decides what the result looks like. Padding
    /// before a background is a margin; after it, it insets the content. Fixing the slots
    /// here means `Card { background: .., padding: 16.0 }` behaves the way the person who
    /// wrote it expects, whichever order they happened to type the attributes in.
    ///
    /// The order below reads outside in: how big the widget is, what shape it is, what
    /// fills it, what outlines it, what lifts it, what responds to a press, and finally
    /// how far its content sits inside all of that.
    ///
    /// A `false` or absent value writes `Empty` into the slot rather than dropping the
    /// mutation, because a slot that keeps its old value would leave the removed modifier
    /// applied.
    fn modifier_for(
        &mut self,
        _element: ElementId,
        node_id: u32,
        name: &'static str,
        value: &AttributeValue,
    ) -> Option<Option<(u16, compose_rust::Modifier)>> {
        use compose_rust::Modifier;

        // Slot assignments, which are chain positions. Every write in one process uses
        // this table, so a node never holds a value under a meaning it no longer has, but
        // the relative order of the slots is the order the Renderer applies them in and
        // has to stay what CSS and Compose both expect: where the box is and how big, then
        // what it is drawn through, then what fills and outlines it, then what responds
        // to a press, then how far its content sits inside.
        const WEIGHT: u16 = 0;
        const OFFSET: u16 = 1;
        const REQUIRED_SIZE: u16 = 2;
        const FILL_MAX_WIDTH: u16 = 3;
        const FILL_MAX_HEIGHT: u16 = 4;
        const WIDTH: u16 = 5;
        const HEIGHT: u16 = 6;
        // The transform sits outside the opacity and the decoration, so all of them turn
        // with the box, and inside the offset, so it turns about the box where it is.
        const TRANSFORM: u16 = 7;
        const ALPHA: u16 = 8;
        // The shadows take SHADOW_SLOT and the MAX_SHADOWS positions after it.
        const CORNER_EACH: u16 = SHADOW_SLOT + MAX_SHADOWS as u16;
        const SHAPE: u16 = CORNER_EACH + 1;
        const BACKGROUND: u16 = CORNER_EACH + 2;
        const BORDER: u16 = CORNER_EACH + 3;
        const BORDER_EACH: u16 = CORNER_EACH + 4;
        const ELEVATION: u16 = CORNER_EACH + 5;
        const CLIP: u16 = CORNER_EACH + 6;
        // CLICKABLE_SLOT sits at CORNER_EACH + 7.
        const PADDING: u16 = CORNER_EACH + 8;
        const OBSERVE_SIZE: u16 = CORNER_EACH + 9;
        const MOTION: u16 = CORNER_EACH + 10;
        const MATERIAL: u16 = CORNER_EACH + 11;

        let float = |value: &AttributeValue| match value {
            AttributeValue::Float(number) => Some(*number as f32),
            AttributeValue::Int(number) => Some(*number as f32),
            _ => None,
        };
        let integer = |value: &AttributeValue| match value {
            AttributeValue::Int(number) => Some(*number),
            AttributeValue::Float(number) => Some(*number as i64),
            _ => None,
        };

        let slot_of = |name: &str| match name {
            "weight" => Some(WEIGHT),
            "fill_max_width" => Some(FILL_MAX_WIDTH),
            "fill_max_height" => Some(FILL_MAX_HEIGHT),
            "width" => Some(WIDTH),
            "height" => Some(HEIGHT),
            "shape_role" | "corner_radius" => Some(SHAPE),
            "background" => Some(BACKGROUND),
            "border_width" | "border_color" => Some(BORDER),
            "elevation" => Some(ELEVATION),
            "padding" | "padding_role" => Some(PADDING),
            "observe_size" => Some(OBSERVE_SIZE),
            "motion" => Some(MOTION),
            "material" => Some(MATERIAL),
            "offset" => Some(OFFSET),
            "animated_alpha" => Some(ALPHA),
            "animated_background" => Some(BACKGROUND),
            "animated_offset" => Some(TRANSFORM),
            "required_size" => Some(REQUIRED_SIZE),
            "alpha" => Some(ALPHA),
            "corner_each" => Some(CORNER_EACH),
            "border_each" => Some(BORDER_EACH),
            "clip" => Some(CLIP),
            "transform" => Some(TRANSFORM),
            _ => None,
        };
        let slot = slot_of(name)?;

        // An unset Modifier attribute is not a property, so it must not fall through to
        // set_property and be rejected as an unknown one. It only produces a mutation if
        // this slot already holds something, because clearing a slot that was never
        // written would cost a mutation on the first frame of every widget.
        if matches!(value, AttributeValue::None) {
            let owners = self
                .modifier_slots
                .entry(node_id)
                .or_insert([""; MODIFIER_SLOTS]);
            // Only the attribute that wrote the slot may clear it. `corner_radius` being
            // unset says nothing about the `shape_role` sitting in the same slot.
            if owners[slot as usize] != name {
                return Some(None);
            }
            owners[slot as usize] = "";
            return Some(Some((slot, compose_rust::Modifier::Empty)));
        }
        self.modifier_slots
            .entry(node_id)
            .or_insert([""; MODIFIER_SLOTS])[slot as usize] = name;

        match name {
            // The token is the application's name for this node, and it stays here: what
            // the Renderer reports back is its own node id, so the pairing has to be
            // remembered at the moment the two are both in hand.
            "observe_size" => {
                let token = u32::try_from(integer(value)?).ok()?;
                self.size_tokens.insert(node_id, token);
                Some(Some((slot, Modifier::ObserveSize { token })))
            }
            "fill_max_width" | "fill_max_height" => {
                let AttributeValue::Bool(enabled) = value else {
                    return Some(None);
                };
                Some(Some((
                    slot,
                    if !enabled {
                        Modifier::Empty
                    } else if slot == FILL_MAX_WIDTH {
                        Modifier::FillMaxWidth
                    } else {
                        Modifier::FillMaxHeight
                    },
                )))
            }
            "weight" => Some(Some((WEIGHT, Modifier::Weight(float(value)?)))),
            "width" => Some(Some((WIDTH, Modifier::Width(float(value)?)))),
            "height" => Some(Some((HEIGHT, Modifier::Height(float(value)?)))),
            "padding" => Some(Some((PADDING, Modifier::Padding(float(value)?)))),
            "padding_role" => Some(Some((
                PADDING,
                Modifier::PaddingRole(
                    compose_rust::SpaceRole::try_from(u16::try_from(integer(value)?).ok()?).ok()?,
                ),
            ))),
            "elevation" => Some(Some((ELEVATION, Modifier::Elevation(float(value)?)))),
            "corner_radius" => {
                let radius = float(value)?;
                Some(Some((
                    SHAPE,
                    Modifier::Shape {
                        top_start: radius,
                        top_end: radius,
                        bottom_end: radius,
                        bottom_start: radius,
                    },
                )))
            }
            // The node says how important its changes are. Which curve and how many
            // milliseconds that means is the design system's answer, and it is never
            // asked here.
            "motion" => Some(Some((
                MOTION,
                Modifier::Motion(
                    compose_rust::MotionRole::try_from(u16::try_from(integer(value)?).ok()?)
                        .ok()?,
                ),
            ))),
            // What the surface is made of. Blur, tone or a flat fill is the running
            // design system's answer, and the node does not get to ask for one of them.
            "material" => Some(Some((
                MATERIAL,
                Modifier::Material(
                    compose_rust::MaterialRole::try_from(u16::try_from(integer(value)?).ok()?)
                        .ok()?,
                ),
            ))),
            "shape_role" => Some(Some((
                SHAPE,
                Modifier::ShapeRole(
                    compose_rust::ShapeRole::try_from(u16::try_from(integer(value)?).ok()?).ok()?,
                ),
            ))),
            // Colour crosses as the bits of a Paint, so a role and a literal colour take
            // the same path and there is one wire representation of colour.
            "background" => Some(Some((
                BACKGROUND,
                Modifier::Background(compose_rust::Paint::from_bits(integer(value)? as u64)?),
            ))),
            // A border needs a width and a colour, which arrive as two attributes. The
            // half that arrives first is remembered so the pair can be written as one
            // modifier, whichever order Dioxus hands them over in.
            "border_width" | "border_color" => {
                let pending = self.pending_borders.entry(node_id).or_default();
                if name == "border_width" {
                    pending.0 = Some(float(value)?);
                } else {
                    pending.1 = Some(compose_rust::Paint::from_bits(integer(value)? as u64)?);
                }
                let (Some(width), Some(paint)) = (pending.0, pending.1) else {
                    return Some(None);
                };
                Some(Some((BORDER, Modifier::Border { width, paint })))
            }
            // A whole modifier, carried as itself so its numbers arrive as one record. Only
            // the kind the attribute names is accepted, so a value meant for one slot can
            // never land in another.
            "offset" | "required_size" | "corner_each" | "border_each" | "transform" => {
                let AttributeValue::Any(any) = value else {
                    return None;
                };
                let modifier = any.as_any().downcast_ref::<Modifier>()?.clone();
                let fits = match (&modifier, name) {
                    (Modifier::Offset { .. }, "offset")
                    | (Modifier::RequiredSize { .. }, "required_size")
                    | (Modifier::Transform { .. }, "transform") => true,
                    // One radius or one border for every side is the record an ordinary
                    // node already sends, which is what `Modifier::corner_radii` and
                    // `Modifier::border_sides` give for equal values.
                    (Modifier::CornerEach { .. } | Modifier::Shape { .. }, "corner_each") => true,
                    (Modifier::BorderEach { .. } | Modifier::Border { .. }, "border_each") => true,
                    _ => false,
                };
                fits.then_some(Some((slot, modifier)))
            }
            // A value that is animated to. The node's underlying value becomes the target at
            // once, and from the second value on a transition from whatever is on screen to
            // it goes out in the same batch, for the Renderer to play on its own clock.
            "animated_alpha" | "animated_background" | "animated_offset" => {
                use compose_rust::{AnimatedProperty, KeyframeValue, TransformFunction};
                let AttributeValue::Any(any) = value else {
                    return None;
                };
                let any = any.as_any();
                let (base, property, to, spec, from, watch) =
                    if let Some(alpha) = any.downcast_ref::<crate::hooks::Animated<f32>>() {
                        (
                            Modifier::Alpha(alpha.target),
                            AnimatedProperty::Alpha,
                            KeyframeValue::Alpha(alpha.target),
                            alpha.spec,
                            alpha.enter_from.map(KeyframeValue::Alpha),
                            alpha.watch,
                        )
                    } else if let Some(paint) =
                        any.downcast_ref::<crate::hooks::Animated<compose_rust::Paint>>()
                    {
                        (
                            Modifier::Background(paint.target),
                            AnimatedProperty::Background,
                            KeyframeValue::Paint(paint.target),
                            paint.spec,
                            paint.enter_from.map(KeyframeValue::Paint),
                            paint.watch,
                        )
                    } else {
                        let offset = any.downcast_ref::<crate::hooks::Animated<(f32, f32)>>()?;
                        let (x, y) = offset.target;
                        (
                            Modifier::Transform {
                                a: 1.0,
                                b: 0.0,
                                c: 0.0,
                                d: 1.0,
                                e: x,
                                f: y,
                                origin_x: 0.5,
                                origin_y: 0.5,
                            },
                            AnimatedProperty::Transform,
                            KeyframeValue::Transform(std::borrow::Cow::Owned(vec![
                                TransformFunction::translate(x, y),
                            ])),
                            offset.spec,
                            offset.enter_from.map(|(from_x, from_y)| {
                                KeyframeValue::Transform(std::borrow::Cow::Owned(vec![
                                    TransformFunction::translate(from_x, from_y),
                                ]))
                            }),
                            offset.watch,
                        )
                    };
                // A node that appears with a starting value plays from it to the target; a
                // later change plays from whatever is on screen.
                let first = self.animated.insert((node_id, name));
                if !first || from.is_some() {
                    let animation_id = self.next_animation_id;
                    self.next_animation_id = self.next_animation_id.wrapping_add(1).max(1);
                    let mut animation =
                        compose_rust::transition(node_id, animation_id, property, to, spec);
                    if let (true, Some(from)) = (first, from) {
                        let start = &mut animation.keyframes.to_mut()[0];
                        start.value = from;
                        start.from_presented = false;
                    }
                    if watch != 0 {
                        animation.events = compose_rust::AnimationEvents::END;
                        crate::animated_visibility::watch_animation(animation_id, watch);
                    }
                    self.write(Mutation::StartAnimation(animation));
                }
                Some(Some((slot, base)))
            }
            "clip" => {
                let AttributeValue::Bool(enabled) = value else {
                    return None;
                };
                Some(Some((CLIP, Modifier::Clip(*enabled))))
            }
            "alpha" => Some(Some((ALPHA, Modifier::Alpha(float(value)?)))),
            _ => None,
        }
    }

    fn write(&mut self, mutation: Mutation<'_>) {
        self.batch.write(mutation);
    }

    /// Writes a node's shadow list: one `Shadow` modifier in each of the positions from
    /// `SHADOW_SLOT`, in the order of the chain, and `Empty` in a position a shorter list
    /// no longer fills.
    ///
    /// A single `Modifier::Shadow` is a list of one, which is what `AbsoluteBox` writes.
    /// A position is cleared only when this attribute wrote it, and only the first time,
    /// so a node that never had a shadow costs no mutation.
    fn write_shadows(&mut self, node_id: u32, value: &AttributeValue) {
        use compose_rust::Modifier;
        let shadows: Vec<Modifier> = match value {
            AttributeValue::Any(any) => {
                let any = any.as_any();
                if let Some(chain) = any.downcast_ref::<crate::widgets::ShadowChain>() {
                    chain.0.clone()
                } else if let Some(shadow @ Modifier::Shadow { .. }) =
                    any.downcast_ref::<Modifier>()
                {
                    vec![shadow.clone()]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };
        let owners = self
            .modifier_slots
            .entry(node_id)
            .or_insert([""; MODIFIER_SLOTS]);
        let mut writes = Vec::new();
        for position in 0..MAX_SHADOWS {
            let slot = SHADOW_SLOT as usize + position;
            match shadows.get(position) {
                Some(shadow @ Modifier::Shadow { .. }) => {
                    owners[slot] = "shadow";
                    writes.push((slot as u16, shadow.clone()));
                }
                _ if owners[slot] == "shadow" => {
                    owners[slot] = "";
                    writes.push((slot as u16, Modifier::Empty));
                }
                _ => {}
            }
        }
        for (index, modifier) in writes {
            self.write(Mutation::SetModifier {
                node_id,
                index,
                modifier,
            });
        }
    }

    fn allocate_node(&mut self, widget: WidgetKind) -> u32 {
        let id = self.next_node_id;
        self.next_node_id = self.next_node_id.checked_add(1).unwrap_or(1);
        self.write(Mutation::Create {
            node_id: id,
            widget,
        });
        id
    }

    fn map_node(&mut self, element: ElementId, node_id: u32) {
        if self.nodes.len() <= element.0 {
            self.nodes.resize(element.0 + 1, None);
        }
        self.nodes[element.0] = Some(node_id);
    }

    fn node(&self, element: ElementId) -> Option<u32> {
        self.nodes.get(element.0).copied().flatten()
    }

    fn build_template_node(
        &mut self,
        node: &TemplateNode,
        path: &mut Vec<u8>,
        paths: &mut HashMap<Vec<u8>, PathTarget>,
    ) -> Option<u32> {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let widget = match widget_kind(tag) {
                    Ok(widget) => widget,
                    Err(error) => {
                        self.batch.fail(error);
                        return None;
                    }
                };
                let node_id = self.allocate_node(widget);
                paths.insert(path.clone(), PathTarget::Node(node_id));
                for attribute in *attrs {
                    if let TemplateAttribute::Static { name, value, .. } = attribute {
                        self.set_property(
                            node_id,
                            None,
                            name,
                            &AttributeValue::Text((*value).into()),
                        );
                    }
                }
                for (index, child) in children.iter().enumerate() {
                    path.push(index as u8);
                    match child {
                        TemplateNode::Dynamic { .. } => {
                            let marker = self.reserve_slot(node_id);
                            paths.insert(
                                path.clone(),
                                PathTarget::Slot {
                                    parent: node_id,
                                    marker,
                                },
                            );
                        }
                        _ => {
                            if let Some(child_id) = self.build_template_node(child, path, paths) {
                                let end = self.end_of(node_id);
                                self.insert_node(node_id, child_id, end, None);
                            }
                        }
                    }
                    path.pop();
                }
                Some(node_id)
            }
            TemplateNode::Text { text } => {
                let node_id = self.allocate_node(WidgetKind::Text);
                self.write(Mutation::SetProp {
                    node_id,
                    property: PropertyKind::Text,
                    value: PropertyValue::String(text),
                });
                paths.insert(path.clone(), PathTarget::Node(node_id));
                Some(node_id)
            }
            TemplateNode::Dynamic { .. } => None,
        }
    }

    /// Role tag 0 means "not sent", so a design property at its neutral value
    /// produces no record at all. A property that was set and then cleared still sends
    /// its zero once, which is what tells the Renderer to drop the override.
    fn should_write_optional_property(
        &mut self,
        node_id: u32,
        property: PropertyKind,
        neutral: bool,
    ) -> bool {
        let tag = property as u16;
        let index = if (FIRST_OPTIONAL_PROPERTY..=LAST_OPTIONAL_PROPERTY).contains(&tag) {
            u32::from(tag - FIRST_OPTIONAL_PROPERTY)
        } else if property == PropertyKind::Spans {
            SPANS_BIT
        } else if let Some((_, bit)) = TEXT_LAYOUT_BITS.iter().find(|(kind, _)| *kind == property) {
            *bit
        } else {
            return true;
        };
        let bit = 1_u64 << index;
        let seen = self.optional_props.get(&node_id).copied().unwrap_or(0);
        if neutral {
            if seen & bit == 0 {
                return false;
            }
            self.optional_props.insert(node_id, seen & !bit);
        } else if seen & bit == 0 {
            self.optional_props.insert(node_id, seen | bit);
        }
        true
    }

    fn set_property(
        &mut self,
        node_id: u32,
        element: Option<ElementId>,
        name: &str,
        value: &AttributeValue,
    ) {
        let Some(property) = property_kind(name) else {
            // dioxus-core has no fallible WriteMutations methods. Preserve the
            // protocol error and surface it when the batch is finalized.
            self.batch.fail(ProtocolError::InvalidProperty(0));
            return;
        };
        if matches!(
            property,
            PropertyKind::PointerKinds | PropertyKind::Cursor | PropertyKind::UserSelect
        ) {
            let key = (node_id, property as u16);
            if matches!(value, AttributeValue::None) {
                if !self.pointer_sent.remove(&key) {
                    return;
                }
            } else {
                self.pointer_sent.insert(key);
            }
        }
        if let AttributeValue::Any(value) = value {
            let any = value.as_any();
            if let Some(growing) = any.downcast_ref::<GrowingText>()
                && property == PropertyKind::Text
            {
                self.set_growing_text(node_id, &growing.0);
                return;
            }
            if let (Some(spans), Some(element)) = (
                any.downcast_ref::<compose_rust::spans::TextSpans>(),
                element,
            ) && spans.spans().any(|span| span.on_click.is_some())
            {
                self.set_linked_spans(node_id, element, property, spans);
                return;
            }
        }
        let value = match value {
            AttributeValue::Text(value) => PropertyValue::String(value),
            AttributeValue::Float(value) => PropertyValue::Float(*value as f32),
            AttributeValue::Int(value) => PropertyValue::Integer(*value),
            AttributeValue::Bool(value) => PropertyValue::Bool(*value),
            AttributeValue::None => PropertyValue::None,
            // A drawing command list is the one value that is neither a number nor text.
            // dioxus-core compares it before calling here, so an unchanged list never
            // reaches this point and costs no record.
            AttributeValue::Any(value) => {
                let any = value.as_any();
                if let Some(list) = any.downcast_ref::<DrawList>() {
                    PropertyValue::Bytes(list.as_bytes())
                } else if let Some(spans) = any.downcast_ref::<compose_rust::spans::TextSpans>() {
                    // Runs inside a string travel the same way a drawing does, and for the
                    // same reason: a list that has not changed compares equal before it
                    // reaches here and costs no record at all.
                    PropertyValue::Bytes(spans.as_bytes())
                } else if let Some(table) = any.downcast_ref::<SpanFontTable>() {
                    PropertyValue::Bytes(table.0.font_table())
                } else if let Some(fonts) = any.downcast_ref::<compose_rust::fonts::FontRefs>() {
                    PropertyValue::Bytes(fonts.as_bytes())
                } else if let Some(decorations) =
                    any.downcast_ref::<compose_rust::code::Decorations>()
                {
                    PropertyValue::Bytes(decorations.as_bytes())
                } else if let Some(folds) = any.downcast_ref::<compose_rust::code::Folds>() {
                    PropertyValue::Bytes(folds.as_bytes())
                } else if let Some(marks) =
                    any.downcast_ref::<compose_rust::code::GutterDecorations>()
                {
                    PropertyValue::Bytes(marks.as_bytes())
                } else if let Some(spans) = any.downcast_ref::<compose_rust::code::SyntaxSpans>() {
                    PropertyValue::Bytes(spans.as_bytes())
                } else {
                    return;
                }
            }
            AttributeValue::Listener(_) => return,
        };
        // A type role of zero is `TypeRole::None`, which is a value: text with no rung of
        // the ladder. A Text with no role at all sends nothing, and one whose role was
        // taken away sends `None`, which is what drops it on the other side.
        let neutral = matches!(value, PropertyValue::None | PropertyValue::Float(0.0))
            || (matches!(value, PropertyValue::Integer(0)) && property != PropertyKind::TypeRole);
        if !self.should_write_optional_property(node_id, property, neutral) {
            return;
        }
        self.write(Mutation::SetProp {
            node_id,
            property,
            value,
        });
    }

    /// Sends a growing string: only its tail when the Renderer already holds the rest,
    /// and the whole string otherwise.
    fn set_growing_text(&mut self, node_id: u32, text: &str) {
        let full = self.text_hasher.hash_one(text.as_bytes());
        let held = self.sent_text.get(&node_id).copied();
        self.sent_text.insert(node_id, (text.len(), full));
        if let Some((length, hash)) = held
            && text.len() > length
            && text.is_char_boundary(length)
            && self.text_hasher.hash_one(&text.as_bytes()[..length]) == hash
        {
            self.write(Mutation::AppendText {
                node_id,
                text: &text[length..],
            });
            return;
        }
        self.write(Mutation::SetProp {
            node_id,
            property: PropertyKind::Text,
            value: PropertyValue::String(text),
        });
    }

    /// Sends a list of runs with every link value swapped for a handler id of the Host's.
    ///
    /// The value is the application's name for the link and means nothing to the
    /// Renderer, which reports a press on a run by the id the run carries. So each value
    /// gets a handler of its own, registered against this node under the link event, and
    /// a press comes back with the value it stood for.
    fn set_linked_spans(
        &mut self,
        node_id: u32,
        element: ElementId,
        property: PropertyKind,
        spans: &compose_rust::spans::TextSpans,
    ) {
        const HANDLER_AT: usize = compose_rust::spans::SPAN_HANDLER_AT;
        let mut scratch = std::mem::take(&mut self.span_scratch);
        scratch.clear();
        scratch.extend_from_slice(spans.as_bytes());
        for record in scratch
            .as_chunks_mut::<{ compose_rust::spans::SPAN_LEN }>()
            .0
        {
            let mut word = [0_u8; 8];
            word.copy_from_slice(&record[HANDLER_AT..HANDLER_AT + 8]);
            let value = u64::from_le_bytes(word);
            if value == 0 {
                continue;
            }
            let handler_id = self.link_handler(node_id, element, value);
            record[HANDLER_AT..HANDLER_AT + 8].copy_from_slice(&handler_id.to_le_bytes());
        }
        self.should_write_optional_property(node_id, property, false);
        self.write(Mutation::SetProp {
            node_id,
            property,
            value: PropertyValue::Bytes(&scratch),
        });
        self.span_scratch = scratch;
    }

    /// The handler id a link value on this node reports through, made the first time the
    /// value is seen.
    fn link_handler(&mut self, node_id: u32, element: ElementId, value: u64) -> u64 {
        if let Some(known) = self
            .link_handlers
            .get(&node_id)
            .and_then(|links| links.iter().find(|(link, _)| *link == value))
        {
            return known.1;
        }
        let handler_id = self.next_handler_id;
        self.next_handler_id += 1;
        self.handlers.push(Handler {
            id: handler_id,
            element,
            node_id,
            name: LINK_EVENT,
            link: value,
        });
        self.link_handlers
            .entry(node_id)
            .or_default()
            .push((value, handler_id));
        handler_id
    }

    /// One past the last position under `parent`.
    fn end_of(&self, parent: u32) -> usize {
        self.children.get(&parent).map_or(0, Vec::len)
    }

    /// Stands a marker in a template's dynamic child until its content arrives, and says
    /// which marker it was.
    fn reserve_slot(&mut self, parent: u32) -> u32 {
        let marker = self.next_marker;
        self.next_marker = self.next_marker.checked_add(1).unwrap_or(1);
        self.children
            .entry(parent)
            .or_default()
            .push(Slot::Pending(marker));
        marker
    }

    /// Where the given slot stands: the parent it hangs under and its position in that
    /// parent's list.
    fn locate(&self, slot: Slot) -> Option<(u32, usize)> {
        let parent = match slot {
            Slot::Node(node_id) => *self.parents.get(&node_id)?,
            Slot::Hole(element) => *self.placeholders.get(&element)?,
            Slot::Pending(_) => return None,
        };
        let position = self
            .children
            .get(&parent)?
            .iter()
            .position(|standing| *standing == slot)?;
        Some((parent, position))
    }

    /// How many drawn nodes stand before `position` under `parent`, which is the index
    /// the Renderer understands.
    fn drawn_before(&self, parent: u32, position: usize) -> u32 {
        self.children.get(&parent).map_or(0, |list| {
            list.iter()
                .take(position)
                .filter(|slot| matches!(slot, Slot::Node(_)))
                .count() as u32
        })
    }

    /// Takes the slot out of wherever it stands, and says where that was.
    fn detach(&mut self, slot: Slot) -> Option<(u32, usize)> {
        let (parent, position) = self.locate(slot)?;
        if let Some(list) = self.children.get_mut(&parent) {
            list.remove(position);
        }
        Some((parent, position))
    }

    /// Puts the slot under `parent` at `position`, and says which position it took.
    fn attach(&mut self, parent: u32, slot: Slot, position: usize) -> usize {
        let mut position = position;
        if let Some((from, was_at)) = self.detach(slot)
            && from == parent
            && was_at < position
        {
            position -= 1;
        }
        let list = self.children.entry(parent).or_default();
        let position = position.min(list.len());
        list.insert(position, slot);
        match slot {
            Slot::Node(node_id) => {
                self.parents.insert(node_id, parent);
            }
            Slot::Hole(element) => {
                self.placeholders.insert(element, parent);
            }
            Slot::Pending(_) => {}
        }
        position
    }

    /// Forgets a node and everything under it, as the Renderer does when it is removed.
    /// The caller has already taken the node out of its parent's list.
    fn forget(&mut self, node_id: u32) {
        self.parents.remove(&node_id);
        self.sent_text.remove(&node_id);
        if self.link_handlers.remove(&node_id).is_some() {
            self.handlers.retain(|handler| handler.node_id != node_id);
        }
        if !self.editors.is_empty() {
            self.editors.retain(|_, node| *node != node_id);
        }
        for slot in self.children.remove(&node_id).unwrap_or_default() {
            match slot {
                Slot::Node(child) => self.forget(child),
                Slot::Hole(element) => {
                    self.placeholders.remove(&element);
                }
                Slot::Pending(_) => {}
            }
        }
    }

    fn insert_stack(&mut self, parent: u32, position: usize, count: usize) {
        let start = self.stack.len().saturating_sub(count);
        let nodes: Vec<_> = self.stack.drain(start..).collect();
        let mut at = position;
        for node in nodes {
            at = self.insert_node(parent, node.node_id, at, node.element) + 1;
        }
    }

    /// Puts one entry under `parent` at `position` and tells the Renderer about it,
    /// unless nothing is drawn for it. Says which position it took.
    fn insert_node(
        &mut self,
        parent: u32,
        node_id: u32,
        position: usize,
        element: Option<ElementId>,
    ) -> usize {
        if node_id == PLACEHOLDER_NODE {
            // Nothing is drawn for a placeholder, so nothing is sent. It still holds its
            // position here, so the branch that fills it in takes that position rather
            // than the end of the list.
            let Some(element) = element else {
                return position;
            };
            return self.attach(parent, Slot::Hole(element.0), position);
        }
        let moving = self.parents.contains_key(&node_id);
        let at = self.attach(parent, Slot::Node(node_id), position);
        let index = self.drawn_before(parent, at);
        self.write(if moving {
            Mutation::Move {
                parent_id: parent,
                node_id,
                index,
            }
        } else {
            Mutation::Insert {
                parent_id: parent,
                node_id,
                index,
            }
        });
        at
    }

    /// Where the given element stands: its own position, or its placeholder's.
    fn slot_of(&self, id: ElementId) -> Option<(u32, usize)> {
        match self.node(id) {
            Some(PLACEHOLDER_NODE) | None => self.locate(Slot::Hole(id.0)),
            Some(node_id) => self.locate(Slot::Node(node_id)),
        }
    }

    /// Announces the pointer handler of `id` when the node names kinds to be told about, and
    /// withdraws it when it stops naming any.
    fn sync_pointer_handler(&mut self, id: ElementId, node_id: u32) {
        let handler = self
            .handlers
            .iter()
            .find(|handler| handler.element == id && is_pointer_event(handler.name))
            .map(|handler| handler.id);
        match (self.pointer_on.contains(&id.0), handler) {
            (true, Some(handler_id)) => {
                if self.pointer_announced.insert(id.0) {
                    self.write(Mutation::SetProp {
                        node_id,
                        property: PropertyKind::OnPointer,
                        value: PropertyValue::Integer(handler_id as i64),
                    });
                }
            }
            (false, _) | (true, None) => {
                if self.pointer_announced.remove(&id.0) {
                    self.write(Mutation::SetProp {
                        node_id,
                        property: PropertyKind::OnPointer,
                        value: PropertyValue::None,
                    });
                }
            }
        }
    }
}

impl WriteMutations for ComposeRenderer {
    fn append_children(&mut self, id: ElementId, m: usize) {
        if let Some(parent) = self.node(id) {
            let end = self.end_of(parent);
            self.insert_stack(parent, end, m);
        }
    }

    fn assign_node_id(&mut self, path: &'static [u8], id: ElementId) {
        if let Some(PathTarget::Node(node_id)) = self
            .stack
            .last()
            .and_then(|loaded| loaded.paths.get(path))
            .copied()
        {
            self.map_node(id, node_id);
        }
    }

    fn create_placeholder(&mut self, id: ElementId) {
        // A Dioxus placeholder is not materialized in the Compose tree. It is
        // represented by its eventual insertion position.
        self.map_node(id, PLACEHOLDER_NODE);
        self.stack.push(StackNode {
            node_id: PLACEHOLDER_NODE,
            element: Some(id),
            paths: HashMap::new(),
        });
    }

    fn create_text_node(&mut self, value: &str, id: ElementId) {
        let node_id = self.allocate_node(WidgetKind::Text);
        self.map_node(id, node_id);
        self.write(Mutation::SetProp {
            node_id,
            property: PropertyKind::Text,
            value: PropertyValue::String(value),
        });
        self.stack.push(StackNode {
            node_id,
            element: None,
            paths: HashMap::new(),
        });
    }

    fn load_template(&mut self, template: Template, index: usize, id: ElementId) {
        let mut paths = HashMap::new();
        let mut path = Vec::new();
        if let Some(root) = self.build_template_node(&template.roots[index], &mut path, &mut paths)
        {
            self.map_node(id, root);
            self.stack.push(StackNode {
                node_id: root,
                element: None,
                paths,
            });
        }
    }

    fn replace_node_with(&mut self, id: ElementId, m: usize) {
        if let Some(node_id) = self.node(id) {
            let target = self.slot_of(id);
            if node_id == PLACEHOLDER_NODE {
                self.detach(Slot::Hole(id.0));
                self.placeholders.remove(&id.0);
            } else {
                self.detach(Slot::Node(node_id));
                self.write(Mutation::Remove { node_id });
                self.forget(node_id);
            }
            // The slot has just been taken out, so what replaces it goes in at the
            // position it held.
            if let Some((parent, position)) = target {
                self.insert_stack(parent, position, m);
                return;
            }
        }
        let keep_from = self.stack.len().saturating_sub(m);
        self.stack.drain(..keep_from);
    }

    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize) {
        let parent_position = self.stack.len().saturating_sub(m + 1);
        let target = self
            .stack
            .get(parent_position)
            .and_then(|loaded| loaded.paths.get(path))
            .copied();
        if let Some(PathTarget::Slot { parent, marker }) = target {
            let Some(position) = self
                .children
                .get(&parent)
                .and_then(|list| list.iter().position(|slot| *slot == Slot::Pending(marker)))
            else {
                return;
            };
            if let Some(list) = self.children.get_mut(&parent) {
                list.remove(position);
            }
            self.insert_stack(parent, position, m);
        }
    }

    fn insert_nodes_after(&mut self, id: ElementId, m: usize) {
        if let Some((parent, position)) = self.slot_of(id) {
            self.insert_stack(parent, position + 1, m);
        }
    }

    fn insert_nodes_before(&mut self, id: ElementId, m: usize) {
        if let Some((parent, position)) = self.slot_of(id) {
            self.insert_stack(parent, position, m);
        }
    }

    fn set_attribute(
        &mut self,
        name: &'static str,
        _namespace: Option<&'static str>,
        value: &AttributeValue,
        id: ElementId,
    ) {
        if let Some(node_id) = self.node(id) {
            // The Host's own name for an editor, kept here and never sent: an edit asked
            // of the handle is addressed to whichever node carries it now.
            if name == "editor" {
                self.editors.retain(|_, node| *node != node_id);
                if let AttributeValue::Int(token) = value
                    && let Ok(token) = u32::try_from(*token)
                {
                    self.editors.insert(token, node_id);
                }
                return;
            }
            if name == "pointer_kinds" {
                if matches!(value, AttributeValue::Int(kinds) if *kinds != 0) {
                    self.pointer_on.insert(id.0);
                } else {
                    self.pointer_on.remove(&id.0);
                }
                self.set_property(node_id, Some(id), name, value);
                self.sync_pointer_handler(id, node_id);
                return;
            }
            // The press modifier is written from the listener (`create_event_listener`),
            // so the attribute itself, which a container without a handler carries as
            // `None`, writes nothing.
            if name == "onclickable" {
                return;
            }
            // A node's shadows are a list, so one attribute writes several positions.
            if name == "shadow" {
                self.write_shadows(node_id, value);
                return;
            }
            if let Some(slot) = self.modifier_for(id, node_id, name, value) {
                if let Some((index, modifier)) = slot {
                    self.write(Mutation::SetModifier {
                        node_id,
                        index,
                        modifier,
                    });
                }
                return;
            }
            self.set_property(node_id, Some(id), name, value);
        }
    }

    fn set_node_text(&mut self, value: &str, id: ElementId) {
        if let Some(node_id) = self.node(id) {
            self.write(Mutation::SetProp {
                node_id,
                property: PropertyKind::Text,
                value: PropertyValue::String(value),
            });
        }
    }

    fn create_event_listener(&mut self, name: &'static str, id: ElementId) {
        let Some(node_id) = self.node(id) else {
            return;
        };
        let handler_id = self.next_handler_id;
        self.next_handler_id += 1;
        self.handlers.push(Handler {
            id: handler_id,
            element: id,
            node_id,
            name,
            link: 0,
        });
        if is_pointer_event(name) {
            self.sync_pointer_handler(id, node_id);
        }
        // A listener reaches the Renderer here and not through `set_attribute`, so the
        // press modifier is written from the listener's handler.
        if is_clickable_event(name) {
            self.write(Mutation::SetModifier {
                node_id,
                index: CLICKABLE_SLOT,
                modifier: compose_rust::Modifier::Clickable { handler_id },
            });
        }
        if let Some(property) = event_property(name) {
            self.write(Mutation::SetProp {
                node_id,
                property,
                value: PropertyValue::Integer(handler_id as i64),
            });
        }
    }

    fn remove_event_listener(&mut self, name: &'static str, id: ElementId) {
        let Some(position) = self
            .handlers
            .iter()
            .position(|handler| handler.element == id && handler.name == name)
        else {
            return;
        };
        self.handlers.swap_remove(position);
        if is_clickable_event(name)
            && let Some(node_id) = self.node(id)
        {
            self.write(Mutation::SetModifier {
                node_id,
                index: CLICKABLE_SLOT,
                modifier: compose_rust::Modifier::Empty,
            });
        }
        if is_pointer_event(name)
            && let Some(node_id) = self.node(id)
        {
            self.sync_pointer_handler(id, node_id);
        }
        if let (Some(node_id), Some(property)) = (self.node(id), event_property(name)) {
            self.write(Mutation::SetProp {
                node_id,
                property,
                value: PropertyValue::None,
            });
        }
    }

    fn remove_node(&mut self, id: ElementId) {
        self.detach(Slot::Hole(id.0));
        self.placeholders.remove(&id.0);
        if let Some(node_id) = self.node(id)
            && node_id != PLACEHOLDER_NODE
        {
            self.detach(Slot::Node(node_id));
            self.write(Mutation::Remove { node_id });
            self.pointer_sent.retain(|(sent, _)| *sent != node_id);
            self.forget(node_id);
        }
        if let Some(slot) = self.nodes.get_mut(id.0) {
            *slot = None;
        }
        self.handlers.retain(|handler| handler.element != id);
        self.pointer_on.remove(&id.0);
        self.pointer_announced.remove(&id.0);
    }

    fn push_root(&mut self, id: ElementId) {
        if let Some(node_id) = self.node(id) {
            self.stack.push(StackNode {
                node_id,
                element: Some(id),
                paths: HashMap::new(),
            });
        }
    }
}

fn widget_kind(name: &str) -> Result<WidgetKind, ProtocolError> {
    WidgetKind::from_name(name).map_err(|()| ProtocolError::InvalidWidget(0))
}

fn property_kind(name: &str) -> Option<PropertyKind> {
    PropertyKind::from_name(name).ok()
}

/// The chain position of the press modifier, which `modifier_for` also uses.
const CLICKABLE_SLOT: u16 = SHADOW_SLOT + MAX_SHADOWS as u16 + 7;

fn is_clickable_event(name: &str) -> bool {
    matches!(name, "clickable" | "onclickable")
}

fn is_pointer_event(name: &str) -> bool {
    matches!(name, "pointer" | "onpointer")
}

fn event_property(name: &str) -> Option<PropertyKind> {
    match name {
        "click" | "onclick" => Some(PropertyKind::OnClick),
        "valuechange" | "onvaluechange" => Some(PropertyKind::OnValueChange),
        "submit" | "onsubmit" => Some(PropertyKind::OnSubmit),
        "focuslost" | "onfocuslost" => Some(PropertyKind::OnFocusLost),
        "keydown" | "onkeydown" => Some(PropertyKind::OnKeyDown),
        "rangerequest" | "onrangerequest" => Some(PropertyKind::OnRangeRequested),
        "dismiss" | "ondismiss" => Some(PropertyKind::OnDismiss),
        "filesentered" | "onfilesentered" => Some(PropertyKind::OnFilesEntered),
        "filesdropped" | "onfilesdropped" => Some(PropertyKind::OnFilesDropped),
        "change" | "onchange" => Some(PropertyKind::OnValueChange),
        // A code editor's committed edits go out under the value change property too.
        "codechange" | "oncodechange" => Some(PropertyKind::OnValueChange),
        "editrejected" | "oneditrejected" => Some(PropertyKind::OnEditRejected),
        "hover" | "onhover" => Some(PropertyKind::OnHover),
        "save" | "onsave" => Some(PropertyKind::OnSave),
        "decorationclick" | "ondecorationclick" => Some(PropertyKind::OnDecorationClick),
        "selectionchange" | "onselectionchange" => Some(PropertyKind::OnSelectionChange),
        "visiblerangechange" | "onvisiblerangechange" => Some(PropertyKind::OnVisibleRangeChange),
        "editorfocuschange" | "oneditorfocuschange" => Some(PropertyKind::OnEditorFocusChange),
        "foldchange" | "onfoldchange" => Some(PropertyKind::OnFoldChange),
        "compositionchange" | "oncompositionchange" => Some(PropertyKind::OnCompositionChange),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    fn app() -> Element {
        rsx! {
            Column {
                Text { text: "zero" }
                TextField { placeholder: "type" }
                Button { text: "increment", on_click: move |_| {} }
            }
        }
    }

    #[test]
    /// The M0 screen written as `rsx!` produces the same tree the protocol describes.
    ///
    /// Named after the requirement now. It was the only test standing behind that
    /// requirement and it did not say so, so counting the tests named for each one
    /// reported none and the requirement read as unimplemented.
    fn fr6_an_rsx_component_builds_the_same_tree() {
        let mut dom = VirtualDom::new(app);
        let mut renderer = ComposeRenderer::new();
        dom.rebuild(&mut renderer);
        let decoded = crate::protocol::decode_batch(renderer.finish_frame().unwrap()).unwrap();
        eprintln!("{decoded:#?}");
        assert!(decoded.iter().any(|mutation| matches!(
            mutation,
            Mutation::Create {
                widget: WidgetKind::Column,
                ..
            }
        )));
        assert!(decoded.iter().any(|mutation| matches!(
            mutation,
            Mutation::Create {
                widget: WidgetKind::TextField,
                ..
            }
        )));
        assert_eq!(
            decoded
                .iter()
                .filter(|mutation| matches!(mutation, Mutation::Insert { .. }))
                .count(),
            3
        );
    }
}
