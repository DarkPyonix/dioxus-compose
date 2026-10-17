//! The neutral display list: what a laid-out HTML document asks a renderer to draw.
//!
//! It names no CSS property and no selector. Every position is resolved to an absolute
//! rectangle in CSS pixels, every colour to RGBA, every length to a number. A renderer can
//! draw it without knowing that the source was HTML.
//!
//! Entries are keyed by blitz-dom node id and listed in paint order, back to front.

use std::collections::{HashMap, HashSet};

use crate::html::NodeId;
use crate::layout::measure::{TextMetrics, TextStyle};

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
///
/// The content size is measured from the boxes laid out inside the container: from the
/// padding box's origin to the furthest edge any of them (or their text) reaches, plus the
/// container's own padding on the far side, as a browser lets you scroll past the last
/// item to the padding. A box that clips its own content counts with its border box only,
/// and a `position: fixed` box does not count, since it does not move with the content.
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
    /// by the [`TextMeasurer`](crate::html::TextMeasurer) also has its `text-transform` applied,
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

/// What kind of form field an `<input>`, `<textarea>` or `<select>` is.
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
    /// A `<select>`: one choice out of a list.
    Select {
        /// What each `<option>` shows (its `label`, or its text), in document order, those
        /// inside an `<optgroup>` included.
        options: Vec<String>,
        /// The option chosen: the one whose value is the select's `value`, else the last
        /// one marked `selected`, else the first. `None` when there are no options.
        selected: Option<usize>,
    },
    /// Any other `type`, by name.
    Other(String),
}

/// A form field to be drawn by the renderer's own text field.
#[derive(Clone, Debug, PartialEq)]
pub struct InputField {
    pub kind: InputKind,
    /// The value as the Host last set it. The renderer owns the field's text while the user
    /// edits it, so this is the starting value, not a mirror of every keystroke. For a
    /// `<select>`, the chosen option's value.
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
    /// What the application's [`ImageResolver`](crate::html::ImageResolver) answered for the
    /// image's natural size during layout, if anything.
    pub natural_size: Option<(f32, f32)>,
}

/// A 2D affine matrix in CSS's order: a point `(x, y)` goes to
/// `(a * x + c * y + e, b * x + d * y + f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Matrix2D {
    pub const IDENTITY: Matrix2D = Matrix2D {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub const fn translation(x: f32, y: f32) -> Self {
        Matrix2D {
            e: x,
            f: y,
            ..Matrix2D::IDENTITY
        }
    }

    pub const fn scaling(x: f32, y: f32) -> Self {
        Matrix2D {
            a: x,
            d: y,
            ..Matrix2D::IDENTITY
        }
    }

    /// A clockwise rotation (the y axis points down), as CSS `rotate()` turns.
    pub fn rotation(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Matrix2D {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `skew(ax, ay)`: each angle in radians.
    pub fn skew(x_radians: f32, y_radians: f32) -> Self {
        Matrix2D {
            a: 1.0,
            b: y_radians.tan(),
            c: x_radians.tan(),
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `self` times `rhs`: `rhs` is applied to a point first, then `self`. This is how a
    /// CSS transform list composes, `transform: A B` being `A * B`.
    pub fn times(&self, rhs: &Matrix2D) -> Matrix2D {
        Matrix2D {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub fn determinant(&self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    /// The matrix that undoes this one; `None` when it flattens the plane (a scale by
    /// zero), which has no inverse.
    pub fn inverse(&self) -> Option<Matrix2D> {
        let det = self.determinant();
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let a = self.d / det;
        let b = -self.b / det;
        let c = -self.c / det;
        let d = self.a / det;
        Some(Matrix2D {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }

    /// Whether the matrix leaves every point where it is.
    pub fn is_identity(&self) -> bool {
        self.is_translation() && self.e.abs() < 1e-6 && self.f.abs() < 1e-6
    }

    /// Whether the matrix only moves points, without turning, scaling or skewing them.
    pub fn is_translation(&self) -> bool {
        (self.a - 1.0).abs() < 1e-6
            && self.b.abs() < 1e-6
            && self.c.abs() < 1e-6
            && (self.d - 1.0).abs() < 1e-6
    }

    /// Each component moved `progress` of the way to `other`'s.
    pub fn lerp(&self, other: &Matrix2D, progress: f32) -> Matrix2D {
        let mix = |from: f32, to: f32| from + (to - from) * progress;
        Matrix2D {
            a: mix(self.a, other.a),
            b: mix(self.b, other.b),
            c: mix(self.c, other.c),
            d: mix(self.d, other.d),
            e: mix(self.e, other.e),
            f: mix(self.f, other.f),
        }
    }
}

/// One function of a CSS transform list, resolved to numbers: lengths in CSS pixels and
/// angles in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformFunction {
    Translate(f32, f32),
    /// Clockwise, as CSS turns. Not reduced to a turn: `rotate(450deg)` is a quarter turn
    /// more than one full one, which an animation from `rotate(0deg)` plays.
    Rotate(f32),
    Scale(f32, f32),
    /// `skew(x, y)`.
    Skew(f32, f32),
    Matrix(Matrix2D),
}

impl TransformFunction {
    /// The matrix of the function.
    pub fn matrix(&self) -> Matrix2D {
        match *self {
            TransformFunction::Translate(x, y) => Matrix2D::translation(x, y),
            TransformFunction::Rotate(radians) => Matrix2D::rotation(radians),
            TransformFunction::Scale(x, y) => Matrix2D::scaling(x, y),
            TransformFunction::Skew(x, y) => Matrix2D::skew(x, y),
            TransformFunction::Matrix(matrix) => matrix,
        }
    }
}

/// A box's 2D `transform`: the matrix the CSS transform list multiplies out to, the list
/// itself, and the point the matrix is applied about (`transform-origin`).
///
/// The origin is measured from the box's own top-left corner, in CSS pixels. A transform
/// moves how a box is drawn and nothing else: its [`NodeEntry::rect`] stays where layout
/// put it, and so does every box inside it, which the transform carries along.
///
/// The list is kept because a matrix cannot say how far something turned, and an animation
/// needs that: `rotate(0deg)` and `rotate(360deg)` are the same matrix, and an animation
/// from one to the other is a full turn. A renderer interpolates two transforms
/// function by function when their lists have the same kinds of function in the same order,
/// as CSS does, and between their matrices otherwise.
#[derive(Clone, Debug, PartialEq)]
pub struct Transform2D {
    /// The product of the functions, in list order.
    pub matrix: Matrix2D,
    pub origin: (f32, f32),
    pub functions: Vec<TransformFunction>,
}

impl Transform2D {
    /// The transform of `functions` about `origin`.
    pub fn new(functions: Vec<TransformFunction>, origin: (f32, f32)) -> Self {
        let matrix = functions
            .iter()
            .fold(Matrix2D::IDENTITY, |total, function| {
                total.times(&function.matrix())
            });
        Transform2D {
            matrix,
            origin,
            functions,
        }
    }

    /// The transform that moves nothing.
    pub fn identity(origin: (f32, f32)) -> Self {
        Transform2D::new(Vec::new(), origin)
    }

    /// The matrix about the origin, in the coordinates of the box's own top-left corner:
    /// move the origin to `(0, 0)`, apply the matrix, move it back.
    pub fn about_origin(&self) -> Matrix2D {
        Matrix2D::translation(self.origin.0, self.origin.1)
            .times(&self.matrix)
            .times(&Matrix2D::translation(-self.origin.0, -self.origin.1))
    }

    /// The transform as a map of document coordinates, for the box whose untransformed
    /// border box is `rect`.
    pub fn in_document(&self, rect: &Rect) -> Matrix2D {
        Matrix2D::translation(rect.x, rect.y)
            .times(&self.about_origin())
            .times(&Matrix2D::translation(-rect.x, -rect.y))
    }

    /// Where the four corners of `rect` (top-left, top-right, bottom-right, bottom-left)
    /// are drawn.
    pub fn corners(&self, rect: &Rect) -> [(f32, f32); 4] {
        let map = self.in_document(rect);
        [
            map.apply(rect.x, rect.y),
            map.apply(rect.right(), rect.y),
            map.apply(rect.right(), rect.bottom()),
            map.apply(rect.x, rect.bottom()),
        ]
    }

    /// The smallest axis-aligned rectangle that holds the drawn `rect`.
    pub fn bounds(&self, rect: &Rect) -> Rect {
        bounds_of(&self.corners(rect))
    }
}

/// The smallest axis-aligned rectangle that holds all of `points`.
pub(crate) fn bounds_of(points: &[(f32, f32); 4]) -> Rect {
    let mut min = points[0];
    let mut max = points[0];
    for &(x, y) in &points[1..] {
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    Rect::new(min.0, min.1, max.0 - min.0, max.1 - min.1)
}

/// A property a renderer can play an animation of by itself, because its value changes
/// nothing about layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimatedProperty {
    /// A box's own `opacity` (not multiplied by its ancestors').
    Opacity,
    Transform,
    BackgroundColor,
}

/// A value of an [`AnimatedProperty`].
#[derive(Clone, Debug, PartialEq)]
pub enum AnimatedValue {
    Opacity(f32),
    Transform(Transform2D),
    Colour(Rgba),
}

/// A value at a point of the animation, `offset` being 0 at its start and 1 at its end.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    pub offset: f32,
    pub value: AnimatedValue,
}

/// Where a `steps()` easing jumps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepPosition {
    JumpStart,
    JumpEnd,
    JumpNone,
    JumpBoth,
}

/// A CSS timing function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Steps { count: u32, position: StepPosition },
}

/// `animation-iteration-count`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Iterations {
    Count(f32),
    Infinite,
}

/// `animation-direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

/// `animation-fill-mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

/// One property's animation, described for a renderer to play by itself: where it goes,
/// how long it takes, and how it repeats. Times are in seconds.
///
/// A CSS `transition` is one of these with two keyframes, one iteration and a backwards
/// fill. An `@keyframes` animation is one per property it changes, with the keyframes
/// that name that property.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub property: AnimatedProperty,
    /// At least two, by ascending offset, the first at 0 and the last at 1.
    pub keyframes: Vec<Keyframe>,
    pub duration: f32,
    pub delay: f32,
    pub easing: Easing,
    pub iterations: Iterations,
    pub direction: PlayDirection,
    pub fill: FillMode,
    /// `animation-play-state: paused`.
    pub paused: bool,
    /// The `@keyframes` name; `None` for a transition.
    pub name: Option<String>,
}

impl Animation {
    /// The value the animation starts from.
    pub fn start(&self) -> AnimatedValue {
        self.keyframes[0].value.clone()
    }

    /// The value the animation ends at.
    pub fn end(&self) -> AnimatedValue {
        self.keyframes[self.keyframes.len() - 1].value.clone()
    }

    /// How long the animation runs from its start, delay included; `None` when it never
    /// ends.
    pub fn total_duration(&self) -> Option<f32> {
        match self.iterations {
            Iterations::Infinite => None,
            Iterations::Count(count) => Some(self.delay + self.duration * count),
        }
    }
}

/// A `transition` that began with this layout. Unlike [`NodeEntry::animations`], which
/// stay for as long as the style that makes them does, a transition is an event: it is in
/// the list for the layout that started it and in none after.
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionStart {
    pub node: NodeId,
    pub animation: Animation,
}

/// A `transform` the display list could not carry, so the box is drawn without it.
#[derive(Clone, Debug, PartialEq)]
pub struct SkippedTransform {
    pub node: NodeId,
    pub tag: String,
    /// The transform as CSS writes it.
    pub transform: String,
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
    /// The box's 2D `transform`, which also carries every box inside it. `None` for no
    /// transform, and for one that only moves the box: a translation is folded into
    /// [`NodeEntry::rect`] (unless an animation or transition of `transform` is declared,
    /// which keeps it here so a renderer can play it).
    pub transform: Option<Transform2D>,
    /// The animations of `@keyframes` this box is running that a renderer plays by itself.
    /// While one is listed, the entry's own value for the property it animates is the value
    /// the animation starts from, and the renderer replaces it as the animation plays.
    pub animations: Vec<Animation>,
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
    /// The transitions this layout started, for a renderer to play.
    pub transitions: Vec<TransitionStart>,
    /// The transforms this layout left out because they are 3D.
    pub skipped_transforms: Vec<SkippedTransform>,
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
    /// The transitions the new layout started. These are events, so they are listed
    /// whether or not an entry changed with them.
    pub transitions: Vec<TransitionStart>,
}

impl DisplayListDiff {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty()
            && self.removed.is_empty()
            && self.order.is_none()
            && self.transitions.is_empty()
    }
}

impl DisplayList {
    pub fn get(&self, node: NodeId) -> Option<&NodeEntry> {
        self.entries.iter().find(|entry| entry.node == node)
    }

    /// The `opacity` of the box itself: [`NodeEntry::opacity`] with its ancestors' taken
    /// out.
    pub fn own_opacity(&self, entry: &NodeEntry) -> f32 {
        let above = entry
            .parent
            .and_then(|parent| self.get(parent))
            .map_or(1.0, |parent| parent.opacity);
        if above > 0.0 {
            entry.opacity / above
        } else {
            entry.opacity
        }
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
            transitions: self.transitions.clone(),
        }
    }

    /// The node drawn on top at a point in document coordinates, or `None` over empty
    /// canvas.
    ///
    /// Over text this is the element that owns the text (a `<span>` inside a paragraph),
    /// otherwise the topmost visible box. A transformed box is hit where it is drawn, not
    /// where layout put it, and so is everything inside it. Scroll positions are taken as
    /// zero; see [`DisplayList::hit_test_scrolled`].
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
        let transformed = self.entries.iter().any(|entry| entry.transform.is_some());
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
            // The point in the coordinates this entry was laid out in, and in the ones its
            // parent's content was.
            let ((px, py), (cx_parent, cy_parent)) = if transformed {
                match unproject(&by_node, entry, (x, y), &scroll_offset) {
                    Some(points) => points,
                    None => continue,
                }
            } else {
                ((x + dx, y + dy), (x, y))
            };
            if let Some(clip) = entry.clip {
                // The innermost clip usually comes from the scroll container itself, whose
                // padding box does not move with its own scroll position: test it in the
                // container's coordinates.
                let clip_space = entry
                    .scroll_parent
                    .and_then(|container| by_node.get(&container))
                    .and_then(|container| container.scroll_parent);
                let (cx, cy) = accumulated(clip_space);
                let inside = if transformed {
                    // The parent's content frame, less the scroll of the containers above
                    // the clip's own.
                    clip.contains(cx_parent - dx + cx, cy_parent - dy + cy)
                } else {
                    clip.contains(x + cx, y + cy)
                };
                if !inside {
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

    /// Where a point of the screen falls in the coordinates `node` was laid out in: the
    /// point with the `transform` of the box and of every box around it taken off. For a
    /// box nothing transforms this is the point itself. Scroll positions are taken as
    /// zero. `None` for a node not in the list, and for a point inside a box that a
    /// transform has flattened (`scale(0)`), which has no layout position.
    pub fn local_point(&self, node: NodeId, x: f32, y: f32) -> Option<(f32, f32)> {
        let by_node: HashMap<NodeId, &NodeEntry> = self
            .entries
            .iter()
            .map(|entry| (entry.node, entry))
            .collect();
        let entry: &NodeEntry = by_node.get(&node)?;
        unproject(&by_node, entry, (x, y), &|_: NodeId| (0.0, 0.0)).map(|(own, _)| own)
    }

    /// Where the point `(x, y)`, given in the coordinates `node` was laid out in, is
    /// drawn: the opposite of [`DisplayList::local_point`].
    pub fn project(&self, node: NodeId, x: f32, y: f32) -> Option<(f32, f32)> {
        let by_node: HashMap<NodeId, &NodeEntry> = self
            .entries
            .iter()
            .map(|entry| (entry.node, entry))
            .collect();
        let mut current = Some(*by_node.get(&node)?);
        let mut point = (x, y);
        let mut seen = HashSet::new();
        while let Some(entry) = current {
            if !seen.insert(entry.node) {
                break;
            }
            if let Some(transform) = &entry.transform {
                point = transform.in_document(&entry.rect).apply(point.0, point.1);
            }
            current = entry
                .parent
                .and_then(|parent| by_node.get(&parent).copied());
        }
        Some(point)
    }
}

/// The boxes from the outermost down to `entry`, following `parent`.
fn ancestry<'a>(
    by_node: &HashMap<NodeId, &'a NodeEntry>,
    entry: &'a NodeEntry,
) -> Vec<&'a NodeEntry> {
    let mut chain = vec![entry];
    let mut seen = HashSet::from([entry.node]);
    let mut parent = entry.parent;
    while let Some(id) = parent {
        if !seen.insert(id) {
            break;
        }
        let Some(&above) = by_node.get(&id) else {
            break;
        };
        chain.push(above);
        parent = above.parent;
    }
    chain.reverse();
    chain
}

/// Takes the transforms and scroll offsets of the boxes around `entry`, and its own
/// transform, off a point of the screen. Returns the point in the coordinates `entry` was
/// laid out in, and the point in the coordinates of its parent's content, which is where
/// the clips it inherits are measured. `None` when a transform on the way cannot be undone.
fn unproject<'a>(
    by_node: &HashMap<NodeId, &'a NodeEntry>,
    entry: &'a NodeEntry,
    point: (f32, f32),
    scroll_offset: &impl Fn(NodeId) -> (f32, f32),
) -> Option<((f32, f32), (f32, f32))> {
    let chain = ancestry(by_node, entry);
    let mut current = point;
    let mut parent_content = point;
    for (index, above) in chain.iter().enumerate() {
        let is_entry = index + 1 == chain.len();
        if is_entry {
            parent_content = current;
        }
        if let Some(transform) = &above.transform {
            let inverse = transform.in_document(&above.rect).inverse()?;
            current = inverse.apply(current.0, current.1);
        }
        if !is_entry && above.scroll.is_some() {
            let (sx, sy) = scroll_offset(above.node);
            current = (current.0 + sx, current.1 + sy);
        }
    }
    Some((current, parent_content))
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
