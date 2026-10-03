//! Turns a laid-out blitz-dom document into a [`DisplayList`].
//!
//! Two walks. The first goes down the layout tree and settles, for every box, its absolute
//! rectangle and what its ancestors impose on it: the clip, the accumulated opacity, the
//! scroll container it moves with. The second puts the boxes in CSS paint order (CSS 2.1
//! appendix E, simplified as noted on [`Painter::paint_context`]) and reads each box's
//! decorations from its computed style.

use std::collections::HashMap;

use blitz_dom::node::SpecialElementData;
use blitz_dom::{BaseDocument, Node, NodeData};
use parley::PositionedLayoutItem;
use style::properties::ComputedValues;
use style::properties::generated::longhands::position::computed_value::T as Position;
use style::values::computed::{BorderStyle, Length, Overflow};

use crate::NodeId;
use crate::convert;
use crate::display_list::{
    Border, BorderLine, BoxShadow, Corners, DisplayList, InputField, InputKind, MeasuredText,
    NodeEntry, Radius, Rect, ScrollContainer, Sides, TextAlign, TextRun,
};
use crate::layout::local;

/// What the first walk settles for one box.
struct Placed {
    rect: Rect,
    /// Clip imposed by ancestors.
    clip: Option<Rect>,
    opacity: f32,
    scroll_parent: Option<NodeId>,
    /// Layout children that are boxes, in tree order.
    children: Vec<NodeId>,
    /// Whether this box starts a stacking context, and at which `z-index`.
    context: Option<i32>,
    /// `position` other than `static`.
    positioned: bool,
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
) -> DisplayList {
    let mut painter = Painter {
        doc,
        texts,
        placed: HashMap::new(),
        order: Vec::new(),
    };
    let root = doc.root_element().id;
    painter.place(
        root,
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
    let entries = painter
        .order
        .iter()
        .filter_map(|&id| painter.entry(id))
        .collect();
    DisplayList { entries }
}

struct Painter<'a> {
    doc: &'a BaseDocument,
    texts: &'a HashMap<usize, MeasuredText>,
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

impl Painter<'_> {
    fn place(&mut self, id: NodeId, ambient: Ambient) {
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

        let children: Vec<NodeId> = layout_children(node)
            .into_iter()
            .filter(|&child| doc.get_node(child).is_some_and(is_box))
            .collect();
        for &child in &children {
            self.place(child, inner);
        }
        self.placed.insert(
            id,
            Placed {
                rect,
                clip: ambient.clip,
                opacity,
                scroll_parent: ambient.scroll_parent,
                children,
                context,
                positioned,
            },
        );
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
            border: None,
            radii: None,
            shadows: Vec::new(),
            clip: placed.clip,
            clips_children: false,
            opacity: placed.opacity,
            scroll: None,
            scroll_parent: placed.scroll_parent,
            texts: Vec::new(),
            input: None,
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
                    content_width: layout.content_size.width,
                    content_height: layout.content_size.height,
                    horizontal: scrolls(overflow_x),
                    vertical: scrolls(overflow_y),
                });
            }
            // An anonymous block has no style of its own worth drawing: it inherits from
            // its parent, which draws its own background.
            if entry.visible && !matches!(node.data, NodeData::AnonymousBlock(_)) {
                self.decorate(&mut entry, style, layout);
            }
        }
        drop(style);

        if entry.visible {
            entry.texts = self.text_runs(id, node, &rect, layout);
            entry.input = input_field(node, &rect, layout);
        }
        Some(entry)
    }

    fn decorate(&self, entry: &mut NodeEntry, style: &ComputedValues, layout: &taffy::Layout) {
        let background = convert::resolve_color(&style.clone_background_color(), style);
        if !background.is_transparent() {
            entry.background = Some(background);
        }

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

fn input_field(node: &Node, rect: &Rect, layout: &taffy::Layout) -> Option<InputField> {
    let element = node.element_data()?;
    let tag: &str = &element.name.local;
    let attr = |name: &str| element.attr(local(name)).map(str::to_string);
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
