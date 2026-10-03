//! The neutral display list: what a laid-out HTML document asks a renderer to draw.
//!
//! It names no CSS property and no selector. Every position is resolved to an absolute
//! rectangle in CSS pixels, every colour to RGBA, every length to a number. A renderer can
//! draw it without knowing that the source was HTML.
//!
//! Entries are keyed by blitz-dom node id and listed in paint order, back to front.

use std::collections::{HashMap, HashSet};

use crate::NodeId;
use crate::measure::{TextMetrics, TextStyle};

/// An 8-bit sRGB colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn is_transparent(&self) -> bool {
        self.a == 0
    }
}

/// An axis-aligned rectangle in CSS pixels, in document coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// Whether the point lies inside, counting the left and top edges but not the right
    /// and bottom ones, so two touching boxes never both contain a point.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// The overlap of two rectangles; empty (zero width or height) when they do not meet.
    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
    }

    /// The rectangle moved inwards by the given amounts on each side.
    pub fn inset(&self, sides: &Sides<f32>) -> Rect {
        Rect::new(
            self.x + sides.left,
            self.y + sides.top,
            (self.width - sides.left - sides.right).max(0.0),
            (self.height - sides.top - sides.bottom).max(0.0),
        )
    }
}

/// One value per side of a box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sides<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

/// One value per corner of a box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners<T> {
    pub top_left: T,
    pub top_right: T,
    pub bottom_right: T,
    pub bottom_left: T,
}

/// An elliptical corner radius: horizontal and vertical, in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Radius {
    pub x: f32,
    pub y: f32,
}

impl Radius {
    pub fn is_zero(&self) -> bool {
        self.x <= 0.0 || self.y <= 0.0
    }
}

/// How a border side is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderLine {
    #[default]
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
    /// `none` or `hidden`. Such a side has zero width, so it only appears when another
    /// side is drawn.
    None,
}

/// The four border sides of a box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    /// Widths in CSS pixels.
    pub widths: Sides<f32>,
    pub colors: Sides<Rgba>,
    pub lines: Sides<BorderLine>,
}

/// One `box-shadow` layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub color: Rgba,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub inset: bool,
}

/// A box whose content scrolls (`overflow: scroll` or `auto`).
///
/// The scroll position belongs to the renderer. Every entry inside the container is
/// positioned as if it were scrolled to the origin; the renderer moves the content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollContainer {
    /// The visible area: the padding box.
    pub viewport: Rect,
    /// Size of the scrollable content, measured from the padding box's origin.
    pub content_width: f32,
    pub content_height: f32,
    /// Whether each axis scrolls. An axis with `overflow: hidden` or `clip` clips without
    /// scrolling.
    pub horizontal: bool,
    pub vertical: bool,
}

/// Horizontal alignment of lines inside their box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

/// A run of text and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    /// The element the text belongs to: the innermost element that holds all of it. A
    /// pointer over this run is routed to this node.
    pub owner: NodeId,
    /// The text's own box: the lines' extent, not the element's.
    pub rect: Rect,
    /// The text with white space collapsed as CSS does; `\n` is a forced break. Text sized
    /// by the [`TextMeasurer`](crate::TextMeasurer) also has its `text-transform` applied,
    /// so it is the string the measurer was asked about.
    pub text: String,
    pub style: TextStyle,
    pub color: Rgba,
    pub align: TextAlign,
    /// The width the lines were broken at. `None` means the text is one line that was not
    /// constrained; the renderer draws it without wrapping.
    pub wrap_width: Option<f32>,
    pub line_count: u32,
    /// Distance from `rect.y` to the first baseline.
    pub baseline: f32,
}

/// What kind of form field an `<input>` or `<textarea>` is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputKind {
    Text,
    Password,
    Email,
    Number,
    Search,
    Tel,
    Url,
    Checkbox {
        checked: bool,
    },
    Radio {
        checked: bool,
    },
    TextArea,
    /// Any other `type`, by name.
    Other(String),
}

/// A form field to be drawn by the renderer's own text field.
#[derive(Clone, Debug, PartialEq)]
pub struct InputField {
    pub kind: InputKind,
    /// The value as the Host last set it. The renderer owns the field's text while the user
    /// edits it, so this is the starting value, not a mirror of every keystroke.
    pub value: String,
    pub placeholder: Option<String>,
    /// The content box, where the text goes.
    pub content_rect: Rect,
    pub style: TextStyle,
    pub color: Rgba,
}

/// A colour at a point along a gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Where the colour sits, as a fraction of the gradient line (linear) or of the
    /// horizontal radius (radial). Already in order, never decreasing; it may lie outside
    /// `0..=1` when the CSS put it there.
    pub offset: f32,
    pub color: Rgba,
}

/// A `linear-gradient()` or `repeating-linear-gradient()`, resolved against the tile it
/// fills.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    /// The direction in degrees, clockwise from pointing up, as CSS measures it. Keywords
    /// (`to right`, `to top left`) are already turned into the angle they mean for this
    /// tile's size.
    pub angle: f32,
    /// Where the gradient line starts and ends, from the tile's top-left corner. The
    /// line is as long as CSS makes it: the corners of the tile lie on the lines through
    /// its ends perpendicular to it.
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub stops: Vec<GradientStop>,
    /// The stops repeat along the line beyond the last one.
    pub repeating: bool,
}

/// A `radial-gradient()` or `repeating-radial-gradient()`, resolved against the tile it
/// fills.
#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    /// `circle` rather than `ellipse`. A circle's two radii are equal.
    pub circle: bool,
    /// The centre, from the tile's top-left corner.
    pub center: (f32, f32),
    /// The ending shape's radii. Extent keywords (`closest-side`, `farthest-corner`) are
    /// already resolved for this tile and centre.
    pub radius_x: f32,
    pub radius_y: f32,
    pub stops: Vec<GradientStop>,
    pub repeating: bool,
}

/// What one background layer draws.
#[derive(Clone, Debug, PartialEq)]
pub enum BackgroundImage {
    /// An image named by URL: as written, resolved against the document's base URL when
    /// there is one. Loading it is the application's business.
    Url(String),
    Linear(LinearGradient),
    Radial(RadialGradient),
}

/// How a background tile repeats along one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileRepeat {
    /// One tile, where `tile` says.
    NoRepeat,
    /// Tiles edge to edge in both directions from `tile`, as far as the painting area
    /// reaches. `round` arrives as this, with the tile already resized to fit a whole number
    /// of times.
    Repeat,
    /// `space`: tiles from the start of the positioning area with `gap` between them, as
    /// many whole tiles as fit.
    Space { gap: f32 },
}

/// One layer of a box's background, resolved to rectangles.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundLayer {
    pub image: BackgroundImage,
    /// The painting area (`background-clip`): nothing of the layer is drawn outside it.
    pub area: Rect,
    /// Where one tile goes (`background-origin`, `background-size` and
    /// `background-position` applied). Gradient geometry is measured from its top-left
    /// corner.
    pub tile: Rect,
    pub repeat_x: TileRepeat,
    pub repeat_y: TileRepeat,
}

/// `object-fit`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectFit {
    #[default]
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

/// The image an `<img>` element shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplacedImage {
    /// The `src`: as written, resolved against the document's base URL when there is one.
    pub source: String,
    pub alt: Option<String>,
    /// The content box, which the image is clipped to.
    pub content_rect: Rect,
    /// Where the image is drawn, with `object-fit` and `object-position` applied. It can
    /// reach outside the content box (`cover`, or `none` on a large image); what lies
    /// outside is not drawn. Without a natural size it is the content box.
    pub image_rect: Rect,
    pub fit: ObjectFit,
    /// What the application's [`ImageResolver`](crate::ImageResolver) answered for the
    /// image's natural size during layout, if anything.
    pub natural_size: Option<(f32, f32)>,
}

/// Everything one node contributes to the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeEntry {
    pub node: NodeId,
    /// The element's tag name, empty for an anonymous box blitz-dom made to hold text.
    pub tag: String,
    /// The border box.
    pub rect: Rect,
    /// `visibility: hidden` boxes keep their place in the list but draw nothing and are not
    /// hit.
    pub visible: bool,
    pub background: Option<Rgba>,
    /// `background-image` layers in CSS order: the first is drawn on top, and all of them
    /// over the background colour.
    pub backgrounds: Vec<BackgroundLayer>,
    pub border: Option<Border>,
    /// `None` when every corner is square. Radii are already reduced the way CSS reduces
    /// radii that would overlap.
    pub radii: Option<Corners<Radius>>,
    pub shadows: Vec<BoxShadow>,
    /// The clip ancestors impose on this box (the intersection of every clipping
    /// ancestor's padding box), or `None` when nothing clips it.
    pub clip: Option<Rect>,
    /// Whether this box clips its own descendants to its padding box (`overflow` other than
    /// `visible`).
    pub clips_children: bool,
    /// The product of this box's `opacity` and every ancestor's.
    pub opacity: f32,
    /// Present when this box scrolls its content.
    pub scroll: Option<ScrollContainer>,
    /// The nearest scrolling ancestor, whose scroll position moves this box.
    pub scroll_parent: Option<NodeId>,
    /// The nearest ancestor box, listed under the id its own entry has. Paint order alone
    /// does not say which box sits inside which (a positioned box can be painted after
    /// boxes that are not its ancestors), and a renderer that nests boxes needs to know.
    pub parent: Option<NodeId>,
    pub texts: Vec<TextRun>,
    pub input: Option<InputField>,
    /// Present for an `<img>` with a `src`.
    pub image: Option<ReplacedImage>,
}

/// A laid-out document, back to front.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub entries: Vec<NodeEntry>,
}

/// What changed between two display lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayListDiff {
    /// Entries that are new or whose content changed, in the new paint order.
    pub changed: Vec<NodeEntry>,
    /// Nodes that no longer draw anything.
    pub removed: Vec<NodeId>,
    /// The full paint order, present only when it changed (a node was added or removed, or
    /// two nodes swapped).
    pub order: Option<Vec<NodeId>>,
}

impl DisplayListDiff {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty() && self.order.is_none()
    }
}

impl DisplayList {
    pub fn get(&self, node: NodeId) -> Option<&NodeEntry> {
        self.entries.iter().find(|entry| entry.node == node)
    }

    /// The paint order as node ids.
    pub fn order(&self) -> Vec<NodeId> {
        self.entries.iter().map(|entry| entry.node).collect()
    }

    /// Only what differs from `previous`: entries that are new or changed, nodes that
    /// disappeared, and the order when it moved.
    pub fn diff_from(&self, previous: &DisplayList) -> DisplayListDiff {
        let old: HashMap<NodeId, &NodeEntry> = previous
            .entries
            .iter()
            .map(|entry| (entry.node, entry))
            .collect();
        let present: HashSet<NodeId> = self.entries.iter().map(|entry| entry.node).collect();
        let changed = self
            .entries
            .iter()
            .filter(|entry| old.get(&entry.node).is_none_or(|before| *before != *entry))
            .cloned()
            .collect();
        let removed = previous
            .entries
            .iter()
            .map(|entry| entry.node)
            .filter(|node| !present.contains(node))
            .collect();
        let order = self.order();
        let order = (order != previous.order()).then_some(order);
        DisplayListDiff {
            changed,
            removed,
            order,
        }
    }

    /// The node drawn on top at a point in document coordinates, or `None` over empty
    /// canvas.
    ///
    /// Over text this is the element that owns the text (a `<span>` inside a paragraph),
    /// otherwise the topmost visible box. Scroll positions are taken as zero; see
    /// [`DisplayList::hit_test_scrolled`].
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.hit_test_scrolled(x, y, |_| (0.0, 0.0))
    }

    /// Like [`DisplayList::hit_test`], for a renderer that has scrolled containers.
    /// `scroll_offset` gives each scroll container's current offset (how far its content has
    /// moved up and left), keyed by the container's node id.
    pub fn hit_test_scrolled(
        &self,
        x: f32,
        y: f32,
        scroll_offset: impl Fn(NodeId) -> (f32, f32),
    ) -> Option<NodeId> {
        let by_node: HashMap<NodeId, &NodeEntry> = self
            .entries
            .iter()
            .map(|entry| (entry.node, entry))
            .collect();
        // Total scroll applied to content inside `container`, through every scrolling
        // ancestor up the chain.
        let accumulated = |mut container: Option<NodeId>| {
            let (mut dx, mut dy) = (0.0f32, 0.0f32);
            while let Some(id) = container {
                let (sx, sy) = scroll_offset(id);
                dx += sx;
                dy += sy;
                container = by_node.get(&id).and_then(|entry| entry.scroll_parent);
            }
            (dx, dy)
        };
        for entry in self.entries.iter().rev() {
            if !entry.visible {
                continue;
            }
            let (dx, dy) = accumulated(entry.scroll_parent);
            // The point in the coordinates this entry was laid out in.
            let (px, py) = (x + dx, y + dy);
            if let Some(clip) = entry.clip {
                // The innermost clip usually comes from the scroll container itself, whose
                // padding box does not move with its own scroll position: test it in the
                // container's coordinates.
                let clip_space = entry
                    .scroll_parent
                    .and_then(|container| by_node.get(&container))
                    .and_then(|container| container.scroll_parent);
                let (cx, cy) = accumulated(clip_space);
                if !clip.contains(x + cx, y + cy) {
                    continue;
                }
            }
            if !entry.rect.contains(px, py) {
                continue;
            }
            if let Some(run) = entry.texts.iter().find(|run| run.rect.contains(px, py)) {
                return Some(run.owner);
            }
            return Some(entry.node);
        }
        None
    }
}

/// A text run measured during layout, before it is placed in the display list.
#[derive(Clone, Debug)]
pub(crate) struct MeasuredText {
    pub owner: NodeId,
    pub text: String,
    pub style: TextStyle,
    pub color: Rgba,
    pub align: TextAlign,
    pub metrics: TextMetrics,
    pub wrap_width: Option<f32>,
}
