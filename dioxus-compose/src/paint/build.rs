//! Turns a laid-out blitz-dom document into a [`DisplayList`].
//!
//! Two walks. The first goes down the layout tree and settles, for every box, its absolute
//! rectangle and what its ancestors impose on it: the clip, the accumulated opacity, the
//! scroll container it moves with. The second puts the boxes in CSS paint order (CSS 2.1
//! appendix E, simplified as noted on [`Painter::paint_context`]) and reads each box's
//! decorations from its computed style.

use std::cell::RefCell;
use std::collections::HashMap;

use blitz_dom::node::SpecialElementData;
use blitz_dom::{BaseDocument, Node, NodeData};
use parley::PositionedLayoutItem;
use style::properties::ComputedValues;
use style::properties::generated::longhands::position::computed_value::T as Position;
use style::values::computed::{BorderStyle, Length, Overflow};

use crate::html::NodeId;
use crate::layout::background::{self, Boxes};
use crate::layout::convert;
use crate::paint::display_list::{
    Border, BorderLine, BoxShadow, Corners, DisplayList, InputField, InputKind, MeasuredText,
    NodeEntry, Radius, Rect, ScrollContainer, Sides, TextAlign, TextRun,
};
use crate::dom::form;
use crate::layout::image::ImageLookup;
use crate::layout::{is_fixed, local};

/// What the first walk settles for one box.
struct Placed {
    rect: Rect,
    /// Clip imposed by ancestors.
    clip: Option<Rect>,
    opacity: f32,
    scroll_parent: Option<NodeId>,
    /// The layout parent, when it is a box too.
    parent: Option<NodeId>,
    /// Layout children that are boxes, in tree order.
    children: Vec<NodeId>,
    /// Whether this box starts a stacking context, and at which `z-index`.
    context: Option<i32>,
    /// `position` other than `static`.
    positioned: bool,
    /// `position: fixed`: placed against the viewport, and moved by no scroll container.
    fixed: bool,
    /// `overflow` other than `visible`: nothing inside reaches past the padding box.
    clips: bool,
}

/// Inherited down the first walk.
#[derive(Clone, Copy)]
struct Ambient {
    x: f32,
    y: f32,
    clip: Option<Rect>,
    opacity: f32,
    scroll_parent: Option<NodeId>,
}

pub(crate) fn build_display_list(
    doc: &BaseDocument,
    texts: &HashMap<usize, MeasuredText>,
    images: &mut ImageLookup<'_>,
) -> DisplayList {
    let mut painter = Painter {
        doc,
        texts,
        images: RefCell::new(images),
        placed: HashMap::new(),
        order: Vec::new(),
    };
    let root = doc.root_element().id;
    painter.place(
        root,
        None,
        Ambient {
            x: 0.0,
            y: 0.0,
            clip: None,
            opacity: 1.0,
            scroll_parent: None,
        },
    );
    if painter.placed.contains_key(&root) {
        painter.paint_context(root);
    }
    let mut entries = Vec::new();
    let mut index_of = HashMap::new();
    for &id in &painter.order {
        if let Some(entry) = painter.entry(id) {
            index_of.insert(id, entries.len());
            entries.push(entry);
        }
    }
    let scrollers: Vec<(NodeId, usize)> = index_of
        .iter()
        .filter(|&(_, &index)| entries[index].scroll.is_some())
        .map(|(&id, &index)| (id, index))
        .collect();
    for (id, index) in scrollers {
        let (width, height) = painter.scroll_content_size(id, &entries, &index_of);
        if let Some(scroll) = entries[index].scroll.as_mut() {
            scroll.content_width = width;
            scroll.content_height = height;
        }
    }
    DisplayList { entries }
}

struct Painter<'a, 'i> {
    doc: &'a BaseDocument,
    texts: &'a HashMap<usize, MeasuredText>,
    /// Natural sizes for background images, and the base `<img>` sources resolve against.
    images: RefCell<&'a mut ImageLookup<'i>>,
    placed: HashMap<NodeId, Placed>,
    order: Vec<NodeId>,
}

/// Whether a node is a box the display list describes: an element or an anonymous block
/// that takes part in layout.
fn is_box(node: &Node) -> bool {
    matches!(
        node.data,
        NodeData::Element(_) | NodeData::AnonymousBlock(_)
    ) && node.style.display != taffy::Display::None
}

fn layout_children(node: &Node) -> Vec<NodeId> {
    node.layout_children.borrow().clone().unwrap_or_default()
}

fn clips(style: &ComputedValues) -> bool {
    style.clone_overflow_x() != Overflow::Visible || style.clone_overflow_y() != Overflow::Visible
}

fn scrolls(overflow: Overflow) -> bool {
    matches!(overflow, Overflow::Scroll | Overflow::Auto)
}

fn padding_box(rect: &Rect, layout: &taffy::Layout) -> Rect {
    rect.inset(&Sides {
        top: layout.border.top,
        right: layout.border.right,
        bottom: layout.border.bottom,
        left: layout.border.left,
    })
}

fn content_box(rect: &Rect, layout: &taffy::Layout) -> Rect {
    rect.inset(&Sides {
        top: layout.border.top + layout.padding.top,
        right: layout.border.right + layout.padding.right,
        bottom: layout.border.bottom + layout.padding.bottom,
        left: layout.border.left + layout.padding.left,
    })
}

impl Painter<'_, '_> {
    fn place(&mut self, id: NodeId, parent: Option<NodeId>, ambient: Ambient) {
        // A copy of the shared reference, so nodes borrowed from it do not hold `self`.
        let doc = self.doc;
        let Some(node) = doc.get_node(id) else {
            return;
        };
        if !is_box(node) {
            return;
        }
        let layout = &node.unrounded_layout;
        let style = node.primary_styles();
        let (shift_x, shift_y) = style.as_ref().map_or((0.0, 0.0), |style| {
            translation(style, layout.size.width, layout.size.height)
        });
        let fixed = is_fixed(node);
        // A fixed box's location is in viewport coordinates already, and no ancestor's
        // clip or scroll position applies to it.
        let ambient = if fixed {
            Ambient {
                x: 0.0,
                y: 0.0,
                clip: None,
                scroll_parent: None,
                ..ambient
            }
        } else {
            ambient
        };
        let rect = Rect::new(
            ambient.x + layout.location.x + shift_x,
            ambient.y + layout.location.y + shift_y,
            layout.size.width,
            layout.size.height,
        );

        let own_opacity = style.as_ref().map_or(1.0, |style| style.clone_opacity());
        let positioned = style
            .as_ref()
            .is_some_and(|style| style.clone_position() != Position::Static);
        let z_index = style.as_ref().and_then(|style| {
            let z = style.clone_z_index();
            (!z.is_auto()).then(|| z.integer_or(0))
        });
        // A positioned box with an integer z-index, or any box with opacity below one,
        // starts a stacking context. (Transforms, filters and the rest also do in CSS; this
        // display list does not carry them.)
        let context = match (positioned, z_index) {
            (true, Some(z)) => Some(z),
            _ if own_opacity < 1.0 => Some(0),
            _ => None,
        };
        let opacity = ambient.opacity * own_opacity;

        let mut inner = Ambient {
            x: rect.x,
            y: rect.y,
            clip: ambient.clip,
            opacity,
            scroll_parent: ambient.scroll_parent,
        };
        let own_clip = style.as_ref().is_some_and(|style| clips(style));
        if let Some(style) = style.as_ref() {
            if clips(style) {
                let padding = padding_box(&rect, layout);
                inner.clip = Some(match ambient.clip {
                    Some(outer) => outer.intersect(&padding),
                    None => padding,
                });
            }
            if scrolls(style.clone_overflow_x()) || scrolls(style.clone_overflow_y()) {
                inner.scroll_parent = Some(id);
            }
        }
        drop(style);

        // A `<select>`'s options are listed by the renderer's dropdown, not drawn in the
        // page.
        let children: Vec<NodeId> = if form::tag(node) == Some("select") {
            Vec::new()
        } else {
            layout_children(node)
                .into_iter()
                .filter(|&child| doc.get_node(child).is_some_and(is_box))
                .collect()
        };
        for &child in &children {
            self.place(child, Some(id), inner);
        }
        self.placed.insert(
            id,
            Placed {
                rect,
                clip: ambient.clip,
                opacity,
                scroll_parent: ambient.scroll_parent,
                parent,
                children,
                context,
                positioned,
                fixed,
                clips: own_clip,
            },
        );
    }

    /// The size of what a scroll container scrolls, measured from its padding box's
    /// origin: the furthest any box inside it reaches (and the text of every box that does
    /// not clip), plus its own padding on the far side. An in-flow child counts with its
    /// margin. See [`ScrollContainer`].
    ///
    /// Taffy's own content size is not used. blitz-dom measures an inline formatting
    /// context with its border box where Taffy expects the content box, so every table cell
    /// and every block of mixed text it lays out reports content its padding beyond its
    /// own edge, and the container would scroll past where anything is drawn.
    fn scroll_content_size(
        &self,
        id: NodeId,
        entries: &[NodeEntry],
        index_of: &HashMap<NodeId, usize>,
    ) -> (f32, f32) {
        let (Some(node), Some(placed)) = (self.doc.get_node(id), self.placed.get(&id)) else {
            return (0.0, 0.0);
        };
        let layout = &node.unrounded_layout;
        let viewport = padding_box(&placed.rect, layout);
        let content = content_box(&placed.rect, layout);
        let mut reach = (content.x, content.y);
        if let Some(&index) = index_of.get(&id) {
            for run in &entries[index].texts {
                reach.0 = reach.0.max(run.rect.right());
                reach.1 = reach.1.max(run.rect.bottom());
            }
        }
        self.extend_reach(id, true, entries, index_of, &mut reach);
        (
            (reach.0 - viewport.x + layout.padding.right).max(0.0),
            (reach.1 - viewport.y + layout.padding.bottom).max(0.0),
        )
    }

    fn extend_reach(
        &self,
        id: NodeId,
        direct: bool,
        entries: &[NodeEntry],
        index_of: &HashMap<NodeId, usize>,
        reach: &mut (f32, f32),
    ) {
        let Some(placed) = self.placed.get(&id) else {
            return;
        };
        for &child in &placed.children {
            let Some(child_placed) = self.placed.get(&child) else {
                continue;
            };
            if child_placed.fixed {
                continue;
            }
            let rect = child_placed.rect;
            if rect.width > 0.0 && rect.height > 0.0 {
                let (mut right, mut bottom) = (rect.right(), rect.bottom());
                if direct && !child_placed.positioned {
                    if let Some(node) = self.doc.get_node(child) {
                        right += node.unrounded_layout.margin.right.max(0.0);
                        bottom += node.unrounded_layout.margin.bottom.max(0.0);
                    }
                }
                reach.0 = reach.0.max(right);
                reach.1 = reach.1.max(bottom);
            }
            if child_placed.clips {
                continue;
            }
            if let Some(&index) = index_of.get(&child) {
                for run in &entries[index].texts {
                    reach.0 = reach.0.max(run.rect.right());
                    reach.1 = reach.1.max(run.rect.bottom());
                }
            }
            self.extend_reach(child, false, entries, index_of, reach);
        }
    }

    /// Paints a stacking context: the box itself, then descendants in negative z-index
    /// contexts, then in-flow descendants in tree order, then positioned descendants and
    /// z-index 0 contexts in tree order, then positive z-index contexts.
    ///
    /// Simplified from CSS 2.1 appendix E in two ways: in-flow descendants are painted
    /// whole, box then content, in tree order (CSS paints every block background before any
    /// inline content, which differs only where in-flow boxes overlap), and a positioned box
    /// with `z-index: auto` is painted as one unit with its descendants.
    fn paint_context(&mut self, id: NodeId) {
        self.order.push(id);
        let mut layers = Layers::default();
        self.collect(id, &mut layers);

        layers.negative.sort_by_key(|&(z, _)| z);
        for (_, child) in layers.negative {
            self.paint_context(child);
        }
        self.order.extend(layers.normal);
        for child in layers.positioned {
            self.paint_context(child);
        }
        layers.positive.sort_by_key(|&(z, _)| z);
        for (_, child) in layers.positive {
            self.paint_context(child);
        }
    }

    fn collect(&self, id: NodeId, layers: &mut Layers) {
        let Some(placed) = self.placed.get(&id) else {
            return;
        };
        for &child in &placed.children {
            let Some(child_placed) = self.placed.get(&child) else {
                continue;
            };
            match child_placed.context {
                Some(z) if z < 0 => layers.negative.push((z, child)),
                Some(z) if z > 0 => layers.positive.push((z, child)),
                Some(_) => layers.positioned.push(child),
                None if child_placed.positioned => layers.positioned.push(child),
                None => {
                    layers.normal.push(child);
                    self.collect(child, layers);
                }
            }
        }
    }

    fn entry(&self, id: NodeId) -> Option<NodeEntry> {
        let node = self.doc.get_node(id)?;
        let placed = self.placed.get(&id)?;
        let element = node.element_data()?;
        let layout = &node.unrounded_layout;
        let rect = placed.rect;
        let tag = match node.data {
            NodeData::AnonymousBlock(_) => String::new(),
            _ => element.name.local.to_string(),
        };

        let mut entry = NodeEntry {
            node: display_key(self.doc, node),
            tag,
            rect,
            visible: true,
            background: None,
            backgrounds: Vec::new(),
            border: None,
            radii: None,
            shadows: Vec::new(),
            clip: placed.clip,
            clips_children: false,
            opacity: placed.opacity,
            scroll: None,
            scroll_parent: placed.scroll_parent,
            // A fixed box is placed against the viewport, so it goes in the root box, not
            // inside a parent that may scroll or clip.
            parent: if placed.fixed && id != self.doc.root_element().id {
                Some(self.doc.root_element().id)
            } else {
                placed
                    .parent
                    .and_then(|parent| self.doc.get_node(parent))
                    .map(|parent| display_key(self.doc, parent))
            },
            texts: Vec::new(),
            input: None,
            image: None,
        };

        let style = node.primary_styles();
        if let Some(style) = style.as_ref() {
            entry.visible = matches!(
                style.clone_visibility(),
                style::computed_values::visibility::T::Visible
            );
            entry.clips_children = clips(style);
            let overflow_x = style.clone_overflow_x();
            let overflow_y = style.clone_overflow_y();
            if scrolls(overflow_x) || scrolls(overflow_y) {
                let viewport = padding_box(&rect, layout);
                entry.scroll = Some(ScrollContainer {
                    viewport,
                    // Measured from the boxes inside once every entry is built:
                    // `Painter::scroll_content_size`.
                    content_width: 0.0,
                    content_height: 0.0,
                    horizontal: scrolls(overflow_x),
                    vertical: scrolls(overflow_y),
                });
            }
            // An anonymous block has no style of its own worth drawing: it inherits from
            // its parent, which draws its own background.
            if entry.visible && !matches!(node.data, NodeData::AnonymousBlock(_)) {
                self.decorate(&mut entry, style, layout);
                let images = self.images.borrow();
                entry.image =
                    background::replaced_image(node, style, content_box(&rect, layout), &images);
            }
        }
        drop(style);

        if entry.visible {
            // A `<select>` shows its chosen option in the renderer's dropdown.
            if form::tag(node) != Some("select") {
                entry.texts = self.text_runs(id, node, &rect, layout);
            }
            entry.input = input_field(self.doc, node, &rect, layout);
        }
        Some(entry)
    }

    fn decorate(&self, entry: &mut NodeEntry, style: &ComputedValues, layout: &taffy::Layout) {
        let background = convert::resolve_color(&style.clone_background_color(), style);
        if !background.is_transparent() {
            entry.background = Some(background);
        }
        let boxes = Boxes {
            border: entry.rect,
            padding: padding_box(&entry.rect, layout),
            content: content_box(&entry.rect, layout),
        };
        let mut images = self.images.borrow_mut();
        entry.backgrounds = background::background_layers(style, &boxes, &mut images);
        drop(images);

        let widths = Sides {
            top: layout.border.top,
            right: layout.border.right,
            bottom: layout.border.bottom,
            left: layout.border.left,
        };
        if widths.top > 0.0 || widths.right > 0.0 || widths.bottom > 0.0 || widths.left > 0.0 {
            entry.border = Some(Border {
                widths,
                colors: Sides {
                    top: convert::resolve_color(&style.clone_border_top_color(), style),
                    right: convert::resolve_color(&style.clone_border_right_color(), style),
                    bottom: convert::resolve_color(&style.clone_border_bottom_color(), style),
                    left: convert::resolve_color(&style.clone_border_left_color(), style),
                },
                lines: Sides {
                    top: border_line(style.clone_border_top_style()),
                    right: border_line(style.clone_border_right_style()),
                    bottom: border_line(style.clone_border_bottom_style()),
                    left: border_line(style.clone_border_left_style()),
                },
            });
        }

        entry.radii = corner_radii(style, entry.rect.width, entry.rect.height);

        entry.shadows = style
            .clone_box_shadow()
            .0
            .iter()
            .map(|shadow| BoxShadow {
                color: convert::resolve_color(&shadow.base.color, style),
                offset_x: shadow.base.horizontal.px(),
                offset_y: shadow.base.vertical.px(),
                blur: shadow.base.blur.0.px(),
                spread: shadow.spread.px(),
                inset: shadow.inset,
            })
            .filter(|shadow| !shadow.color.is_transparent())
            .collect();
    }

    fn text_runs(
        &self,
        id: NodeId,
        node: &Node,
        rect: &Rect,
        layout: &taffy::Layout,
    ) -> Vec<TextRun> {
        let content = content_box(rect, layout);
        if let Some(measured) = self.texts.get(&id) {
            let width = measured.metrics.width;
            let free = (content.width - width).max(0.0);
            let offset = match measured.align {
                TextAlign::Center => free / 2.0,
                TextAlign::Right | TextAlign::End => free,
                TextAlign::Start | TextAlign::Left | TextAlign::Justify => 0.0,
            };
            return vec![TextRun {
                owner: measured.owner,
                rect: Rect::new(
                    content.x + offset,
                    content.y,
                    width,
                    measured.metrics.height,
                ),
                text: measured.text.clone(),
                style: measured.style.clone(),
                color: measured.color,
                align: measured.align,
                wrap_width: measured.wrap_width,
                line_count: measured.metrics.line_count,
                baseline: measured.metrics.first_baseline,
            }];
        }
        if node.flags.is_inline_root() {
            return self.parley_runs(node, &content);
        }
        Vec::new()
    }

    /// The glyph runs of an inline formatting context blitz-dom laid out with Parley, one
    /// text run per stretch of text in one style on one line.
    fn parley_runs(&self, node: &Node, content: &Rect) -> Vec<TextRun> {
        let Some(text_layout) = node
            .element_data()
            .and_then(|element| element.inline_layout_data.as_ref())
        else {
            return Vec::new();
        };
        let layout = &text_layout.layout;
        let scale = layout.scale();
        let mut runs = Vec::new();
        for line in layout.lines() {
            let mut current_run: Option<std::ops::Range<usize>> = None;
            let mut run_start_x = 0.0f32;
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let run_range = run.text_range();
                if current_run.as_ref() != Some(&run_range) {
                    current_run = Some(run_range);
                    run_start_x = glyph_run.offset();
                }
                // A shaped run can carry several styles (a coloured span in a sentence).
                // The clusters whose start falls inside this glyph run's extent are its text.
                let start = glyph_run.offset();
                let end = start + glyph_run.advance();
                let mut x = run_start_x;
                let mut range: Option<(usize, usize)> = None;
                for cluster in run.clusters() {
                    let cluster_x = x;
                    x += cluster.advance();
                    if cluster_x + 0.01 >= start && cluster_x < end - 0.01 {
                        let text_range = cluster.text_range();
                        range = Some(match range {
                            Some((from, to)) => {
                                (from.min(text_range.start), to.max(text_range.end))
                            }
                            None => (text_range.start, text_range.end),
                        });
                    }
                }
                let Some((from, to)) = range else {
                    continue;
                };
                let Some(text) = text_layout.text.get(from..to) else {
                    continue;
                };
                if text.trim().is_empty() {
                    continue;
                }
                let owner = stable_owner(self.doc, glyph_run.style().brush.id);
                let Some(owner_style) = self.doc.get_node(owner).and_then(|n| n.primary_styles())
                else {
                    continue;
                };
                let metrics = run.metrics();
                runs.push(TextRun {
                    owner,
                    rect: Rect::new(
                        content.x + start / scale,
                        content.y + (glyph_run.baseline() - metrics.ascent) / scale,
                        glyph_run.advance() / scale,
                        (metrics.ascent + metrics.descent) / scale,
                    ),
                    text: text.to_string(),
                    style: convert::text_style(&owner_style),
                    color: convert::text_color(&owner_style),
                    align: TextAlign::Start,
                    wrap_width: None,
                    line_count: 1,
                    baseline: metrics.ascent / scale,
                });
            }
        }
        runs
    }
}

/// The id an entry is listed under.
///
/// blitz-dom rebuilds the anonymous blocks it wraps around loose text on every layout, under
/// new ids, so an anonymous block is listed under the first node it wraps that has no box
/// of its own (its first text node or inline element). Those are DOM nodes and keep their
/// ids, so an unchanged anonymous block diffs as unchanged.
fn display_key(doc: &BaseDocument, node: &Node) -> NodeId {
    if !matches!(node.data, NodeData::AnonymousBlock(_)) {
        return node.id;
    }
    let boxes = layout_children(node);
    node.children
        .iter()
        .copied()
        .find(|child| !boxes.contains(child) && doc.get_node(*child).is_some())
        .unwrap_or(node.id)
}

/// The element text belongs to: for text shaped in an anonymous block's own style, the
/// element the block was made inside.
fn stable_owner(doc: &BaseDocument, id: NodeId) -> NodeId {
    match doc.get_node(id) {
        Some(node) if matches!(node.data, NodeData::AnonymousBlock(_)) => {
            node.layout_parent.get().unwrap_or(id)
        }
        _ => id,
    }
}

#[derive(Default)]
struct Layers {
    negative: Vec<(i32, NodeId)>,
    normal: Vec<NodeId>,
    positioned: Vec<NodeId>,
    positive: Vec<(i32, NodeId)>,
}

/// How far a box's `transform` moves it and everything inside it, when the transform only
/// translates. A translation changes where a box is drawn and nothing else, so it is folded
/// into the box's position. Anything else in the list (a rotation, a scale) is not carried
/// by this display list, and the box is left where layout put it rather than half-moved.
fn translation(style: &ComputedValues, width: f32, height: f32) -> (f32, f32) {
    use style::values::generics::transform::GenericTransformOperation as Operation;

    let mut x = 0.0;
    let mut y = 0.0;
    for operation in style.get_box().transform.0.iter() {
        match operation {
            Operation::Translate(tx, ty) => {
                x += tx.resolve(Length::new(width)).px();
                y += ty.resolve(Length::new(height)).px();
            }
            Operation::TranslateX(t) => x += t.resolve(Length::new(width)).px(),
            Operation::TranslateY(t) => y += t.resolve(Length::new(height)).px(),
            _ => return (0.0, 0.0),
        }
    }
    (x, y)
}

fn border_line(style: BorderStyle) -> BorderLine {
    match style {
        BorderStyle::Solid => BorderLine::Solid,
        BorderStyle::Dashed => BorderLine::Dashed,
        BorderStyle::Dotted => BorderLine::Dotted,
        BorderStyle::Double => BorderLine::Double,
        BorderStyle::Groove => BorderLine::Groove,
        BorderStyle::Ridge => BorderLine::Ridge,
        BorderStyle::Inset => BorderLine::Inset,
        BorderStyle::Outset => BorderLine::Outset,
        BorderStyle::None | BorderStyle::Hidden => BorderLine::None,
    }
}

/// The four corner radii, percentages resolved against the border box and reduced the way
/// CSS reduces radii whose sum exceeds a side (CSS Backgrounds 3, 5.5).
fn corner_radii(style: &ComputedValues, width: f32, height: f32) -> Option<Corners<Radius>> {
    let resolve = |radius: style::values::computed::BorderCornerRadius| Radius {
        x: radius.0.width.0.resolve(Length::new(width)).px().max(0.0),
        y: radius.0.height.0.resolve(Length::new(height)).px().max(0.0),
    };
    let mut corners = Corners {
        top_left: resolve(style.clone_border_top_left_radius()),
        top_right: resolve(style.clone_border_top_right_radius()),
        bottom_right: resolve(style.clone_border_bottom_right_radius()),
        bottom_left: resolve(style.clone_border_bottom_left_radius()),
    };
    if corners.top_left.is_zero()
        && corners.top_right.is_zero()
        && corners.bottom_right.is_zero()
        && corners.bottom_left.is_zero()
    {
        return None;
    }
    let ratio = |side: f32, a: f32, b: f32| {
        if a + b > side && a + b > 0.0 {
            side / (a + b)
        } else {
            1.0
        }
    };
    let factor = ratio(width, corners.top_left.x, corners.top_right.x)
        .min(ratio(width, corners.bottom_left.x, corners.bottom_right.x))
        .min(ratio(height, corners.top_left.y, corners.bottom_left.y))
        .min(ratio(height, corners.top_right.y, corners.bottom_right.y));
    if factor < 1.0 {
        for radius in [
            &mut corners.top_left,
            &mut corners.top_right,
            &mut corners.bottom_right,
            &mut corners.bottom_left,
        ] {
            radius.x *= factor;
            radius.y *= factor;
        }
    }
    Some(corners)
}

fn input_field(
    doc: &BaseDocument,
    node: &Node,
    rect: &Rect,
    layout: &taffy::Layout,
) -> Option<InputField> {
    let element = node.element_data()?;
    let tag: &str = &element.name.local;
    let attr = |name: &str| element.attr(local(name)).map(str::to_string);
    if tag == "select" && matches!(node.data, NodeData::Element(_)) {
        let options = form::select_options(doc, node.id);
        let selected = form::selected_index(doc, node.id, &options);
        let style = node.primary_styles()?;
        return Some(InputField {
            value: selected
                .map(|index| options[index].value.clone())
                .unwrap_or_default(),
            kind: InputKind::Select {
                options: options.into_iter().map(|option| option.label).collect(),
                selected,
            },
            placeholder: None,
            content_rect: content_box(rect, layout),
            style: convert::text_style(&style),
            color: convert::text_color(&style),
        });
    }
    let kind = match tag {
        "textarea" => InputKind::TextArea,
        "input" => {
            let checked = match &element.special_data {
                SpecialElementData::CheckboxInput(checked) => *checked,
                _ => element
                    .attr(local("checked"))
                    .is_some_and(|value| value != "false"),
            };
            match attr("type")
                .as_deref()
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                None | Some("text") | Some("") => InputKind::Text,
                Some("password") => InputKind::Password,
                Some("email") => InputKind::Email,
                Some("number") => InputKind::Number,
                Some("search") => InputKind::Search,
                Some("tel") => InputKind::Tel,
                Some("url") => InputKind::Url,
                Some("checkbox") => InputKind::Checkbox { checked },
                Some("radio") => InputKind::Radio { checked },
                Some("hidden") => return None,
                Some(other) => InputKind::Other(other.to_string()),
            }
        }
        _ => return None,
    };
    // The `value` attribute is what the Host set. blitz-dom's own editor for the field is
    // seeded with a single space when there is no value, and the renderer, not the Host,
    // owns the text once the user types, so the editor is not consulted.
    let value = attr("value").unwrap_or_default();
    let style = node.primary_styles()?;
    Some(InputField {
        kind,
        value,
        placeholder: attr("placeholder"),
        content_rect: content_box(rect, layout),
        style: convert::text_style(&style),
        color: convert::text_color(&style),
    })
}
