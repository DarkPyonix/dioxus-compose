//! The intrinsic width of a row flex container, summed the way Chromium sums it.
//!
//! When a row flex container is sized by its content (a flex item with `flex-basis: auto`,
//! a shrink-to-fit box), its max-content width is the sum of its items' max-content
//! contributions: each item's content width, clamped by its `min-width` and `max-width`
//! (and by its flex base size when it may not shrink or grow), plus its margins.
//!
//! Taffy 0.9 works this sum out per item as the flex base size plus a share of the
//! difference between the contribution and the base size. It divides that difference by
//! one scaled shrink factor, `max(1, flex-shrink * inner base size)`, and multiplies it back
//! by another, `max(1, flex-shrink) * inner base size`. The two agree only when
//! `flex-shrink * inner base size` is at least one, so an item that may shrink by less than
//! one (`flex: none`, `flex-shrink: 0`) and whose contribution is below its base size comes
//! out wrong, as far as hundreds of pixels negative. An icon with `flex: none` and
//! `margin-right: -6px` is such an item, and the button holding it and a label collapsed to
//! its padding: the label measured, but the sum it was part of was negative.
//!
//! [`MeasuredTree::row_flex_intrinsic_width`] repeats Taffy's own per-item arithmetic and,
//! only for a container where that arithmetic loses the contribution, answers with the sum
//! of the contributions, which is what Taffy means to compute and what Chromium computes.
//! Every other container is left to Taffy untouched.

use taffy::{
    AlignItems, AvailableSpace, BoxSizing, Display, FlexDirection, FlexWrap, LayoutInput,
    LayoutPartialTree, Line, MaybeMath, MaybeResolve, NodeId, Overflow, Position, RequestedAxis,
    ResolveOrZero, RunMode, Size, SizingMode, TraversePartialTree,
};

use super::{MeasuredTree, resolve_calc_value};

/// Below this, a difference between two sums of pixels is rounding.
const EPSILON: f32 = 0.01;

/// What Taffy works out for one flex item before it sums the line.
struct Item {
    /// The item's max- or min-content contribution, margins included.
    contribution: f32,
    /// What Taffy adds to the line for the item instead.
    taffy_size: f32,
}

impl MeasuredTree<'_> {
    /// The border-box width of the row flex container `node` sized under the min- or
    /// max-content constraint in `inputs`, when Taffy would get it wrong; `None` when
    /// Taffy gets it right or the container's width does not come from its content.
    pub(super) fn row_flex_intrinsic_width(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
    ) -> Option<f32> {
        let style = self.node(node).style.clone();
        if !matches!(
            style.flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        ) || inputs.known_dimensions.width.is_some()
        {
            return None;
        }
        let wrap = matches!(style.flex_wrap, FlexWrap::Wrap | FlexWrap::WrapReverse);
        match inputs.available_space.width {
            AvailableSpace::MaxContent => {}
            // A wrapping container under a min-content constraint is sized from its
            // items' base sizes, which Taffy sums without the shares.
            AvailableSpace::MinContent if !wrap => {}
            _ => return None,
        }

        // The container's own size, as `compute_flexbox_layout` resolves it.
        let parent_size = inputs.parent_size;
        let padding = style
            .padding
            .resolve_or_zero(parent_size.width, resolve_calc_value);
        let border = style
            .border
            .resolve_or_zero(parent_size.width, resolve_calc_value);
        let padding_border = Size {
            width: padding.left + padding.right + border.left + border.right,
            height: padding.top + padding.bottom + border.top + border.bottom,
        };
        let adjustment = if style.box_sizing == BoxSizing::ContentBox {
            padding_border
        } else {
            Size::ZERO
        };
        let resolve = |size: Size<taffy::Dimension>| {
            size.maybe_resolve(parent_size, resolve_calc_value)
                .maybe_apply_aspect_ratio(style.aspect_ratio)
                .maybe_add(adjustment)
        };
        let min_size = resolve(style.min_size);
        let max_size = resolve(style.max_size);
        let clamped_style_size = if inputs.sizing_mode == SizingMode::InherentSize {
            resolve(style.size).maybe_clamp(min_size, max_size)
        } else {
            Size::NONE
        };
        let min_max_definite = Size {
            width: match (min_size.width, max_size.width) {
                (Some(min), Some(max)) if max <= min => Some(min),
                _ => None,
            },
            height: match (min_size.height, max_size.height) {
                (Some(min), Some(max)) if max <= min => Some(min),
                _ => None,
            },
        };
        let known = inputs
            .known_dimensions
            .or(min_max_definite.or(clamped_style_size).maybe_max(Size {
                width: Some(padding_border.width),
                height: Some(padding_border.height),
            }));
        if known.width.is_some() {
            return None;
        }

        // The constants of the algorithm.
        let margin = style
            .margin
            .resolve_or_zero(parent_size.width, resolve_calc_value);
        let scrollbar_gutter_x = match style.overflow.y {
            Overflow::Scroll => style.scrollbar_width,
            _ => 0.0,
        };
        let scrollbar_gutter_y = match style.overflow.x {
            Overflow::Scroll => style.scrollbar_width,
            _ => 0.0,
        };
        let inset_x = padding_border.width + scrollbar_gutter_x;
        let inset_y = padding_border.height + scrollbar_gutter_y;
        let inner_height = known.height.map(|height| height - inset_y);
        let node_inner_size = Size {
            width: None,
            height: inner_height,
        };
        let gap = style
            .gap
            .width
            .resolve_or_zero(Some(0.0), resolve_calc_value);
        let available_width = inputs.available_space.width;
        let available_height = match known.height {
            Some(height) => AvailableSpace::Definite(height - inset_y),
            None => inputs
                .available_space
                .height
                .maybe_sub(margin.top + margin.bottom)
                .maybe_sub(inset_y),
        };
        let align_items = style.align_items.unwrap_or(AlignItems::Stretch);

        let children: Vec<NodeId> = self.child_ids(node).collect();
        let mut items = Vec::with_capacity(children.len());
        for child in children {
            let child_style = self.node(child).style.clone();
            if child_style.position == Position::Absolute || child_style.display == Display::None {
                continue;
            }
            items.push(self.flex_item(
                child,
                &child_style,
                node_inner_size,
                available_width,
                available_height,
                margin.top + margin.bottom,
                align_items,
                inset_x,
            ));
        }
        let broken = items
            .iter()
            .any(|item| (item.taffy_size - item.contribution).abs() > EPSILON);
        if !broken {
            return None;
        }

        let gaps = gap * items.len().saturating_sub(1) as f32;
        let inner: f32 = items.iter().map(|item| item.contribution).sum::<f32>() + gaps;
        let outer = (inner + inset_x)
            .maybe_clamp(min_size.width, max_size.width)
            .max(inset_x - scrollbar_gutter_x);
        Some(outer)
    }

    /// One item's contribution and what Taffy adds for it, by the steps of
    /// `generate_anonymous_flex_items`, `determine_flex_base_size` and
    /// `determine_container_main_size` in Taffy's flexbox algorithm.
    #[allow(clippy::too_many_arguments)]
    fn flex_item(
        &mut self,
        child: NodeId,
        style: &taffy::Style<blitz_dom::Atom>,
        node_inner_size: Size<Option<f32>>,
        available_width: AvailableSpace,
        available_height: AvailableSpace,
        container_margin_y: f32,
        align_items: AlignItems,
        container_inset_x: f32,
    ) -> Item {
        let padding = style
            .padding
            .resolve_or_zero(node_inner_size.width, resolve_calc_value);
        let border = style
            .border
            .resolve_or_zero(node_inner_size.width, resolve_calc_value);
        let padding_border = Size {
            width: padding.left + padding.right + border.left + border.right,
            height: padding.top + padding.bottom + border.top + border.bottom,
        };
        let adjustment = if style.box_sizing == BoxSizing::ContentBox {
            padding_border
        } else {
            Size::ZERO
        };
        let resolve = |size: Size<taffy::Dimension>| {
            size.maybe_resolve(node_inner_size, resolve_calc_value)
                .maybe_apply_aspect_ratio(style.aspect_ratio)
                .maybe_add(adjustment)
        };
        let size = resolve(style.size);
        let min_size = resolve(style.min_size);
        let max_size = resolve(style.max_size);
        let margin = style
            .margin
            .resolve_or_zero(node_inner_size.width, resolve_calc_value);
        let margin_x = margin.left + margin.right;
        let margin_y = margin.top + margin.bottom;
        let align_self = style.align_self.unwrap_or(align_items);
        let scroll = |overflow: Overflow| matches!(overflow, Overflow::Hidden | Overflow::Scroll);

        // The flex base size.
        let child_min_cross = min_size.height.maybe_add(container_margin_y);
        let child_max_cross = max_size.height.maybe_add(container_margin_y);
        let cross_available = match available_height {
            AvailableSpace::Definite(value) => AvailableSpace::Definite(
                node_inner_size
                    .height
                    .unwrap_or(value)
                    .maybe_clamp(child_min_cross, child_max_cross),
            ),
            AvailableSpace::MinContent => match child_min_cross {
                Some(min) => AvailableSpace::Definite(min),
                None => AvailableSpace::MinContent,
            },
            AvailableSpace::MaxContent => match child_max_cross {
                Some(max) => AvailableSpace::Definite(max),
                None => AvailableSpace::MaxContent,
            },
        };
        let mut child_known = Size {
            width: None,
            height: size.height,
        };
        if align_self == AlignItems::Stretch
            && !style.margin.top.is_auto()
            && !style.margin.bottom.is_auto()
            && child_known.height.is_none()
        {
            child_known.height = cross_available.into_option().maybe_sub(margin_y);
        }
        let child_parent_size = Size {
            width: None,
            height: node_inner_size.height,
        };
        let main_constraint = if available_width == AvailableSpace::MinContent {
            AvailableSpace::MinContent
        } else {
            AvailableSpace::MaxContent
        };
        let flex_basis = style
            .flex_basis
            .maybe_resolve(node_inner_size.width, resolve_calc_value)
            .maybe_add(adjustment.width);
        let flex_basis = match flex_basis.or(size.width) {
            Some(basis) => basis,
            None => self.measure_width(
                child,
                child_known,
                child_parent_size,
                Size {
                    width: main_constraint,
                    height: cross_available,
                },
                SizingMode::ContentSize,
            ),
        }
        .max(padding_border.width);
        let inner_flex_basis = flex_basis - padding_border.width;

        let style_min_main = min_size.width.or(scroll(style.overflow.x).then_some(0.0));
        let resolved_minimum = match style_min_main {
            Some(min) => min,
            None => self
                .measure_width(
                    child,
                    child_known,
                    child_parent_size,
                    Size {
                        width: AvailableSpace::MinContent,
                        height: cross_available,
                    },
                    SizingMode::ContentSize,
                )
                .maybe_min(size.width)
                .maybe_min(max_size.width)
                .max(padding_border.width),
        };

        // The contribution.
        let clamping_basis = Some(flex_basis).maybe_max(size.width);
        let basis_min = clamping_basis.filter(|_| style.flex_shrink == 0.0);
        let basis_max = clamping_basis.filter(|_| style.flex_grow == 0.0);
        let min_main = min_size
            .width
            .maybe_max(basis_min)
            .or(basis_min)
            .unwrap_or(resolved_minimum)
            .max(resolved_minimum);
        let max_main = max_size
            .width
            .maybe_min(basis_max)
            .or(basis_max)
            .unwrap_or(f32::INFINITY);
        let contribution = match (min_main, size.width, max_main) {
            (min, Some(preferred), max) if max <= min || max <= preferred => {
                preferred.min(max).max(min) + margin_x
            }
            (min, _, max) if max <= min => min + margin_x,
            _ if scroll(style.overflow.x) || scroll(style.overflow.y) => flex_basis + margin_x,
            _ => {
                let cross_parent = node_inner_size.height;
                let cross_space = available_height
                    .map_definite_value(|value| cross_parent.unwrap_or(value))
                    .maybe_clamp(child_min_cross, child_max_cross);
                let mut known = Size {
                    width: None,
                    height: size.height,
                };
                if align_self == AlignItems::Stretch && known.height.is_none() {
                    known.height = cross_space.into_option().maybe_sub(margin_y);
                }
                let content = self.measure_width(
                    child,
                    known,
                    node_inner_size,
                    Size {
                        width: available_width,
                        height: cross_space,
                    },
                    SizingMode::InherentSize,
                ) + margin_x;
                content
                    .maybe_clamp(min_size.width, max_size.width)
                    .max(container_inset_x)
            }
        };

        // What Taffy adds for the item: its base size and a share of the difference,
        // scaled down by one factor and back up by another.
        let difference = contribution - flex_basis;
        let fraction = if difference > 0.0 {
            difference / style.flex_grow.max(1.0)
        } else if difference < 0.0 {
            difference / (style.flex_shrink * inner_flex_basis).max(1.0)
        } else {
            0.0
        };
        let share = if fraction > 0.0 {
            style.flex_grow.max(1.0) * fraction
        } else if fraction < 0.0 {
            style.flex_shrink.max(1.0) * inner_flex_basis * fraction
        } else {
            0.0
        };
        Item {
            contribution,
            taffy_size: flex_basis + share,
        }
    }

    /// The width of `child` sized by the rules of a flex item measured for its base size or
    /// contribution, asked exactly as Taffy asks so that Taffy finds the answer cached.
    fn measure_width(
        &mut self,
        child: NodeId,
        known_dimensions: Size<Option<f32>>,
        parent_size: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        sizing_mode: SizingMode,
    ) -> f32 {
        self.compute_child_layout(
            child,
            LayoutInput {
                known_dimensions,
                parent_size,
                available_space,
                sizing_mode,
                axis: RequestedAxis::Horizontal,
                run_mode: RunMode::ComputeSize,
                vertical_margins_are_collapsible: Line::FALSE,
            },
        )
        .size
        .width
    }
}
