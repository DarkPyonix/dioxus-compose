//! The display list turned into a tree of drawing elements.
//!
//! A [`DisplayList`] is flat: absolute rectangles in paint order. Compose draws a tree, in
//! which a child is placed relative to its parent and painted after it, a clip or an alpha
//! applies to a whole subtree, and a scroll container moves what is inside it. [`plan_from`]
//! builds that tree: a [`PlanKind::AbsoluteBox`] for every box that holds something, each
//! child carrying an [`PlanModifier::Offset`] from its container's origin, scroll containers
//! as [`PlanKind::ScrollColumn`] and [`PlanKind::ScrollRow`] around their content, text runs
//! as [`PlanKind::Text`] children, form fields as [`PlanKind::TextField`] and the image of an
//! `<img>` as [`PlanKind::Image`].
//!
//! A background colour is a [`PlanModifier::Background`]. A background image or gradient is
//! a [`PlanModifier::BackgroundBrush`] that names a [`Brush`] by its [`BrushId`]; the plan
//! lists every brush it uses once, in [`Plan::brushes`], so a renderer registers each brush
//! once and a node refers to it by id. The id is derived from the brush's content: the
//! same gradient keeps its id from one plan to the next, and a gradient whose stops change
//! gets a new one, which is one change to the background slot of the node that draws it.
//!
//! Images are drawn with the assets an [`ImageResolver`] names for their URLs. A URL it has
//! no asset for, or any URL when there is no resolver, draws nothing: no image node, no
//! brush.
//!
//! [`diff`] compares two plans node by node and says what a renderer has to change: a
//! modifier set or removed, a node inserted, removed or moved among its siblings.
//!
//! This is plain data. Nothing here is encoded for the boundary yet.
//!
//! ## How boxes are nested
//!
//! Every box would ideally sit inside its nearest ancestor box, so that the ancestor's
//! clip, alpha and scroll position apply to it. Paint order can forbid that: a positioned
//! box is painted after boxes that come later in the tree than its parent, and in a tree a
//! child is painted before its parent's later siblings. So each box, taken in paint order,
//! goes into the nearest ancestor where being painted early changes nothing on screen:
//! where no box already placed after that point in the tree overlaps it. A box that
//! overlaps nothing stays with its parent, whatever its `z-index`; a box that does overlap
//! moves out to an ancestor whose subtree ends where it should be painted. The outermost
//! container, the page, always qualifies.
//!
//! A box moved out of an ancestor that clips it is wrapped in a container that clips it to
//! the same rectangle. A box moved out of a scroll container no longer moves with that
//! container's scroll position; that only happens where the box overlaps something painted
//! between the two, which CSS allows and a tree cannot express.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::NodeId;
use crate::display_list::{
    BackgroundImage, Corners, DisplayList, GradientStop, InputKind, NodeEntry, Radius, Rect, Rgba,
    Sides, TextAlign, TileRepeat,
};
use crate::image::{AssetId, ImageResolver};
use crate::measure::TextStyle;

/// How close two lengths must be to count as equal, in CSS pixels.
const EPSILON: f32 = 0.01;

/// A tree of drawing elements. Lengths are CSS pixels, which the renderer draws as dp.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The page: a [`PlanKind::AbsoluteBox`] keyed [`PlanKey::Page`] holding every
    /// top-level box.
    pub root: PlanNode,
    /// Every brush a [`PlanModifier::BackgroundBrush`] in the tree names, once.
    pub brushes: BTreeMap<BrushId, Brush>,
}

/// Names a [`Brush`] by its content: two equal brushes have the same id, in this plan and
/// in the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BrushId(pub u64);

/// A paint that is not one colour: a gradient, or an image asset. Gradient geometry is
/// measured from the top-left corner of the tile it fills, so the same gradient on tiles
/// of the same size is one brush.
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    LinearGradient {
        start: (f32, f32),
        end: (f32, f32),
        stops: Vec<GradientStop>,
        repeating: bool,
    },
    RadialGradient {
        center: (f32, f32),
        radius_x: f32,
        radius_y: f32,
        stops: Vec<GradientStop>,
        repeating: bool,
    },
    /// An image the application registered.
    Image(AssetId),
}

impl Brush {
    /// The id this brush is named by.
    pub fn id(&self) -> BrushId {
        fn point(hasher: &mut DefaultHasher, (x, y): (f32, f32)) {
            x.to_bits().hash(hasher);
            y.to_bits().hash(hasher);
        }
        fn stops(hasher: &mut DefaultHasher, stops: &[GradientStop]) {
            stops.len().hash(hasher);
            for stop in stops {
                stop.offset.to_bits().hash(hasher);
                stop.color.hash(hasher);
            }
        }
        let mut hasher = DefaultHasher::new();
        match self {
            Brush::LinearGradient {
                start,
                end,
                stops: list,
                repeating,
            } => {
                0u8.hash(&mut hasher);
                point(&mut hasher, *start);
                point(&mut hasher, *end);
                stops(&mut hasher, list);
                repeating.hash(&mut hasher);
            }
            Brush::RadialGradient {
                center,
                radius_x,
                radius_y,
                stops: list,
                repeating,
            } => {
                1u8.hash(&mut hasher);
                point(&mut hasher, *center);
                point(&mut hasher, (*radius_x, *radius_y));
                stops(&mut hasher, list);
                repeating.hash(&mut hasher);
            }
            Brush::Image(asset) => {
                2u8.hash(&mut hasher);
                asset.hash(&mut hasher);
            }
        }
        BrushId(hasher.finish())
    }
}

/// Names a node of a plan across updates.
///
/// Every node comes from one display list entry, so every key but [`PlanKey::Page`] names
/// a DOM node; the variant says which of the elements made for that node it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlanKey {
    /// The page every top-level box is placed in.
    Page,
    /// The box of a display list entry.
    Node(NodeId),
    /// The `index`th text run of a node's entry.
    Text { node: NodeId, index: usize },
    /// The field drawn for an `<input>` or `<textarea>`.
    Field(NodeId),
    /// The padding box of a box that clips its content and has a border: CSS clips to the
    /// inside of the border, Compose clips to a node's own bounds, so the content goes into
    /// a node of the padding box's size.
    ClipContent(NodeId),
    /// The vertical scroll viewport of a scroll container.
    ScrollColumn(NodeId),
    /// The horizontal scroll viewport of a scroll container.
    ScrollRow(NodeId),
    /// The content a scroll container scrolls.
    ScrollContent(NodeId),
    /// A clip a box inherits from an ancestor it could not be nested inside.
    InheritedClip(NodeId),
    /// The image of an `<img>`.
    Image(NodeId),
}

impl PlanKey {
    /// The DOM node this key belongs to; `None` for the page.
    pub fn node(&self) -> Option<NodeId> {
        match *self {
            PlanKey::Page => None,
            PlanKey::Node(node)
            | PlanKey::Text { node, .. }
            | PlanKey::Field(node)
            | PlanKey::ClipContent(node)
            | PlanKey::ScrollColumn(node)
            | PlanKey::ScrollRow(node)
            | PlanKey::ScrollContent(node)
            | PlanKey::InheritedClip(node)
            | PlanKey::Image(node) => Some(node),
        }
    }
}

/// One drawing element and what is inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanNode {
    pub key: PlanKey,
    pub kind: PlanKind,
    /// In a fixed order: offset, size, alpha, shape, shadows, background, background
    /// brushes, border, clip.
    pub modifiers: Vec<PlanModifier>,
    /// Back to front.
    pub children: Vec<PlanNode>,
}

/// What a plan node is.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanKind {
    /// Places each child at its own [`PlanModifier::Offset`]; children do not affect each
    /// other's place or size. Later children are drawn on top.
    AbsoluteBox,
    /// A box with nothing inside: background, border, shadow.
    Box,
    /// A run of text. Line breaks and glyph positions are the renderer's.
    Text(PlanText),
    /// An image.
    Image(PlanImage),
    /// Scrolls its one child vertically. The renderer owns the scroll position.
    ScrollColumn,
    /// Scrolls its one child horizontally. The renderer owns the scroll position.
    ScrollRow,
    /// An uncontrolled text field: the renderer owns its text once the user edits it.
    TextField(PlanTextField),
    /// `<input type="checkbox">`.
    Checkbox { checked: bool },
    /// `<input type="radio">`.
    RadioButton { selected: bool },
}

impl PlanKind {
    fn same_variant(&self, other: &PlanKind) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// The text of a [`PlanKind::Text`].
#[derive(Clone, Debug, PartialEq)]
pub struct PlanText {
    pub text: String,
    pub style: TextStyle,
    pub color: Rgba,
    pub align: TextAlign,
    /// Whether lines wrap at the node's width. A run laid out as one unconstrained line
    /// does not.
    pub soft_wrap: bool,
}

/// The image of a [`PlanKind::Image`]. The node is the `<img>` element's content box.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanImage {
    /// The URL, as the display list records it.
    pub source: String,
    /// What the application's [`ImageResolver`] named for the URL.
    pub asset: AssetId,
    /// Where the image is drawn, from the node's top-left corner, with `object-fit` and
    /// `object-position` applied. The image is stretched to exactly this rectangle. When it
    /// reaches outside the node, the node also has a [`PlanModifier::Clip`].
    pub draw: Rect,
}

/// The field of a [`PlanKind::TextField`].
#[derive(Clone, Debug, PartialEq)]
pub struct PlanTextField {
    /// A text-like kind: never `Checkbox` or `Radio`, which have kinds of their own.
    pub input: InputKind,
    /// The value the Host set.
    pub value: String,
    pub placeholder: Option<String>,
    pub style: TextStyle,
    pub color: Rgba,
    /// `<textarea>`.
    pub multiline: bool,
}

/// One side of a [`PlanModifier::BorderEach`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderSide {
    pub width: f32,
    pub color: Rgba,
}

/// Something applied to a plan node.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanModifier {
    /// Where the node goes, from its container's origin.
    Offset {
        x: f32,
        y: f32,
    },
    /// The node's size, whatever its container would allow.
    RequiredSize {
        width: f32,
        height: f32,
    },
    /// The width a text run is laid out in. Its height is the renderer's.
    Width(f32),
    /// Applies to the node and everything inside it as one group: the group is composed
    /// first and faded once.
    Alpha(f32),
    /// Rounds every corner by the same radius.
    Shape {
        radius: f32,
    },
    /// Rounds each corner by its own radius.
    CornerEach(Corners<f32>),
    /// One outer shadow, in the node's shape. A box with several shadows has several,
    /// back to front.
    Shadow {
        x: f32,
        y: f32,
        blur: f32,
        spread: f32,
        color: Rgba,
    },
    Background(Rgba),
    /// One background image or gradient, drawn over the background colour. A box with
    /// several has several, back to front (the reverse of CSS's order).
    BackgroundBrush {
        brush: BrushId,
        /// Nothing of the brush is drawn outside this, from the node's top-left corner.
        area: Rect,
        /// Where one tile of the brush goes, from the node's top-left corner.
        tile: Rect,
        repeat_x: TileRepeat,
        repeat_y: TileRepeat,
    },
    /// The same width and colour on all four sides.
    Border {
        width: f32,
        color: Rgba,
    },
    /// A width and colour per side, when they are not all the same.
    BorderEach(Sides<BorderSide>),
    /// Clips the node's content to its bounds, in its shape when it has one.
    Clip,
}

/// Which modifier of a node a change is about. A node has at most one modifier per slot;
/// shadows are told apart by their place among the node's shadows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModifierSlot {
    Offset,
    RequiredSize,
    Width,
    Alpha,
    Shape,
    CornerEach,
    Shadow(usize),
    Background,
    /// Told apart by their place among the node's brushes, back to front.
    BackgroundBrush(usize),
    Border,
    BorderEach,
    Clip,
}

/// What a renderer changes to turn one plan into the next.
///
/// [`diff`] lists removals first, then walks the new tree: for a container, the new order
/// of the children it kept, then its insertions by ascending index, so that applying them
/// in the order given yields the new children list.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanChange {
    /// The node and everything inside it are gone.
    Removed { key: PlanKey },
    /// The children `parent` kept, in their new order. Children inserted in this update are
    /// not listed.
    Reordered {
        parent: PlanKey,
        order: Vec<PlanKey>,
    },
    /// A new node, with everything inside it, at `index` in its parent's new children.
    Inserted {
        parent: PlanKey,
        index: usize,
        node: PlanNode,
    },
    /// The node's kind carries new data (a text run's text or style, a field's value).
    KindChanged { key: PlanKey, kind: PlanKind },
    /// The modifier in `slot` is new or has a new value.
    ModifierSet {
        key: PlanKey,
        slot: ModifierSlot,
        modifier: PlanModifier,
    },
    /// The node no longer has a modifier in `slot`.
    ModifierRemoved { key: PlanKey, slot: ModifierSlot },
}

/// Which colour of a node is being resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColourUse {
    Background,
    Border,
    Shadow,
    Text,
    Field,
}

/// Decides how each colour of the plan is expressed.
///
/// This is where a colour that came from a theme variable can be replaced by the colour
/// role it stands for, so it follows the theme without the page being laid out again. The
/// display list carries only the computed colour, so a resolver that maps variables needs
/// to be told which variable a node's colour came from; [`LiteralColours`], the default,
/// keeps every colour as it is.
pub trait ColourResolver {
    fn resolve(&mut self, node: NodeId, used_for: ColourUse, colour: Rgba) -> Rgba;
}

/// Keeps every colour literal.
#[derive(Clone, Copy, Debug, Default)]
pub struct LiteralColours;

impl ColourResolver for LiteralColours {
    fn resolve(&mut self, _node: NodeId, _used_for: ColourUse, colour: Rgba) -> Rgba {
        colour
    }
}

impl Plan {
    /// The brushes this plan uses that `previous` did not: the ones a renderer has not
    /// registered yet. The other way round, `previous.brushes_not_in(self)`, they are the
    /// ones it can release.
    pub fn brushes_not_in(&self, previous: &Plan) -> Vec<(BrushId, &Brush)> {
        self.brushes
            .iter()
            .filter(|(id, _)| !previous.brushes.contains_key(id))
            .map(|(id, brush)| (*id, brush))
            .collect()
    }

    /// The node under `key`, anywhere in the tree.
    pub fn find(&self, key: PlanKey) -> Option<&PlanNode> {
        find_in(&self.root, key)
    }

    /// The node `key` is a child of; `None` for the page or a key not in the plan.
    pub fn parent_of(&self, key: PlanKey) -> Option<&PlanNode> {
        parent_in(&self.root, key)
    }

    /// Every node, parents before children, back to front.
    pub fn nodes(&self) -> Vec<&PlanNode> {
        let mut out = Vec::new();
        collect_nodes(&self.root, &mut out);
        out
    }
}

impl PlanNode {
    /// The modifier in `slot`, if the node has one.
    pub fn modifier(&self, slot: ModifierSlot) -> Option<&PlanModifier> {
        slots(&self.modifiers)
            .into_iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, modifier)| modifier)
    }

    /// The node's [`PlanModifier::Offset`], `(0, 0)` when it has none.
    pub fn offset(&self) -> (f32, f32) {
        match self.modifier(ModifierSlot::Offset) {
            Some(PlanModifier::Offset { x, y }) => (*x, *y),
            _ => (0.0, 0.0),
        }
    }
}

fn find_in(node: &PlanNode, key: PlanKey) -> Option<&PlanNode> {
    if node.key == key {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_in(child, key))
}

fn parent_in(node: &PlanNode, key: PlanKey) -> Option<&PlanNode> {
    if node.children.iter().any(|child| child.key == key) {
        return Some(node);
    }
    node.children.iter().find_map(|child| parent_in(child, key))
}

fn collect_nodes<'a>(node: &'a PlanNode, out: &mut Vec<&'a PlanNode>) {
    out.push(node);
    for child in &node.children {
        collect_nodes(child, out);
    }
}

/// Builds the plan for a display list, with colours kept literal and no images.
pub fn plan_from(list: &DisplayList) -> Plan {
    plan_from_with(list, &mut LiteralColours)
}

/// Builds the plan for a display list, expressing colours through `colours`. No image
/// draws: see [`plan_from_images`].
pub fn plan_from_with(list: &DisplayList, colours: &mut dyn ColourResolver) -> Plan {
    plan_from_images(list, colours, None)
}

/// Builds the plan for a display list, expressing colours through `colours` and drawing
/// each image with the asset `images` names for its URL. Without `images`, or for a URL it
/// names no asset for, an image draws nothing.
pub fn plan_from_images(
    list: &DisplayList,
    colours: &mut dyn ColourResolver,
    images: Option<&mut dyn ImageResolver>,
) -> Plan {
    // Reborrowed so all three share the builder's one lifetime: a `&mut dyn` is invariant,
    // so the resolvers' own lifetimes cannot simply be shortened in place.
    let images = images.map(|images| -> &mut dyn ImageResolver { &mut *images });
    Builder::new(list, &mut *colours, images).build()
}

/// A plan node while the tree is being built: children are indices into the arena.
struct Slot {
    key: PlanKey,
    kind: PlanKind,
    modifiers: Vec<PlanModifier>,
    children: Vec<usize>,
    parent: Option<usize>,
    /// What a child placed in this node is measured from, in document coordinates.
    origin: (f32, f32),
    /// The clip this node imposes on what is placed in it, in document coordinates.
    clip: Option<Rect>,
    /// The accumulated opacity of what is placed in it.
    opacity: f32,
    /// What the node itself draws, for the overlap test; `None` for containers that draw
    /// nothing.
    drawn: Option<Rect>,
}

struct Builder<'a> {
    entries: &'a [NodeEntry],
    colours: &'a mut dyn ColourResolver,
    images: Option<&'a mut dyn ImageResolver>,
    brushes: BTreeMap<BrushId, Brush>,
    by_node: HashMap<NodeId, usize>,
    /// Per entry: what it and every box inside it draw.
    subtree: Vec<Option<Rect>>,
    slots: Vec<Slot>,
    /// Per placed entry: the slot its descendants go into.
    content: HashMap<NodeId, usize>,
}

impl<'a> Builder<'a> {
    fn new(
        list: &'a DisplayList,
        colours: &'a mut dyn ColourResolver,
        images: Option<&'a mut dyn ImageResolver>,
    ) -> Self {
        let entries = &list.entries[..];
        let by_node: HashMap<NodeId, usize> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.node, index))
            .collect();
        let mut subtree: Vec<Option<Rect>> = entries.iter().map(drawn_bounds).collect();
        for (index, entry) in entries.iter().enumerate() {
            let Some(own) = subtree[index] else {
                continue;
            };
            let mut seen = HashSet::from([entry.node]);
            let mut parent = entry.parent;
            while let Some(id) = parent {
                if !seen.insert(id) {
                    break;
                }
                let Some(&at) = by_node.get(&id) else {
                    break;
                };
                subtree[at] = union(subtree[at], Some(own));
                parent = entries[at].parent;
            }
        }
        let page = Slot {
            key: PlanKey::Page,
            kind: PlanKind::AbsoluteBox,
            modifiers: Vec::new(),
            children: Vec::new(),
            parent: None,
            origin: (0.0, 0.0),
            clip: None,
            opacity: 1.0,
            drawn: None,
        };
        Builder {
            entries,
            colours,
            images,
            brushes: BTreeMap::new(),
            by_node,
            subtree,
            slots: vec![page],
            content: HashMap::new(),
        }
    }

    fn build(mut self) -> Plan {
        for index in 0..self.entries.len() {
            self.place(index);
        }
        let root = self.finish(0);
        Plan {
            root,
            brushes: std::mem::take(&mut self.brushes),
        }
    }

    fn push(&mut self, parent: usize, slot: Slot) -> usize {
        let index = self.slots.len();
        self.slots.push(Slot {
            parent: Some(parent),
            ..slot
        });
        self.slots[parent].children.push(index);
        index
    }

    /// The container an entry goes into: the slot of the nearest ancestor where it can be
    /// drawn early without covering or being covered by anything drawn in between.
    fn container_for(&self, index: usize) -> usize {
        let entry = &self.entries[index];
        let bounds = self.subtree[index];
        let mut seen = HashSet::from([entry.node]);
        let mut parent = entry.parent;
        while let Some(id) = parent {
            if !seen.insert(id) {
                break;
            }
            if let Some(&container) = self.content.get(&id) {
                if !self.drawn_after(container, bounds) {
                    return container;
                }
            }
            parent = self
                .by_node
                .get(&id)
                .and_then(|&at| self.entries[at].parent);
        }
        0
    }

    /// Whether anything placed after the end of `container`'s subtree, in tree order,
    /// overlaps `bounds`. Those are the boxes a child appended to `container` would be
    /// drawn underneath.
    fn drawn_after(&self, container: usize, bounds: Option<Rect>) -> bool {
        let Some(bounds) = bounds else {
            return false;
        };
        let mut at = container;
        while let Some(parent) = self.slots[at].parent {
            let siblings = &self.slots[parent].children;
            let position = siblings
                .iter()
                .position(|&child| child == at)
                .expect("a slot is among its parent's children");
            for &later in &siblings[position + 1..] {
                if self.subtree_overlaps(later, &bounds) {
                    return true;
                }
            }
            at = parent;
        }
        false
    }

    fn subtree_overlaps(&self, slot: usize, bounds: &Rect) -> bool {
        if self.slots[slot]
            .drawn
            .is_some_and(|drawn| overlaps(&drawn, bounds))
        {
            return true;
        }
        self.slots[slot]
            .children
            .iter()
            .any(|&child| self.subtree_overlaps(child, bounds))
    }

    fn place(&mut self, index: usize) {
        let entries = self.entries;
        let entry = &entries[index];
        let mut container = self.container_for(index);

        // The clip this entry's ancestors impose, where its container does not already.
        let inherited = entry.clip.filter(|clip| {
            !self.slots[container]
                .clip
                .is_some_and(|provided| same_rect(&provided, clip))
        });
        if let Some(clip) = inherited {
            let (ox, oy) = self.slots[container].origin;
            let opacity = self.slots[container].opacity;
            container = self.push(
                container,
                Slot {
                    key: PlanKey::InheritedClip(entry.node),
                    kind: PlanKind::AbsoluteBox,
                    modifiers: vec![
                        PlanModifier::Offset {
                            x: clip.x - ox,
                            y: clip.y - oy,
                        },
                        PlanModifier::RequiredSize {
                            width: clip.width,
                            height: clip.height,
                        },
                        PlanModifier::Clip,
                    ],
                    children: Vec::new(),
                    parent: None,
                    origin: (clip.x, clip.y),
                    clip: Some(clip),
                    opacity,
                    drawn: None,
                },
            );
        }

        let (ox, oy) = self.slots[container].origin;
        let provided_opacity = self.slots[container].opacity;
        let provided_clip = self.slots[container].clip;
        let rect = entry.rect;

        let mut modifiers = vec![
            PlanModifier::Offset {
                x: rect.x - ox,
                y: rect.y - oy,
            },
            PlanModifier::RequiredSize {
                width: rect.width,
                height: rect.height,
            },
        ];
        // The entry's opacity includes every ancestor's; the container already applies
        // the part that belongs to them.
        if provided_opacity > 0.0 {
            let own = entry.opacity / provided_opacity;
            if own < 1.0 - 1e-4 {
                modifiers.push(PlanModifier::Alpha(own.max(0.0)));
            }
        }
        if entry.visible {
            self.decorate(entry, &mut modifiers);
        }

        let border = entry.border.map(|border| border.widths);
        let has_border = border.is_some_and(|widths| {
            widths.top > 0.0 || widths.right > 0.0 || widths.bottom > 0.0 || widths.left > 0.0
        });
        let own_clip = entry.clips_children && entry.scroll.is_none() && !has_border;
        if own_clip {
            modifiers.push(PlanModifier::Clip);
        }

        let node = self.push(
            container,
            Slot {
                key: PlanKey::Node(entry.node),
                kind: PlanKind::AbsoluteBox,
                modifiers,
                children: Vec::new(),
                parent: None,
                origin: (rect.x, rect.y),
                clip: if own_clip {
                    Some(intersect(provided_clip, rect))
                } else {
                    provided_clip
                },
                opacity: entry.opacity,
                drawn: drawn_bounds(entry),
            },
        );

        let content = if let Some(scroll) = entry.scroll {
            let viewport = scroll.viewport;
            let content_width = scroll.content_width.max(viewport.width);
            let content_height = scroll.content_height.max(viewport.height);
            let clip = Some(intersect(provided_clip, viewport));
            let viewport_slot = |key: PlanKey, width: f32, height: f32, offset: (f32, f32)| Slot {
                key,
                kind: match key {
                    PlanKey::ScrollRow(_) => PlanKind::ScrollRow,
                    _ => PlanKind::ScrollColumn,
                },
                modifiers: vec![
                    PlanModifier::Offset {
                        x: offset.0,
                        y: offset.1,
                    },
                    PlanModifier::RequiredSize { width, height },
                    PlanModifier::Clip,
                ],
                children: Vec::new(),
                parent: None,
                origin: (viewport.x, viewport.y),
                clip,
                opacity: entry.opacity,
                drawn: None,
            };
            let inset = (viewport.x - rect.x, viewport.y - rect.y);
            let mut outer = node;
            if scroll.vertical {
                outer = self.push(
                    outer,
                    viewport_slot(
                        PlanKey::ScrollColumn(entry.node),
                        viewport.width,
                        viewport.height,
                        inset,
                    ),
                );
            }
            if scroll.horizontal {
                // Inside a vertical scroll, the row is as tall as the content it scrolls
                // sideways, so that the column has that height to scroll through.
                let (height, offset) = if scroll.vertical {
                    (content_height, (0.0, 0.0))
                } else {
                    (viewport.height, inset)
                };
                outer = self.push(
                    outer,
                    viewport_slot(
                        PlanKey::ScrollRow(entry.node),
                        viewport.width,
                        height,
                        offset,
                    ),
                );
            }
            self.push(
                outer,
                Slot {
                    key: PlanKey::ScrollContent(entry.node),
                    kind: PlanKind::AbsoluteBox,
                    modifiers: vec![
                        PlanModifier::Offset { x: 0.0, y: 0.0 },
                        PlanModifier::RequiredSize {
                            width: content_width,
                            height: content_height,
                        },
                    ],
                    children: Vec::new(),
                    parent: None,
                    origin: (viewport.x, viewport.y),
                    clip,
                    opacity: entry.opacity,
                    drawn: None,
                },
            )
        } else if entry.clips_children && has_border {
            let widths = border.unwrap_or_default();
            let padding = rect.inset(&widths);
            let mut modifiers = vec![
                PlanModifier::Offset {
                    x: widths.left,
                    y: widths.top,
                },
                PlanModifier::RequiredSize {
                    width: padding.width,
                    height: padding.height,
                },
            ];
            if let Some(radii) = entry.radii {
                push_shape(&mut modifiers, inner_radii(&radii, &widths));
            }
            modifiers.push(PlanModifier::Clip);
            self.push(
                node,
                Slot {
                    key: PlanKey::ClipContent(entry.node),
                    kind: PlanKind::AbsoluteBox,
                    modifiers,
                    children: Vec::new(),
                    parent: None,
                    origin: (padding.x, padding.y),
                    clip: Some(intersect(provided_clip, padding)),
                    opacity: entry.opacity,
                    drawn: None,
                },
            )
        } else {
            node
        };

        // A field draws its own text; text laid out inside it would be drawn twice.
        if entry.input.is_none() {
            self.add_texts(entry, content);
        }
        self.add_field(entry, content);
        self.add_image(entry, content);
        self.content.insert(entry.node, content);
    }

    fn decorate(&mut self, entry: &NodeEntry, modifiers: &mut Vec<PlanModifier>) {
        let node = entry.node;
        if let Some(radii) = entry.radii {
            push_shape(
                modifiers,
                Corners {
                    top_left: one_radius(radii.top_left),
                    top_right: one_radius(radii.top_right),
                    bottom_right: one_radius(radii.bottom_right),
                    bottom_left: one_radius(radii.bottom_left),
                },
            );
        }
        for shadow in entry.shadows.iter().filter(|shadow| !shadow.inset) {
            modifiers.push(PlanModifier::Shadow {
                x: shadow.offset_x,
                y: shadow.offset_y,
                blur: shadow.blur,
                spread: shadow.spread,
                color: self.colours.resolve(node, ColourUse::Shadow, shadow.color),
            });
        }
        if let Some(background) = entry.background {
            modifiers.push(PlanModifier::Background(self.colours.resolve(
                node,
                ColourUse::Background,
                background,
            )));
        }
        // CSS lists the top layer first; a renderer draws modifiers back to front.
        for layer in entry.backgrounds.iter().rev() {
            let brush = match &layer.image {
                BackgroundImage::Url(url) => {
                    match self.images.as_mut().and_then(|images| images.resolve(url)) {
                        Some(asset) => Brush::Image(asset),
                        None => continue,
                    }
                }
                BackgroundImage::Linear(gradient) => Brush::LinearGradient {
                    start: gradient.start,
                    end: gradient.end,
                    stops: self.stops(node, &gradient.stops),
                    repeating: gradient.repeating,
                },
                BackgroundImage::Radial(gradient) => Brush::RadialGradient {
                    center: gradient.center,
                    radius_x: gradient.radius_x,
                    radius_y: gradient.radius_y,
                    stops: self.stops(node, &gradient.stops),
                    repeating: gradient.repeating,
                },
            };
            let id = brush.id();
            self.brushes.entry(id).or_insert(brush);
            let relative = |rect: Rect| {
                Rect::new(
                    rect.x - entry.rect.x,
                    rect.y - entry.rect.y,
                    rect.width,
                    rect.height,
                )
            };
            modifiers.push(PlanModifier::BackgroundBrush {
                brush: id,
                area: relative(layer.area),
                tile: relative(layer.tile),
                repeat_x: layer.repeat_x,
                repeat_y: layer.repeat_y,
            });
        }
        if let Some(border) = entry.border {
            let mut side = |width: f32, color: Rgba| BorderSide {
                width,
                color: self.colours.resolve(node, ColourUse::Border, color),
            };
            let sides = Sides {
                top: side(border.widths.top, border.colors.top),
                right: side(border.widths.right, border.colors.right),
                bottom: side(border.widths.bottom, border.colors.bottom),
                left: side(border.widths.left, border.colors.left),
            };
            let all = [sides.top, sides.right, sides.bottom, sides.left];
            if all.iter().all(|side| side.width <= 0.0) {
                // Nothing to draw.
            } else if all
                .iter()
                .all(|side| close(side.width, sides.top.width) && side.color == sides.top.color)
            {
                modifiers.push(PlanModifier::Border {
                    width: sides.top.width,
                    color: sides.top.color,
                });
            } else {
                modifiers.push(PlanModifier::BorderEach(sides));
            }
        }
    }

    /// Gradient stops with their colours expressed through the colour resolver.
    fn stops(&mut self, node: NodeId, stops: &[GradientStop]) -> Vec<GradientStop> {
        stops
            .iter()
            .map(|stop| GradientStop {
                offset: stop.offset,
                color: self
                    .colours
                    .resolve(node, ColourUse::Background, stop.color),
            })
            .collect()
    }

    /// The image of an `<img>`, when the resolver names an asset for it.
    fn add_image(&mut self, entry: &NodeEntry, content: usize) {
        let Some(image) = entry.image.as_ref() else {
            return;
        };
        let Some(asset) = self
            .images
            .as_mut()
            .and_then(|images| images.resolve(&image.source))
        else {
            return;
        };
        let (ox, oy) = self.slots[content].origin;
        let clip = self.slots[content].clip;
        let opacity = self.slots[content].opacity;
        let bounds = image.content_rect;
        let draw = Rect::new(
            image.image_rect.x - bounds.x,
            image.image_rect.y - bounds.y,
            image.image_rect.width,
            image.image_rect.height,
        );
        let mut modifiers = vec![
            PlanModifier::Offset {
                x: bounds.x - ox,
                y: bounds.y - oy,
            },
            PlanModifier::RequiredSize {
                width: bounds.width,
                height: bounds.height,
            },
        ];
        let spills = draw.x < -EPSILON
            || draw.y < -EPSILON
            || draw.right() > bounds.width + EPSILON
            || draw.bottom() > bounds.height + EPSILON;
        if spills {
            modifiers.push(PlanModifier::Clip);
        }
        self.push(
            content,
            Slot {
                key: PlanKey::Image(entry.node),
                kind: PlanKind::Image(PlanImage {
                    source: image.source.clone(),
                    asset,
                    draw,
                }),
                modifiers,
                children: Vec::new(),
                parent: None,
                origin: (bounds.x, bounds.y),
                clip,
                opacity,
                drawn: None,
            },
        );
    }

    fn add_texts(&mut self, entry: &NodeEntry, content: usize) {
        let (ox, oy) = self.slots[content].origin;
        let clip = self.slots[content].clip;
        let opacity = self.slots[content].opacity;
        for (index, run) in entry.texts.iter().enumerate() {
            let color = self.colours.resolve(run.owner, ColourUse::Text, run.color);
            self.push(
                content,
                Slot {
                    key: PlanKey::Text {
                        node: entry.node,
                        index,
                    },
                    kind: PlanKind::Text(PlanText {
                        text: run.text.clone(),
                        style: run.style.clone(),
                        color,
                        align: run.align,
                        soft_wrap: run.wrap_width.is_some(),
                    }),
                    modifiers: vec![
                        PlanModifier::Offset {
                            x: run.rect.x - ox,
                            y: run.rect.y - oy,
                        },
                        PlanModifier::Width(run.rect.width),
                    ],
                    children: Vec::new(),
                    parent: None,
                    origin: (run.rect.x, run.rect.y),
                    clip,
                    opacity,
                    drawn: None,
                },
            );
        }
    }

    fn add_field(&mut self, entry: &NodeEntry, content: usize) {
        let Some(input) = entry.input.as_ref() else {
            return;
        };
        let (ox, oy) = self.slots[content].origin;
        let clip = self.slots[content].clip;
        let opacity = self.slots[content].opacity;
        let field = input.content_rect;
        let kind = match &input.kind {
            InputKind::Checkbox { checked } => PlanKind::Checkbox { checked: *checked },
            InputKind::Radio { checked } => PlanKind::RadioButton { selected: *checked },
            kind => PlanKind::TextField(PlanTextField {
                input: kind.clone(),
                value: input.value.clone(),
                placeholder: input.placeholder.clone(),
                style: input.style.clone(),
                color: self
                    .colours
                    .resolve(entry.node, ColourUse::Field, input.color),
                multiline: matches!(kind, InputKind::TextArea),
            }),
        };
        self.push(
            content,
            Slot {
                key: PlanKey::Field(entry.node),
                kind,
                modifiers: vec![
                    PlanModifier::Offset {
                        x: field.x - ox,
                        y: field.y - oy,
                    },
                    PlanModifier::RequiredSize {
                        width: field.width,
                        height: field.height,
                    },
                ],
                children: Vec::new(),
                parent: None,
                origin: (field.x, field.y),
                clip,
                opacity,
                drawn: None,
            },
        );
    }

    fn finish(&self, slot: usize) -> PlanNode {
        let source = &self.slots[slot];
        let children: Vec<PlanNode> = source
            .children
            .iter()
            .map(|&child| self.finish(child))
            .collect();
        let kind = match (&source.key, &source.kind) {
            // A box with nothing inside it has no children to place.
            (PlanKey::Node(_), PlanKind::AbsoluteBox) if children.is_empty() => PlanKind::Box,
            (_, kind) => kind.clone(),
        };
        PlanNode {
            key: source.key,
            kind,
            modifiers: source.modifiers.clone(),
            children,
        }
    }
}

/// A corner's single radius. Compose's rounded shapes take one radius per corner; an
/// elliptical CSS corner is drawn with the smaller of its two.
fn one_radius(radius: Radius) -> f32 {
    if radius.is_zero() {
        0.0
    } else {
        radius.x.min(radius.y)
    }
}

/// The radii of the padding box's corners: the outer radii less the adjoining borders.
fn inner_radii(radii: &Corners<Radius>, widths: &Sides<f32>) -> Corners<f32> {
    let inner = |radius: Radius, horizontal: f32, vertical: f32| {
        if radius.is_zero() {
            0.0
        } else {
            (radius.x - horizontal).min(radius.y - vertical).max(0.0)
        }
    };
    Corners {
        top_left: inner(radii.top_left, widths.left, widths.top),
        top_right: inner(radii.top_right, widths.right, widths.top),
        bottom_right: inner(radii.bottom_right, widths.right, widths.bottom),
        bottom_left: inner(radii.bottom_left, widths.left, widths.bottom),
    }
}

/// One radius when every corner has it, a radius per corner otherwise, nothing when all
/// corners are square.
fn push_shape(modifiers: &mut Vec<PlanModifier>, corners: Corners<f32>) {
    let all = [
        corners.top_left,
        corners.top_right,
        corners.bottom_right,
        corners.bottom_left,
    ];
    if all.iter().all(|radius| *radius <= 0.0) {
        return;
    }
    if all.iter().all(|radius| close(*radius, corners.top_left)) {
        modifiers.push(PlanModifier::Shape {
            radius: corners.top_left,
        });
    } else {
        modifiers.push(PlanModifier::CornerEach(corners));
    }
}

/// What an entry draws by itself, clipped by its ancestors: its box, its outer shadows,
/// its text and its field. `None` when it draws nothing.
fn drawn_bounds(entry: &NodeEntry) -> Option<Rect> {
    if !entry.visible {
        return None;
    }
    let mut bounds = None;
    let decorated =
        entry.background.is_some() || entry.border.is_some() || !entry.backgrounds.is_empty();
    if decorated {
        bounds = union(bounds, Some(entry.rect));
    }
    for shadow in entry.shadows.iter().filter(|shadow| !shadow.inset) {
        let reach = shadow.blur.max(0.0) + shadow.spread;
        let rect = entry.rect;
        bounds = union(
            bounds,
            Some(Rect::new(
                rect.x + shadow.offset_x - reach,
                rect.y + shadow.offset_y - reach,
                rect.width + 2.0 * reach,
                rect.height + 2.0 * reach,
            )),
        );
    }
    for run in &entry.texts {
        bounds = union(bounds, Some(run.rect));
    }
    if entry.input.is_some() {
        bounds = union(bounds, Some(entry.rect));
    }
    // Whether the image is drawn depends on the resolver; counting it either way only
    // makes the nesting more careful.
    if let Some(image) = &entry.image {
        bounds = union(bounds, Some(image.content_rect));
    }
    match (bounds, entry.clip) {
        (Some(bounds), Some(clip)) => {
            let clipped = bounds.intersect(&clip);
            (clipped.width > 0.0 && clipped.height > 0.0).then_some(clipped)
        }
        (bounds, _) => bounds,
    }
}

fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let x = a.x.min(b.x);
            let y = a.y.min(b.y);
            Some(Rect::new(
                x,
                y,
                a.right().max(b.right()) - x,
                a.bottom().max(b.bottom()) - y,
            ))
        }
        (a, None) => a,
        (None, b) => b,
    }
}

fn intersect(clip: Option<Rect>, rect: Rect) -> Rect {
    match clip {
        Some(clip) => clip.intersect(&rect),
        None => rect,
    }
}

/// Whether two rectangles share some area. Touching edges do not count.
fn overlaps(a: &Rect, b: &Rect) -> bool {
    let shared = a.intersect(b);
    shared.width > EPSILON && shared.height > EPSILON
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < EPSILON
}

fn same_rect(a: &Rect, b: &Rect) -> bool {
    close(a.x, b.x) && close(a.y, b.y) && close(a.width, b.width) && close(a.height, b.height)
}

/// The modifiers of a node by slot.
fn slots(modifiers: &[PlanModifier]) -> Vec<(ModifierSlot, &PlanModifier)> {
    let mut shadows = 0;
    let mut brushes = 0;
    modifiers
        .iter()
        .map(|modifier| {
            let slot = match modifier {
                PlanModifier::Offset { .. } => ModifierSlot::Offset,
                PlanModifier::RequiredSize { .. } => ModifierSlot::RequiredSize,
                PlanModifier::Width(_) => ModifierSlot::Width,
                PlanModifier::Alpha(_) => ModifierSlot::Alpha,
                PlanModifier::Shape { .. } => ModifierSlot::Shape,
                PlanModifier::CornerEach(_) => ModifierSlot::CornerEach,
                PlanModifier::Shadow { .. } => {
                    shadows += 1;
                    ModifierSlot::Shadow(shadows - 1)
                }
                PlanModifier::Background(_) => ModifierSlot::Background,
                PlanModifier::BackgroundBrush { .. } => {
                    brushes += 1;
                    ModifierSlot::BackgroundBrush(brushes - 1)
                }
                PlanModifier::Border { .. } => ModifierSlot::Border,
                PlanModifier::BorderEach(_) => ModifierSlot::BorderEach,
                PlanModifier::Clip => ModifierSlot::Clip,
            };
            (slot, modifier)
        })
        .collect()
}

/// A plan node with its parent's key.
struct Indexed<'a> {
    parent: Option<PlanKey>,
    node: &'a PlanNode,
}

fn index_plan<'a>(
    node: &'a PlanNode,
    parent: Option<PlanKey>,
    out: &mut HashMap<PlanKey, Indexed<'a>>,
    order: &mut Vec<PlanKey>,
) {
    out.insert(node.key, Indexed { parent, node });
    order.push(node.key);
    for child in &node.children {
        index_plan(child, Some(node.key), out, order);
    }
}

/// What changed from `previous` to `next`, node by node.
///
/// A node is kept when the same key is under the same parent, that parent is kept, and the
/// node is still the same kind of element; then only its kind's data and the modifiers
/// that differ are reported, one change per modifier. A node that is new, that moved to
/// another parent, or that became another kind of element is reported removed (where it
/// was) and inserted (where it is), with everything inside it.
pub fn diff(previous: &Plan, next: &Plan) -> Vec<PlanChange> {
    let mut old = HashMap::new();
    let mut old_order = Vec::new();
    index_plan(&previous.root, None, &mut old, &mut old_order);
    let mut new = HashMap::new();
    let mut new_order = Vec::new();
    index_plan(&next.root, None, &mut new, &mut new_order);

    let mut kept: HashMap<PlanKey, bool> = HashMap::new();
    for &key in &new_order {
        is_kept(key, &old, &new, &mut kept);
    }

    let mut changes = Vec::new();
    for &key in &old_order {
        let still_here = kept.get(&key).copied().unwrap_or(false);
        let parent_kept = old[&key]
            .parent
            .is_none_or(|parent| kept.get(&parent).copied().unwrap_or(false));
        if !still_here && parent_kept {
            changes.push(PlanChange::Removed { key });
        }
    }

    if kept.get(&next.root.key).copied().unwrap_or(false) {
        diff_node(&next.root, &old, &kept, &mut changes);
    } else {
        // A different root: nothing carries over.
        changes.push(PlanChange::Inserted {
            parent: next.root.key,
            index: 0,
            node: next.root.clone(),
        });
    }
    changes
}

fn is_kept(
    key: PlanKey,
    old: &HashMap<PlanKey, Indexed<'_>>,
    new: &HashMap<PlanKey, Indexed<'_>>,
    kept: &mut HashMap<PlanKey, bool>,
) -> bool {
    if let Some(&known) = kept.get(&key) {
        return known;
    }
    let (Some(before), Some(after)) = (old.get(&key), new.get(&key)) else {
        kept.insert(key, false);
        return false;
    };
    let result = before.parent == after.parent
        && before.node.kind.same_variant(&after.node.kind)
        && after
            .parent
            .is_none_or(|parent| is_kept(parent, old, new, kept));
    kept.insert(key, result);
    result
}

fn diff_node(
    node: &PlanNode,
    old: &HashMap<PlanKey, Indexed<'_>>,
    kept: &HashMap<PlanKey, bool>,
    changes: &mut Vec<PlanChange>,
) {
    let before = old[&node.key].node;
    if before.kind != node.kind {
        changes.push(PlanChange::KindChanged {
            key: node.key,
            kind: node.kind.clone(),
        });
    }

    let old_slots = slots(&before.modifiers);
    let new_slots = slots(&node.modifiers);
    for (slot, _) in &old_slots {
        if !new_slots.iter().any(|(s, _)| s == slot) {
            changes.push(PlanChange::ModifierRemoved {
                key: node.key,
                slot: *slot,
            });
        }
    }
    for (slot, modifier) in &new_slots {
        let unchanged = old_slots
            .iter()
            .any(|(s, previous)| s == slot && *previous == *modifier);
        if !unchanged {
            changes.push(PlanChange::ModifierSet {
                key: node.key,
                slot: *slot,
                modifier: (*modifier).clone(),
            });
        }
    }

    let stays = |key: &PlanKey| kept.get(key).copied().unwrap_or(false);
    let survivors: Vec<PlanKey> = node
        .children
        .iter()
        .map(|child| child.key)
        .filter(stays)
        .collect();
    let previous_survivors: Vec<PlanKey> = before
        .children
        .iter()
        .map(|child| child.key)
        .filter(stays)
        .collect();
    if survivors != previous_survivors {
        changes.push(PlanChange::Reordered {
            parent: node.key,
            order: survivors,
        });
    }

    for (index, child) in node.children.iter().enumerate() {
        if stays(&child.key) {
            diff_node(child, old, kept, changes);
        } else {
            changes.push(PlanChange::Inserted {
                parent: node.key,
                index,
                node: child.clone(),
            });
        }
    }
}
