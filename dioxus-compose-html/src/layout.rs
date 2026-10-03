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
//! does. An inline formatting context that holds text in a single style and no inline boxes
//! is sized by the measurer. Everything else (tables, replaced elements, form controls,
//! inline contexts that mix styles or embed boxes) is handed back to blitz-dom's own
//! implementation for that subtree, so it keeps working, measured by Parley.

use std::cell::Ref;
use std::collections::HashMap;

use blitz_dom::{Atom, BaseDocument, LocalName, Node, NodeData};
use style::values::computed::Length;
use style::values::computed::length_percentage::CalcLengthPercentage;
use taffy::{
    AvailableSpace, CacheTree, Display, Layout, LayoutBlockContainer, LayoutFlexboxContainer,
    LayoutGridContainer, LayoutInput, LayoutOutput, LayoutPartialTree, NodeId, ResolveOrZero,
    RoundTree, RunMode, Size, Style, TraversePartialTree, TraverseTree, compute_block_layout,
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_leaf_layout,
    compute_root_layout, round_layout,
};

use crate::convert;
use crate::display_list::{MeasuredText, Rgba, TextAlign};
use crate::measure::{TextMeasureRequest, TextMeasurer, TextStyle, WidthConstraint};

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
    round_layout(&mut tree, root);
    texts
}

/// Text in an inline formatting context that the measurer can size on its own.
#[derive(Clone, Debug)]
struct PreparedText {
    owner: usize,
    text: String,
    style: TextStyle,
    color: Rgba,
    align: TextAlign,
}

enum Route {
    Hidden,
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
}

/// Decides whether the inline formatting context rooted at `root` can be sized by the
/// measurer alone, and gathers what the measurer needs.
///
/// `None` means it cannot: it embeds inline boxes (an `inline-block`, an `<img>`, an
/// `<input>`), it is a list item with a marker, or its text uses more than one font or
/// colour. Those contexts are laid out by blitz-dom with Parley.
fn prepare_inline(doc: &BaseDocument, root: usize) -> Option<Option<PreparedText>> {
    let node = doc.get_node(root)?;
    let element = node.element_data()?;
    let layout = element.inline_layout_data.as_ref()?;
    if !layout.layout.inline_boxes().is_empty() || element.list_item_data.is_some() {
        return None;
    }

    let mut holders = Vec::new();
    collect_text_holders(doc, root, &mut holders);
    let Some(&first) = holders.first() else {
        return Some(None);
    };

    let styled = |id: usize| {
        doc.get_node(id)
            .and_then(|node| node.primary_styles())
            .map(|style| (convert::text_style(&style), convert::text_color(&style)))
    };
    let (style, color) = styled(first)?;
    for &holder in &holders[1..] {
        let (other_style, other_color) = styled(holder)?;
        if other_style != style || other_color != color {
            return None;
        }
    }

    let root_style = node.primary_styles()?;
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
        text,
        style,
        color,
        align: convert::text_align(&root_style),
    }))
}

/// The elements whose children include a text node that is not only white space, inside
/// the inline formatting context rooted at `id`.
fn collect_text_holders(doc: &BaseDocument, id: usize, holders: &mut Vec<usize>) {
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
                // whose id survives the next layout.
                let holder = child.parent.unwrap_or(id);
                if !text.content.trim().is_empty() && !holders.contains(&holder) {
                    holders.push(holder);
                }
            }
            NodeData::Element(_) => {
                let hidden = child
                    .primary_styles()
                    .is_some_and(|style| style.clone_display().is_none());
                if !hidden {
                    collect_text_holders(doc, child_id, holders);
                }
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
                Route::Blitz => <BaseDocument as LayoutPartialTree>::compute_child_layout(
                    &mut *tree.doc,
                    node_id,
                    inputs,
                ),
                Route::Measured(prepared) => tree.compute_measured(id, prepared, inputs),
                Route::Block => compute_block_layout(tree, node_id, inputs),
                Route::Flex => compute_flexbox_layout(tree, node_id, inputs),
                Route::Grid => compute_grid_layout(tree, node_id, inputs),
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
