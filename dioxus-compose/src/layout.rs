//! The layout pass: Taffy over blitz-dom's styled tree, with text sizes from a
//! caller-supplied [`TextMeasurer`].
//!
//! blitz-dom lays itself out by implementing Taffy's tree traits on `BaseDocument`
//! (`layout/mod.rs`), and measures every inline formatting context with Parley inside that
//! implementation (`layout/inline.rs`, `compute_inline_layout`). Both are private to the
//! crate and there is no hook for another measurer. What it does make public is every piece
//! the traits need: each node's Taffy style, layout children, cache and layout fields, the
//! inline-root flag, and the style and construction steps of `resolve` one by one.
//!
//! So this module implements the same traits on a wrapper around the document. Block, flex
//! and grid containers run Taffy's own algorithms over the wrapper, exactly as blitz-dom
//! does. An inline formatting context whose text is all in one font, with no inline boxes
//! and no inline element that pads or borders its text, is sized by the measurer, generated
//! content (`::before` and `::after`) included. Everything else (tables, replaced elements,
//! form controls, inline contexts that mix fonts or embed boxes) is handed back to
//! blitz-dom's own implementation for that subtree, so it keeps working, measured by Parley.
//!
//! Taffy 0.9 has no intrinsic size keywords, and stylo_taffy turns `width`, `min-width` and
//! `max-width` of `min-content`, `max-content` and `fit-content` into `auto`. So before a
//! container is laid out, the children that use them are measured and the result is written
//! into their Taffy styles as a length.
//!
//! A `<select>` is sized here as a control of its own: as wide as its widest option plus
//! room for the dropdown's indicator, one line tall. Its options are not laid out.
//!
//! stylo_taffy reads `position: fixed` as `absolute`, and Taffy places an absolutely
//! positioned box against its parent. A fixed box belongs to the viewport, so once the tree
//! is laid out each fixed box is sized and placed again against the viewport, and its
//! location is kept in viewport coordinates (see [`is_fixed`]).
//!
//! The pieces the pass and the display list both read live under it: the text measurer
//! ([`measure`]), the conversions from stylo's computed values ([`convert`]), backgrounds
//! and replaced images ([`background`]), and the image lookup ([`image`]).

pub(crate) mod background;
pub(crate) mod convert;
pub(crate) mod image;
pub(crate) mod measure;

use std::cell::Ref;
use std::collections::{HashMap, HashSet};

use blitz_dom::{Atom, BaseDocument, LocalName, Node, NodeData};
use style::properties::generated::longhands::position::computed_value::T as Position;
use style::values::computed::length_percentage::CalcLengthPercentage;
use style::values::computed::{
    Length, LengthPercentage, MaxSize as StyloMaxSize, Size as StyloSize,
};
use taffy::{
    AvailableSpace, BoxSizing, CacheTree, Dimension, Display, Layout, LayoutBlockContainer,
    LayoutFlexboxContainer, LayoutGridContainer, LayoutInput, LayoutOutput, LayoutPartialTree,
    Line, MaybeResolve, NodeId, Point, Rect, RequestedAxis, ResolveOrZero, RoundTree, RunMode,
    Size, SizingMode, Style, TraversePartialTree, TraverseTree, compute_block_layout,
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_leaf_layout,
    compute_root_layout, round_layout,
};

use crate::dom::form;
use crate::paint::display_list::{MeasuredText, Rgba, TextAlign};
use measure::{
    TextLineHeight, TextMeasureRequest, TextMeasurer, TextStyle, TextWhiteSpace, WidthConstraint,
};

/// The room a `<select>` keeps beside its widest option for the dropdown's indicator, in
/// CSS pixels: the size of the arrow icon a Compose dropdown shows.
pub(crate) const SELECT_INDICATOR_WIDTH: f32 = 24.0;

/// Whether a box is `position: fixed`. Its layout location is then in viewport
/// coordinates, not its parent's.
pub(crate) fn is_fixed(node: &Node) -> bool {
    node.primary_styles()
        .is_some_and(|style| style.clone_position() == Position::Fixed)
}

/// Resolves a CSS `calc()` that Taffy carries as an opaque pointer to stylo's value.
pub(crate) fn resolve_calc_value(calc_ptr: *const (), basis: f32) -> f32 {
    // Taffy only hands back pointers that stylo_taffy stored when it converted a computed
    // `calc()`, and the computed style that owns the value outlives the layout pass.
    let calc = unsafe { &*(calc_ptr as *const CalcLengthPercentage) };
    calc.resolve(Length::new(basis)).px()
}

/// Runs stylo's cascade and blitz-dom's box construction: everything `BaseDocument::resolve`
/// does except its own layout, which [`run_layout`] replaces.
pub(crate) fn resolve_styles(doc: &mut BaseDocument) {
    doc.resolve_stylist(0.0);
    doc.resolve_layout_children();
    doc.resolve_deferred_tasks();
    let root = doc.root_element().id;
    doc.flush_styles_to_layout(root);
}

/// Lays the document out at its current viewport. Returns the text measured on the way,
/// keyed by the inline formatting context that holds it.
pub(crate) fn run_layout(
    doc: &mut BaseDocument,
    measurer: &mut dyn TextMeasurer,
) -> HashMap<usize, MeasuredText> {
    // No size blitz-dom measured with Parley may answer for a node this pass measures.
    let ids: Vec<usize> = doc.tree().iter().map(|(id, _)| id).collect();
    for id in ids {
        if let Some(node) = doc.get_node_mut(id) {
            node.cache.clear();
        }
    }

    let viewport = doc.viewport();
    let scale = viewport.scale();
    let width = viewport.window_size.0 as f32 / scale;
    let height = viewport.window_size.1 as f32 / scale;
    let root = NodeId::from(doc.root_element().id);
    size_selects_in_lines(doc, measurer);

    let mut texts = HashMap::new();
    let mut tree = MeasuredTree {
        doc,
        measurer,
        texts: &mut texts,
    };
    compute_root_layout(
        &mut tree,
        root,
        Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        },
    );
    tree.place_fixed_boxes(root, Size { width, height });
    round_layout(&mut tree, root);
    texts
}

/// Text in an inline formatting context that the measurer can size on its own.
#[derive(Clone, Debug)]
struct PreparedText {
    owner: usize,
    /// As drawn: white space collapsed and `text-transform` applied.
    text: String,
    style: TextStyle,
    color: Rgba,
    align: TextAlign,
    white_space: TextWhiteSpace,
}

enum Route {
    Hidden,
    /// A `<select>`, sized from its options.
    Select,
    /// Lay this subtree out with blitz-dom's own implementation.
    Blitz,
    /// An inline formatting context sized by the measurer. `None` when it holds no text.
    Measured(Option<PreparedText>),
    Block,
    Flex,
    Grid,
}

struct MeasuredTree<'a> {
    doc: &'a mut BaseDocument,
    measurer: &'a mut dyn TextMeasurer,
    texts: &'a mut HashMap<usize, MeasuredText>,
}

impl MeasuredTree<'_> {
    fn node(&self, id: NodeId) -> &Node {
        self.doc
            .get_node(usize::from(id))
            .expect("Taffy only visits nodes the document lists as layout children")
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.doc
            .get_node_mut(usize::from(id))
            .expect("Taffy only visits nodes the document lists as layout children")
    }

    fn route(&self, id: usize) -> Route {
        let Some(node) = self.doc.get_node(id) else {
            return Route::Hidden;
        };
        match &node.data {
            NodeData::Document => Route::Block,
            NodeData::Element(element) | NodeData::AnonymousBlock(element) => {
                let tag: &str = &element.name.local;
                if tag == "select" && matches!(node.data, NodeData::Element(_)) {
                    return match node.style.display {
                        Display::None => Route::Hidden,
                        _ => Route::Select,
                    };
                }
                // Replaced elements and form controls: blitz-dom sizes them from their
                // attributes and image data, with no text measurement to replace.
                if matches!(tag, "textarea" | "input" | "img" | "canvas" | "svg")
                    || node.flags.is_table_root()
                {
                    return Route::Blitz;
                }
                if node.flags.is_inline_root() {
                    return match prepare_inline(&*self.doc, id) {
                        Some(prepared) => Route::Measured(prepared),
                        None => Route::Blitz,
                    };
                }
                match node.style.display {
                    Display::Block => Route::Block,
                    Display::Flex => Route::Flex,
                    Display::Grid => Route::Grid,
                    Display::None => Route::Hidden,
                }
            }
            _ => Route::Hidden,
        }
    }

    /// A `<select>` as one box: its content is as wide as the widest option's label plus
    /// [`SELECT_INDICATOR_WIDTH`], and as tall as one line of it.
    fn compute_select(&mut self, id: usize, inputs: LayoutInput) -> LayoutOutput {
        let style = self.node(NodeId::from(id)).style.clone();
        let content = select_content_size(&*self.doc, id, &mut *self.measurer);
        compute_leaf_layout(inputs, &style, resolve_calc_value, |_, _| content)
    }

    /// Sizes and places every `position: fixed` box under `root` against the viewport,
    /// outer boxes before the boxes inside them. A fixed box's location is written in
    /// viewport coordinates.
    ///
    /// The insets, sizes, margins, padding and borders resolve against the viewport, as
    /// they do against any containing block, by the rules Taffy applies to absolutely
    /// positioned boxes. On an axis where both insets are `auto` the box stays where it
    /// would have been in flow (its static position), which Taffy has already worked out
    /// against its parent; that place is turned into viewport coordinates here, without the
    /// `transform` of any ancestor.
    fn place_fixed_boxes(&mut self, root: NodeId, viewport: Size<f32>) {
        let mut fixed = Vec::new();
        collect_fixed(&*self.doc, usize::from(root), &mut fixed);
        let mut placed = HashSet::new();
        for id in fixed {
            let static_origin = self.page_origin_of_parent(id, &placed);
            self.place_fixed(NodeId::from(id), viewport, static_origin);
            placed.insert(id);
        }
    }

    /// Where the parent of `id` is, in viewport coordinates: its layout location and every
    /// layout ancestor's added up, up to the first fixed one, whose location already is in
    /// viewport coordinates.
    fn page_origin_of_parent(&self, id: usize, fixed: &HashSet<usize>) -> Point<f32> {
        let mut origin = Point { x: 0.0, y: 0.0 };
        let mut current = self
            .doc
            .get_node(id)
            .and_then(|node| node.layout_parent.get());
        while let Some(ancestor) = current {
            let Some(node) = self.doc.get_node(ancestor) else {
                break;
            };
            origin.x += node.unrounded_layout.location.x;
            origin.y += node.unrounded_layout.location.y;
            if fixed.contains(&ancestor) {
                break;
            }
            current = node.layout_parent.get();
        }
        origin
    }

    fn place_fixed(&mut self, node: NodeId, viewport: Size<f32>, static_origin: Point<f32>) {
        let calc = resolve_calc_value;
        let style = self.node(node).style.clone();
        let current = self.node(node).unrounded_layout;
        let (width, height) = (viewport.width, viewport.height);

        let left = style.inset.left.resolve_to_option(width, calc);
        let right = style.inset.right.resolve_to_option(width, calc);
        let top = style.inset.top.resolve_to_option(height, calc);
        let bottom = style.inset.bottom.resolve_to_option(height, calc);
        // Margins and padding resolve against the containing block's width on both axes.
        let margin = Rect {
            left: style.margin.left.resolve_to_option(width, calc),
            right: style.margin.right.resolve_to_option(width, calc),
            top: style.margin.top.resolve_to_option(width, calc),
            bottom: style.margin.bottom.resolve_to_option(width, calc),
        };
        let padding = style.padding.resolve_or_zero(Some(width), calc);
        let border = style.border.resolve_or_zero(Some(width), calc);
        let insets_x = padding.left + padding.right + border.left + border.right;
        let insets_y = padding.top + padding.bottom + border.top + border.bottom;
        let (adjust_x, adjust_y) = match style.box_sizing {
            BoxSizing::ContentBox => (insets_x, insets_y),
            BoxSizing::BorderBox => (0.0, 0.0),
        };

        let resolve = |dimension: Dimension, basis: f32, adjust: f32| {
            MaybeResolve::<Option<f32>, Option<f32>>::maybe_resolve(dimension, Some(basis), calc)
                .map(|length| length + adjust)
        };
        let min_width = resolve(style.min_size.width, width, adjust_x)
            .unwrap_or(0.0)
            .max(insets_x);
        let min_height = resolve(style.min_size.height, height, adjust_y)
            .unwrap_or(0.0)
            .max(insets_y);
        let max_width = resolve(style.max_size.width, width, adjust_x);
        let max_height = resolve(style.max_size.height, height, adjust_y);
        let clamp = |value: f32, min: f32, max: Option<f32>| {
            max.map_or(value, |max| value.min(max)).max(min)
        };

        let mut known = Size {
            width: resolve(style.size.width, width, adjust_x)
                .map(|value| clamp(value, min_width, max_width)),
            height: resolve(style.size.height, height, adjust_y)
                .map(|value| clamp(value, min_height, max_height)),
        };
        if let (None, Some(left), Some(right)) = (known.width, left, right) {
            let stretched =
                width - margin.left.unwrap_or(0.0) - margin.right.unwrap_or(0.0) - left - right;
            known.width = Some(clamp(stretched.max(0.0), min_width, max_width));
        }
        if let (None, Some(top), Some(bottom)) = (known.height, top, bottom) {
            let stretched =
                height - margin.top.unwrap_or(0.0) - margin.bottom.unwrap_or(0.0) - top - bottom;
            known.height = Some(clamp(stretched.max(0.0), min_height, max_height));
        }
        if let Some(ratio) = style.aspect_ratio {
            match (known.width, known.height) {
                (Some(known_width), None) => known.height = Some(known_width / ratio),
                (None, Some(known_height)) => known.width = Some(known_height * ratio),
                _ => {}
            }
        }

        let parent_size = Size {
            width: Some(width),
            height: Some(height),
        };
        let available_space = Size {
            width: AvailableSpace::Definite(clamp(width, min_width, max_width)),
            height: AvailableSpace::Definite(clamp(height, min_height, max_height)),
        };
        let measured = if known.width.is_none() || known.height.is_none() {
            self.compute_child_layout(
                node,
                LayoutInput {
                    run_mode: RunMode::ComputeSize,
                    sizing_mode: SizingMode::ContentSize,
                    axis: RequestedAxis::Both,
                    known_dimensions: known,
                    parent_size,
                    available_space,
                    vertical_margins_are_collapsible: Line::FALSE,
                },
            )
            .size
        } else {
            Size::ZERO
        };
        let size = Size {
            width: clamp(known.width.unwrap_or(measured.width), min_width, max_width),
            height: clamp(
                known.height.unwrap_or(measured.height),
                min_height,
                max_height,
            ),
        };
        let output = self.compute_child_layout(
            node,
            LayoutInput {
                run_mode: RunMode::PerformLayout,
                sizing_mode: SizingMode::ContentSize,
                axis: RequestedAxis::Both,
                known_dimensions: size.map(Some),
                parent_size,
                available_space,
                vertical_margins_are_collapsible: Line::FALSE,
            },
        );

        // `auto` margins share what is left between two set insets; otherwise they are
        // zero.
        let resolve_margins = |start: Option<f32>,
                               end: Option<f32>,
                               inset_start: Option<f32>,
                               inset_end: Option<f32>,
                               room: f32,
                               extent: f32| {
            match (inset_start, inset_end) {
                (Some(inset_start), Some(inset_end)) => {
                    let free = room
                        - inset_start
                        - inset_end
                        - extent
                        - start.unwrap_or(0.0)
                        - end.unwrap_or(0.0);
                    match (start, end) {
                        (None, None) if free >= 0.0 => (free / 2.0, free / 2.0),
                        (None, None) => (0.0, free),
                        (None, Some(end)) => (free, end),
                        (Some(start), None) => (start, free),
                        (Some(start), Some(end)) => (start, end),
                    }
                }
                _ => (start.unwrap_or(0.0), end.unwrap_or(0.0)),
            }
        };
        let (margin_left, margin_right) =
            resolve_margins(margin.left, margin.right, left, right, width, size.width);
        let (margin_top, margin_bottom) =
            resolve_margins(margin.top, margin.bottom, top, bottom, height, size.height);

        let location = Point {
            x: match (left, right) {
                (Some(left), _) => left + margin_left,
                (None, Some(right)) => width - right - size.width - margin_right,
                // Taffy's location already holds the static position and the margin.
                (None, None) => static_origin.x + current.location.x,
            },
            y: match (top, bottom) {
                (Some(top), _) => top + margin_top,
                (None, Some(bottom)) => height - bottom - size.height - margin_bottom,
                (None, None) => static_origin.y + current.location.y,
            },
        };

        self.node_mut(node).unrounded_layout = Layout {
            location,
            size,
            content_size: output.content_size,
            padding,
            border,
            margin: Rect {
                left: margin_left,
                right: margin_right,
                top: margin_top,
                bottom: margin_bottom,
            },
            ..current
        };
    }

    fn compute_measured(
        &mut self,
        id: usize,
        prepared: Option<PreparedText>,
        inputs: LayoutInput,
    ) -> LayoutOutput {
        let style = self.node(NodeId::from(id)).style.clone();
        let Some(prepared) = prepared else {
            return compute_leaf_layout(inputs, &style, resolve_calc_value, |_, _| Size::ZERO);
        };

        // Padding and borders resolve against the containing block's width, as
        // compute_leaf_layout resolves them.
        let padding = style
            .padding
            .resolve_or_zero(inputs.parent_size.width, resolve_calc_value);
        let border = style
            .border
            .resolve_or_zero(inputs.parent_size.width, resolve_calc_value);
        let horizontal_insets = padding.left + padding.right + border.left + border.right;
        let top_inset = padding.top + border.top;

        let measurer = &mut *self.measurer;
        let mut measured = None;
        let mut output =
            compute_leaf_layout(inputs, &style, resolve_calc_value, |_known, available| {
                // A width the parent has already fixed (a stretched block, a flex item
                // after flexing) is the width the lines must fit in.
                let known_width = inputs
                    .known_dimensions
                    .width
                    .map(|width| (width - horizontal_insets).max(0.0));
                let constraint = match known_width {
                    // `nowrap` and `pre` text is one line however much room there is, and
                    // its min-content width is that line's width too.
                    _ if !prepared.white_space.wrap => WidthConstraint::MaxContent,
                    Some(width) => WidthConstraint::AtMost(width),
                    None => match available.width {
                        AvailableSpace::Definite(width) => WidthConstraint::AtMost(width.max(0.0)),
                        AvailableSpace::MinContent => WidthConstraint::MinContent,
                        AvailableSpace::MaxContent => WidthConstraint::MaxContent,
                    },
                };
                let metrics = measurer.measure(&TextMeasureRequest {
                    text: &prepared.text,
                    style: &prepared.style,
                    width: constraint,
                    white_space: prepared.white_space,
                });
                measured = Some((metrics, constraint));
                Size {
                    width: known_width.unwrap_or(metrics.width),
                    height: metrics.height,
                }
            });

        if let Some((metrics, constraint)) = measured {
            output.first_baselines.y = Some(top_inset + metrics.first_baseline);
            // Only the final pass places text; the sizing passes before it ask about widths
            // the box does not end up with.
            if inputs.run_mode == RunMode::PerformLayout {
                let wrap_width = match constraint {
                    WidthConstraint::AtMost(width) => Some(width),
                    WidthConstraint::MinContent => Some(metrics.width),
                    WidthConstraint::MaxContent => None,
                };
                self.texts.insert(
                    id,
                    MeasuredText {
                        owner: prepared.owner,
                        text: prepared.text,
                        style: prepared.style,
                        color: prepared.color,
                        align: prepared.align,
                        metrics,
                        wrap_width,
                    },
                );
            }
        }
        output
    }

    /// Writes the intrinsic width keywords of `parent`'s children into their Taffy styles.
    ///
    /// stylo_taffy reads `width`, `min-width` and `max-width` of `min-content`,
    /// `max-content` and `fit-content` as `auto`, because Taffy 0.9 has no such sizes. A
    /// VS Code editor tab is `width: 120px; min-width: fit-content`: read as `auto`, a tab
    /// whose label needs more than 120px is cut to 120px, and every tab after it lands too
    /// far left. So each child that uses one is measured here with its own size properties
    /// ignored, and the answer goes into its style as a length before the parent's
    /// algorithm reads it.
    fn resolve_intrinsic_widths(&mut self, parent: NodeId, inputs: LayoutInput) {
        let child_count = self.child_count(parent);
        if child_count == 0 {
            return;
        }

        // The children's containing block: the parent's content box, as far as it is known.
        let parent_style = &self.node(parent).style;
        let padding = parent_style
            .padding
            .resolve_or_zero(inputs.parent_size.width, resolve_calc_value);
        let border = parent_style
            .border
            .resolve_or_zero(inputs.parent_size.width, resolve_calc_value);
        let horizontal = padding.left + padding.right + border.left + border.right;
        let vertical = padding.top + padding.bottom + border.top + border.bottom;
        let inner_width = inputs
            .known_dimensions
            .width
            .or(match inputs.available_space.width {
                AvailableSpace::Definite(width) => Some(width),
                AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
            })
            .map(|width| (width - horizontal).max(0.0));
        let inner_height = inputs
            .known_dimensions
            .height
            .map(|height| (height - vertical).max(0.0));
        let inner = Size {
            width: inner_width,
            height: inner_height,
        };

        for index in 0..child_count {
            let child_id = self.get_child_id(parent, index);
            let Some(keywords) = intrinsic_widths(&*self.doc, usize::from(child_id), inner_width)
            else {
                continue;
            };
            if self.node(child_id).style.display == Display::None {
                continue;
            }

            // Forget what an earlier pass wrote, and any size cached under it, so the
            // measurement is of the contents alone.
            {
                let style = &mut self.node_mut(child_id).style;
                if keywords.width.is_some() {
                    style.size.width = Dimension::auto();
                }
                if keywords.min.is_some() {
                    style.min_size.width = Dimension::auto();
                }
                if keywords.max.is_some() {
                    style.max_size.width = Dimension::auto();
                }
            }
            self.node_mut(child_id).cache.clear();

            let needs_min = [keywords.width, keywords.min, keywords.max]
                .into_iter()
                .flatten()
                .any(|keyword| !matches!(keyword, IntrinsicWidth::MaxContent));
            let min_content = if needs_min {
                self.content_width(child_id, inner, AvailableSpace::MinContent)
            } else {
                0.0
            };
            let max_content = self.content_width(child_id, inner, AvailableSpace::MaxContent);
            // The measurement above asked for the contents' size; the next request for this
            // child is for its own, which the cache must not answer with this one.
            self.node_mut(child_id).cache.clear();

            let style = &self.node(child_id).style;
            let margin = style
                .margin
                .resolve_or_zero(inner_width, resolve_calc_value);
            // The room a `fit-content` box may stretch into: the containing block less the
            // box's own margins. Under a min- or max-content constraint there is no such
            // room, and fit-content is the size that constraint asks for.
            let stretch = inner_width.map(|width| (width - margin.left - margin.right).max(0.0));
            let fit = |limit: Option<f32>| match limit.or(stretch) {
                Some(room) => max_content.min(room.max(min_content)),
                None => match inputs.available_space.width {
                    AvailableSpace::MinContent => min_content,
                    _ => max_content,
                },
            };
            let border_box = |keyword: IntrinsicWidth| match keyword {
                IntrinsicWidth::MinContent => min_content,
                IntrinsicWidth::MaxContent => max_content,
                IntrinsicWidth::FitContent(limit) => fit(limit),
            };
            // Taffy measured the border box. A `box-sizing: content-box` style length is the
            // content box, and Taffy adds the padding and border back to it.
            let content_box_adjustment = match style.box_sizing {
                BoxSizing::BorderBox => 0.0,
                BoxSizing::ContentBox => {
                    let padding = style
                        .padding
                        .resolve_or_zero(inner_width, resolve_calc_value);
                    let border = style
                        .border
                        .resolve_or_zero(inner_width, resolve_calc_value);
                    padding.left + padding.right + border.left + border.right
                }
            };
            let length = |keyword: IntrinsicWidth| {
                Dimension::length((border_box(keyword) - content_box_adjustment).max(0.0))
            };
            let width = keywords.width.map(length);
            let min = keywords.min.map(length);
            let max = keywords.max.map(length);

            let style = &mut self.node_mut(child_id).style;
            if let Some(width) = width {
                style.size.width = width;
            }
            if let Some(min) = min {
                style.min_size.width = min;
            }
            if let Some(max) = max {
                style.max_size.width = max;
            }
        }
    }

    /// The border-box width of `child`'s contents under `space`, its own `width`,
    /// `min-width` and `max-width` ignored.
    fn content_width(
        &mut self,
        child: NodeId,
        parent_size: Size<Option<f32>>,
        space: AvailableSpace,
    ) -> f32 {
        self.compute_child_layout(
            child,
            LayoutInput {
                run_mode: RunMode::ComputeSize,
                sizing_mode: SizingMode::ContentSize,
                axis: RequestedAxis::Horizontal,
                known_dimensions: Size::NONE,
                parent_size,
                available_space: Size {
                    width: space,
                    height: AvailableSpace::MaxContent,
                },
                vertical_margins_are_collapsible: Line::FALSE,
            },
        )
        .size
        .width
    }
}

/// An intrinsic size keyword in `width`, `min-width` or `max-width`.
// The variants are named after the CSS keywords min-content, max-content and fit-content.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug)]
enum IntrinsicWidth {
    MinContent,
    MaxContent,
    /// `fit-content`, or `fit-content(<length-percentage>)` with its limit resolved.
    FitContent(Option<f32>),
}

/// The intrinsic keywords one box uses for its width.
#[derive(Clone, Copy, Debug)]
struct IntrinsicWidths {
    width: Option<IntrinsicWidth>,
    min: Option<IntrinsicWidth>,
    max: Option<IntrinsicWidth>,
}

/// Reads a box's intrinsic width keywords from its computed style. `None` when it uses
/// none. A `fit-content()` percentage resolves against `containing_width`; with no
/// containing width to resolve against it is read as plain `fit-content`.
fn intrinsic_widths(
    doc: &BaseDocument,
    id: usize,
    containing_width: Option<f32>,
) -> Option<IntrinsicWidths> {
    let style = doc.get_node(id)?.primary_styles()?;
    let position = style.get_position();
    let limit = |length: &LengthPercentage| {
        if length.has_percentage() && containing_width.is_none() {
            None
        } else {
            Some(
                length
                    .resolve(Length::new(containing_width.unwrap_or(0.0)))
                    .px(),
            )
        }
    };
    let size = |value: &StyloSize| match value {
        StyloSize::MinContent => Some(IntrinsicWidth::MinContent),
        StyloSize::MaxContent => Some(IntrinsicWidth::MaxContent),
        StyloSize::FitContent => Some(IntrinsicWidth::FitContent(None)),
        StyloSize::FitContentFunction(length) => Some(IntrinsicWidth::FitContent(limit(&length.0))),
        _ => None,
    };
    let max_size = |value: &StyloMaxSize| match value {
        StyloMaxSize::MinContent => Some(IntrinsicWidth::MinContent),
        StyloMaxSize::MaxContent => Some(IntrinsicWidth::MaxContent),
        StyloMaxSize::FitContent => Some(IntrinsicWidth::FitContent(None)),
        StyloMaxSize::FitContentFunction(length) => {
            Some(IntrinsicWidth::FitContent(limit(&length.0)))
        }
        _ => None,
    };
    let widths = IntrinsicWidths {
        width: size(&position.width),
        min: size(&position.min_width),
        max: max_size(&position.max_width),
    };
    (widths.width.is_some() || widths.min.is_some() || widths.max.is_some()).then_some(widths)
}

/// Decides whether the inline formatting context rooted at `root` can be sized by the
/// measurer alone, and gathers what the measurer needs.
///
/// `None` means it cannot, and the context is laid out by blitz-dom with Parley instead:
/// - it embeds inline boxes (an `inline-block`, an `<img>`, an `<input>`), or it is a list
///   item with a marker;
/// - an inline element inside it has padding, a border or a margin on its left or right,
///   which moves the text after it along the line;
/// - its text is not all in one style: the pieces differ in font, colour,
///   `text-transform` or whether they may wrap. The measurer is asked about one run of
///   text in one font and the display list draws it as one run in one colour, so a context
///   that mixes them stays with Parley, which shapes each piece in its own style.
///
/// Inline elements that only wrap text (a `<span>` or an `<a>` around a label, in the same
/// font) do not stop the measurer: the text is measured as one run, and the run's owner is
/// the innermost element holding all of it.
fn prepare_inline(doc: &BaseDocument, root: usize) -> Option<Option<PreparedText>> {
    let node = doc.get_node(root)?;
    let element = node.element_data()?;
    let layout = element.inline_layout_data.as_ref()?;
    if !layout.layout.inline_boxes().is_empty() || element.list_item_data.is_some() {
        return None;
    }

    let mut content = InlineContent::default();
    collect_inline_content(doc, root, &mut content);
    if content.spaced {
        return None;
    }
    let holders = content.holders;
    let Some(&first) = holders.first() else {
        return Some(None);
    };

    let styled = |id: usize| {
        doc.get_node(id)
            .and_then(|node| node.primary_styles())
            .map(|style| {
                (
                    convert::text_style(&style),
                    convert::text_color(&style),
                    convert::text_transform(&style),
                    convert::white_space(&style),
                )
            })
    };
    let (mut style, color, transform, white_space) = styled(first)?;
    for &holder in &holders[1..] {
        let (other_style, other_color, other_transform, other_white_space) = styled(holder)?;
        if other_style != style
            || other_color != color
            || other_transform != transform
            || other_white_space.wrap != white_space.wrap
        {
            return None;
        }
    }

    let root_style = node.primary_styles()?;
    // The root's own line height is the least every line takes (its strut), whatever the
    // text inside sets, as blitz-dom floors each span's line height by it.
    if let (TextLineHeight::Px(text_line), TextLineHeight::Px(root_line)) = (
        style.line_height,
        convert::text_style(&root_style).line_height,
    ) && root_line > text_line
    {
        style.line_height = TextLineHeight::Px(root_line);
    }

    let text = if convert::collapses_white_space(&root_style) {
        // Collapsible white space at the start and end of a line is removed (CSS Text 3,
        // 4.1.2). Only ASCII white space collapses; a no-break space stays.
        layout
            .text
            .trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}'))
            .to_string()
    } else {
        layout.text.clone()
    };
    if text.is_empty() {
        return Some(None);
    }

    Some(Some(PreparedText {
        owner: common_ancestor(doc, root, &holders),
        text: convert::transform_text(&text, transform),
        style,
        color,
        align: convert::text_align(&root_style),
        white_space,
    }))
}

/// The content size of a `<select>`: as wide as its widest option's label plus
/// [`SELECT_INDICATOR_WIDTH`], as tall as one line of it.
fn select_content_size(
    doc: &BaseDocument,
    id: usize,
    measurer: &mut dyn TextMeasurer,
) -> Size<f32> {
    let Some(text_style) = doc
        .get_node(id)
        .and_then(|node| node.primary_styles())
        .map(|computed| convert::text_style(&computed))
    else {
        return Size::ZERO;
    };
    let mut labels: Vec<String> = form::select_options(doc, id)
        .into_iter()
        .map(|option| option.label)
        .collect();
    if labels.is_empty() {
        // An empty select is still a line tall.
        labels.push(" ".to_string());
    }
    let mut size = Size::ZERO;
    for label in &labels {
        let metrics = measurer.measure(&TextMeasureRequest {
            text: label,
            style: &text_style,
            width: WidthConstraint::MaxContent,
            white_space: TextWhiteSpace::NOWRAP,
        });
        size.width = f32::max(size.width, metrics.width);
        size.height = f32::max(size.height, metrics.height);
    }
    size.width += SELECT_INDICATOR_WIDTH;
    size
}

/// Gives every `<select>` that sits in a line of text its size as a length in its Taffy
/// style, where its CSS leaves the size `auto`.
///
/// A line of text that embeds a box is laid out by blitz-dom, which sizes the boxes on the
/// line itself and knows nothing of a `<select>`: with its options not drawn, it would find
/// the select empty and make it zero wide. A select in a block, flex or grid container is
/// sized by [`MeasuredTree::compute_select`] instead and is left alone.
fn size_selects_in_lines(doc: &mut BaseDocument, measurer: &mut dyn TextMeasurer) {
    let selects: Vec<usize> = doc
        .tree()
        .iter()
        .filter(|(_, node)| form::tag(node) == Some("select"))
        .map(|(id, _)| id)
        .collect();
    for id in selects {
        let in_line = doc
            .get_node(id)
            .and_then(|node| node.layout_parent.get())
            .and_then(|parent| doc.get_node(parent))
            .is_some_and(|parent| parent.flags.is_inline_root());
        if !in_line {
            continue;
        }
        let Some((auto_width, auto_height)) = doc
            .get_node(id)
            .and_then(|node| node.primary_styles())
            .map(|style| {
                let position = style.get_position();
                (
                    matches!(position.width, StyloSize::Auto),
                    matches!(position.height, StyloSize::Auto),
                )
            })
        else {
            continue;
        };
        let content = select_content_size(doc, id, measurer);
        let Some(node) = doc.get_node_mut(id) else {
            continue;
        };
        let style = &mut node.style;
        // A `border-box` length includes the padding and border; they resolve against a
        // containing block this pass does not know yet, so only lengths count.
        let padding = style.padding.resolve_or_zero(None, resolve_calc_value);
        let border = style.border.resolve_or_zero(None, resolve_calc_value);
        let (extra_x, extra_y) = match style.box_sizing {
            BoxSizing::BorderBox => (
                padding.left + padding.right + border.left + border.right,
                padding.top + padding.bottom + border.top + border.bottom,
            ),
            BoxSizing::ContentBox => (0.0, 0.0),
        };
        if auto_width {
            style.size.width = Dimension::length(content.width + extra_x);
        }
        if auto_height {
            style.size.height = Dimension::length(content.height + extra_y);
        }
        node.cache.clear();
    }
}

/// The `position: fixed` boxes below `id` in the layout tree, in tree order, so that a fixed
/// box comes before the fixed boxes inside it.
fn collect_fixed(doc: &BaseDocument, id: usize, out: &mut Vec<usize>) {
    let Some(node) = doc.get_node(id) else {
        return;
    };
    let children = node.layout_children.borrow().clone().unwrap_or_default();
    for child in children {
        let Some(child_node) = doc.get_node(child) else {
            continue;
        };
        if child_node.style.display == Display::None {
            continue;
        }
        if is_fixed(child_node) {
            out.push(child);
        }
        collect_fixed(doc, child, out);
    }
}

/// What an inline formatting context holds, as far as the measurer is concerned.
#[derive(Default)]
struct InlineContent {
    /// The nodes whose children include a text node that is not only white space.
    holders: Vec<usize>,
    /// Whether an inline element inside takes room beside its text.
    spaced: bool,
}

/// Walks the inline formatting context rooted at `id`: its text, the inline elements around
/// it, and the boxes `::before` and `::after` generate, whose `content` text is text of the
/// context like any other (a glyph in an icon font included).
fn collect_inline_content(doc: &BaseDocument, id: usize, content: &mut InlineContent) {
    let Some(node) = doc.get_node(id) else {
        return;
    };
    let children = node
        .before
        .into_iter()
        .chain(node.children.iter().copied())
        .chain(node.after);
    for child_id in children {
        let Some(child) = doc.get_node(child_id) else {
            continue;
        };
        match &child.data {
            NodeData::Text(text) => {
                // The text node's own parent, not `id`: when the context is an anonymous
                // block blitz-dom made around loose text, the parent is the real element,
                // whose id survives the next layout. The text blitz-dom creates for a
                // pseudo-element's `content` has no parent, and belongs to the pseudo-element.
                let holder = child.parent.unwrap_or(id);
                if !text.content.trim().is_empty() && !content.holders.contains(&holder) {
                    content.holders.push(holder);
                }
            }
            // An element, or the anonymous box blitz-dom makes for a `::before` or `::after`
            // (only pseudo-elements are reachable through `before`, `children` and `after`;
            // the anonymous blocks around loose text are layout children only).
            NodeData::Element(_) | NodeData::AnonymousBlock(_) => {
                let Some(style) = child.primary_styles() else {
                    continue;
                };
                let display = style.clone_display();
                if display.is_none() {
                    continue;
                }
                if !display.is_contents() && convert::has_inline_spacing(&style) {
                    content.spaced = true;
                }
                drop(style);
                collect_inline_content(doc, child_id, content);
            }
            _ => {}
        }
    }
}

/// The innermost node that contains every holder. The walk stops at `root`, or goes on to
/// the document when the holders sit outside it (loose text in an anonymous block).
fn common_ancestor(doc: &BaseDocument, root: usize, holders: &[usize]) -> usize {
    let chain = |id: usize| {
        let mut chain = vec![id];
        let mut current = id;
        while current != root {
            match doc.get_node(current).and_then(|node| node.parent) {
                Some(parent) => {
                    chain.push(parent);
                    current = parent;
                }
                None => break,
            }
        }
        chain.reverse();
        chain
    };
    let mut common = chain(holders[0]);
    for &holder in &holders[1..] {
        let other = chain(holder);
        let shared = common
            .iter()
            .zip(other.iter())
            .take_while(|(a, b)| a == b)
            .count();
        common.truncate(shared);
    }
    common.last().copied().unwrap_or(root)
}

pub(crate) struct LayoutChildren<'a> {
    items: Ref<'a, [usize]>,
    index: usize,
}

impl Iterator for LayoutChildren<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let id = self.items.get(self.index).copied()?;
        self.index += 1;
        Some(NodeId::from(id))
    }
}

impl TraversePartialTree for MeasuredTree<'_> {
    type ChildIter<'a>
        = LayoutChildren<'a>
    where
        Self: 'a;

    fn child_ids(&self, node_id: NodeId) -> Self::ChildIter<'_> {
        let children = self.node(node_id).layout_children.borrow();
        LayoutChildren {
            items: Ref::map(children, |children| children.as_deref().unwrap_or(&[])),
            index: 0,
        }
    }

    fn child_count(&self, node_id: NodeId) -> usize {
        self.node(node_id)
            .layout_children
            .borrow()
            .as_ref()
            .map_or(0, |children| children.len())
    }

    fn get_child_id(&self, node_id: NodeId, index: usize) -> NodeId {
        let id = self
            .node(node_id)
            .layout_children
            .borrow()
            .as_ref()
            .expect("Taffy asks for a child by index only after counting the node's children")
            [index];
        NodeId::from(id)
    }
}

impl TraverseTree for MeasuredTree<'_> {}

impl LayoutPartialTree for MeasuredTree<'_> {
    type CoreContainerStyle<'a>
        = &'a Style<Atom>
    where
        Self: 'a;

    type CustomIdent = Atom;

    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.node(node_id).style
    }

    fn resolve_calc_value(&self, calc_ptr: *const (), basis: f32) -> f32 {
        resolve_calc_value(calc_ptr, basis)
    }

    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.node_mut(node_id).unrounded_layout = *layout;
    }

    fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let id = usize::from(node_id);
            match tree.route(id) {
                Route::Hidden => LayoutOutput::HIDDEN,
                Route::Select => tree.compute_select(id, inputs),
                Route::Blitz => <BaseDocument as LayoutPartialTree>::compute_child_layout(
                    &mut *tree.doc,
                    node_id,
                    inputs,
                ),
                Route::Measured(prepared) => tree.compute_measured(id, prepared, inputs),
                Route::Block => {
                    tree.resolve_intrinsic_widths(node_id, inputs);
                    compute_block_layout(tree, node_id, inputs)
                }
                Route::Flex => {
                    tree.resolve_intrinsic_widths(node_id, inputs);
                    compute_flexbox_layout(tree, node_id, inputs)
                }
                Route::Grid => {
                    tree.resolve_intrinsic_widths(node_id, inputs);
                    compute_grid_layout(tree, node_id, inputs)
                }
            }
        })
    }
}

impl CacheTree for MeasuredTree<'_> {
    fn cache_get(
        &self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Option<LayoutOutput> {
        self.node(node_id)
            .cache
            .get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
        layout_output: LayoutOutput,
    ) {
        self.node_mut(node_id).cache.store(
            known_dimensions,
            available_space,
            run_mode,
            layout_output,
        );
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.node_mut(node_id).cache.clear();
    }
}

impl LayoutBlockContainer for MeasuredTree<'_> {
    type BlockContainerStyle<'a>
        = &'a Style<Atom>
    where
        Self: 'a;

    type BlockItemStyle<'a>
        = &'a Style<Atom>
    where
        Self: 'a;

    fn get_block_container_style(&self, node_id: NodeId) -> Self::BlockContainerStyle<'_> {
        &self.node(node_id).style
    }

    fn get_block_child_style(&self, child_node_id: NodeId) -> Self::BlockItemStyle<'_> {
        &self.node(child_node_id).style
    }
}

impl LayoutFlexboxContainer for MeasuredTree<'_> {
    type FlexboxContainerStyle<'a>
        = &'a Style<Atom>
    where
        Self: 'a;

    type FlexboxItemStyle<'a>
        = &'a Style<Atom>
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.node(node_id).style
    }

    fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.node(child_node_id).style
    }
}

impl LayoutGridContainer for MeasuredTree<'_> {
    type GridContainerStyle<'a>
        = &'a Style<Atom>
    where
        Self: 'a;

    type GridItemStyle<'a>
        = &'a Style<Atom>
    where
        Self: 'a;

    fn get_grid_container_style(&self, node_id: NodeId) -> Self::GridContainerStyle<'_> {
        &self.node(node_id).style
    }

    fn get_grid_child_style(&self, child_node_id: NodeId) -> Self::GridItemStyle<'_> {
        &self.node(child_node_id).style
    }
}

impl RoundTree for MeasuredTree<'_> {
    fn get_unrounded_layout(&self, node_id: NodeId) -> Layout {
        self.node(node_id).unrounded_layout
    }

    fn set_final_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.node_mut(node_id).final_layout = *layout;
    }
}

/// A tag name for comparisons that do not go through markup5ever's static atom macros.
pub(crate) fn local(name: &str) -> LocalName {
    LocalName::from(name)
}
