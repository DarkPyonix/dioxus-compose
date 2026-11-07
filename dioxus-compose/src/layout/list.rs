//! List markers: which marker a `display: list-item` element has, and its number.
//!
//! blitz-dom builds a marker for the list items under an `ol` or `ul`, but it numbers them
//! from the `start` attribute only (`reversed` and `value` are ignored, and `start="0"`
//! overflows), it has no marker for the roman styles, and it draws an outside marker itself
//! with a layout the display list cannot read. So the markers here are worked out from the
//! computed style and the attributes, and measured by the same [`TextMeasurer`] as the text
//! beside them.
//!
//! stylo's servo build parses `list-style-type` as a fixed set of keywords that leaves out
//! `lower-roman` and `upper-roman`. A declaration that uses them is dropped when the
//! stylesheet is parsed, so the element keeps the inherited type and the roman styles never
//! reach this module.

use blitz_dom::BaseDocument;
use blitz_dom::ElementData;
use blitz_dom::node::{ListItemLayout, ListItemLayoutPosition, Marker};
use style::computed_values::list_style_position::T as ListStylePosition;
use style::computed_values::list_style_type::T as ListStyleType;

use crate::layout::local;

/// The marker of one list item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListMarker {
    /// What the marker says, without the space that follows it: `•`, `3.`, `c.`.
    pub text: String,
    /// `list-style-position: inside`.
    pub inside: bool,
}

/// The marker of the element `id`, if it is a list item and its `list-style-type` is not
/// `none`.
pub(crate) fn marker(doc: &BaseDocument, id: usize) -> Option<ListMarker> {
    let node = doc.get_node(id)?;
    node.element_data()?;
    let (list_style_type, inside) = {
        let style = node.primary_styles()?;
        if !style.get_box().display.is_list_item() {
            return None;
        }
        (
            style.clone_list_style_type(),
            matches!(style.clone_list_style_position(), ListStylePosition::Inside),
        )
    };
    let text = marker_text(list_style_type, ordinal(doc, id))?;
    Some(ListMarker { text, inside })
}

fn is_list_item(doc: &BaseDocument, id: usize) -> bool {
    doc.get_node(id)
        .filter(|node| node.element_data().is_some())
        .and_then(|node| node.primary_styles())
        .is_some_and(|style| style.get_box().display.is_list_item())
}

/// The number of the list item `id`, as the list-item counter holds it.
///
/// The items of one parent count together. In an `ol`, the first is `start` (1, or the
/// number of items when the list is `reversed`) and each next one steps by 1 (by -1 when
/// reversed); an item with a `value` attribute takes that value and the items after it
/// carry on from there. Outside an `ol` the first item is 1.
fn ordinal(doc: &BaseDocument, id: usize) -> i32 {
    let Some(parent) = doc
        .get_node(id)
        .and_then(|node| node.parent)
        .and_then(|parent| doc.get_node(parent))
    else {
        return 1;
    };
    let items: Vec<usize> = parent
        .children
        .iter()
        .copied()
        .filter(|&child| is_list_item(doc, child))
        .collect();
    let ordered = parent
        .element_data()
        .is_some_and(|data| data.name.local == local("ol"));
    let reversed = ordered && parent.attr(local("reversed")).is_some();
    let step: i32 = if reversed { -1 } else { 1 };
    let given_start = if ordered {
        parent
            .attr(local("start"))
            .and_then(|start| start.trim().parse::<i32>().ok())
    } else {
        None
    };
    let start = given_start.unwrap_or(if reversed { items.len() as i32 } else { 1 });

    // The counter before the first item, so every item (the first included) steps once.
    let mut value = start.saturating_sub(step);
    for item in items {
        let given = doc
            .get_node(item)
            .and_then(|node| node.attr(local("value")))
            .and_then(|value| value.trim().parse::<i32>().ok());
        value = given.unwrap_or_else(|| value.saturating_add(step));
        if item == id {
            return value;
        }
    }
    value
}

/// The text of a marker, without the space after it.
fn marker_text(list_style_type: ListStyleType, number: i32) -> Option<String> {
    let named = |symbols: &[char]| {
        alphabetic(number, symbols).map_or_else(|| format!("{number}."), |name| format!("{name}."))
    };
    Some(match list_style_type {
        ListStyleType::None => return None,
        ListStyleType::Disc => "\u{2022}".to_string(),
        ListStyleType::Circle => "\u{25e6}".to_string(),
        ListStyleType::Square => "\u{25aa}".to_string(),
        ListStyleType::DisclosureOpen => "\u{25be}".to_string(),
        ListStyleType::DisclosureClosed => "\u{25b8}".to_string(),
        ListStyleType::LowerAlpha => named(&('a'..='z').collect::<Vec<_>>()),
        ListStyleType::UpperAlpha => named(&('A'..='Z').collect::<Vec<_>>()),
        ListStyleType::LowerGreek => named(&LOWER_GREEK.chars().collect::<Vec<_>>()),
        // `decimal`, and the numbering systems this does not draw (`cjk-decimal`,
        // `devanagari`, `hiragana`, ...), which CSS says fall back to `decimal`.
        _ => format!("{number}."),
    })
}

const LOWER_GREEK: &str = "αβγδεζηθικλμνξοπρστυφχψω";

/// The alphabetic counter system: `a`, `b`, ... `z`, `aa`, `ab`, ... Zero and negative
/// numbers have no representation, and the caller falls back to decimal.
fn alphabetic(number: i32, symbols: &[char]) -> Option<String> {
    if number < 1 || symbols.is_empty() {
        return None;
    }
    let base = symbols.len() as i32;
    let mut rest = number;
    let mut digits = Vec::new();
    while rest > 0 {
        rest -= 1;
        digits.push(symbols[(rest % base) as usize]);
        rest /= base;
    }
    Some(digits.into_iter().rev().collect())
}

/// What blitz-dom put at the start of the text of an inline formatting context for a marker
/// inside the item: the marker and the space after it. Empty when the item has none.
pub(crate) fn blitz_inside_prefix(element: &ElementData) -> String {
    match element.list_item_data.as_deref() {
        Some(ListItemLayout {
            marker,
            position: ListItemLayoutPosition::Inside,
        }) => match marker {
            Marker::Char(c) => format!("{c} "),
            Marker::String(text) => text.clone(),
        },
        _ => String::new(),
    }
}
