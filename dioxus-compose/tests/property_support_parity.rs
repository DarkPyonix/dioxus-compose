//! What each widget may carry, as the Host declares it and as the Renderer enforces it.
//!
//! Two copies of the same fact. The Host's `element!` calls say which attributes a widget
//! accepts, and `NodeTable.supportsProperty` in the Renderer decides which ones it will
//! keep. Nothing compared them, so adding an attribute on one side and forgetting the
//! other produced a property that encoded, crossed the boundary, and was thrown away on
//! arrival with a protocol error nobody was reading.
//!
//! That is not hypothetical. `NavigationItem` was given a colour, the Host sent it, and
//! the table refused it because the colour rule named Text, Button, TextField and Icon.
//! The sample drew in the design system's accent and looked exactly like a sample that
//! had never been changed, which is the worst way for a mistake to present itself.
//!
//! This reads both files as text, which is blunt, and it is the only thing that can see
//! both sides at once.

use std::collections::BTreeSet;

const HOST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
const RENDERER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../dioxus-compose-renderer/desktop/src/renderer/NodeTable.kt"
));

/// The widget name and attribute list of every `element!` call.
///
/// Panics on a shape it cannot read rather than returning nothing: an `element!` written
/// some other way would silently drop that widget out of the comparison, and a comparison
/// of nothing passes.
fn declared() -> Vec<(String, BTreeSet<String>)> {
    let mut found = Vec::new();
    let mut rest = HOST;
    while let Some(start) = rest.find("element!(") {
        rest = &rest[start + "element!(".len()..];
        let end = rest
            .find(");")
            .expect("an element! call that is never closed");
        let body = &rest[..end];
        rest = &rest[end..];

        let name = body
            .split('"')
            .nth(1)
            .unwrap_or_else(|| panic!("element! with no widget name: {body}"))
            .to_string();
        let open = body.find('[').unwrap_or_else(|| {
            panic!("element! for {name} has no attribute list, not even an empty one")
        });
        let close = body[open..]
            .find(']')
            .unwrap_or_else(|| panic!("element! for {name} never closes its attribute list"))
            + open;
        let attributes = body[open + 1..close]
            .split(',')
            .map(str::trim)
            .filter(|attribute| !attribute.is_empty())
            .map(str::to_string)
            .collect();
        found.push((name, attributes));
    }
    assert!(
        found.len() > 20,
        "only {} element! calls were read, so the shape this comparison looks for has \
         changed and most of the vocabulary is no longer being checked",
        found.len()
    );
    found
}

/// `attribute_name` as the Renderer spells the same property.
fn property_of(attribute: &str) -> Option<&'static str> {
    Some(match attribute {
        "text" => "Text",
        "placeholder" => "Placeholder",
        "enabled" => "Enabled",
        "multiline" => "Multiline",
        "item_count" => "ItemCount",
        "item_key" => "ItemKey",
        "type_role" => "TypeRole",
        "font_size" => "FontSize",
        "font_weight" => "FontWeight",
        "line_height" => "LineHeight",
        "letter_spacing" => "LetterSpacing",
        "color" => "Color",
        "text_align" => "TextAlign",
        "max_lines" => "MaxLines",
        "overflow" => "Overflow",
        "arrangement" => "Arrangement",
        "spacing" => "Spacing",
        "space_role" => "SpaceRole",
        "alignment" => "Alignment",
        "variant" => "Variant",
        "asset" => "Asset",
        "checked" => "Checked",
        "steps" => "Steps",
        "determinate" => "Determinate",
        "circular" => "Circular",
        "icon" => "Icon",
        // Everything else is either a Modifier rather than a property, or a property this
        // comparison has not been taught. Returning None skips it rather than failing,
        // because a Modifier is not subject to the rule being checked here.
        _ => return None,
    })
}

/// Every widget the Renderer will keep this property on.
///
/// Read from the `when (property)` arms of `supportsProperty`: each arm lists the
/// properties it governs and then the widgets it allows them on.
fn allowed_widgets(property: &str) -> Option<BTreeSet<String>> {
    let body = RENDERER
        .split("internal fun supportsProperty")
        .nth(1)
        .expect("NodeTable.kt no longer declares supportsProperty");
    let needle = format!("PropertyKind.{property},");
    let single = format!("PropertyKind.{property}\n");
    let at = body.find(&needle).or_else(|| body.find(&single))?;
    // The arm runs from the property to the `->` that answers it, and the answer runs to
    // the blank line that ends the arm.
    let arrow = body[at..].find("->")? + at;
    let answer_end = body[arrow..]
        .find("\n\n")
        .map(|offset| offset + arrow)
        .unwrap_or(body.len());
    let answer = &body[arrow..answer_end];
    let mut widgets = BTreeSet::new();
    let mut scan = answer;
    while let Some(start) = scan.find("WidgetKind.") {
        scan = &scan[start + "WidgetKind.".len()..];
        let name: String = scan
            .chars()
            .take_while(|character| character.is_alphanumeric())
            .collect();
        widgets.insert(name);
    }
    Some(widgets)
}

/// A property the Host offers on a widget is a property the Renderer keeps there.
///
/// The other direction is not checked. The Renderer is allowed to accept a property no
/// widget currently offers, because the schema is shared with hosts that are not this
/// crate; what cannot happen is the Host sending something that is thrown away.
#[test]
fn fr15_every_attribute_the_host_offers_is_one_the_renderer_keeps() {
    let mut refused = Vec::new();
    for (widget, attributes) in declared() {
        for attribute in &attributes {
            let Some(property) = property_of(attribute) else {
                continue;
            };
            let Some(widgets) = allowed_widgets(property) else {
                continue;
            };
            // An arm that names no widget at all answers `true` for every widget.
            if widgets.is_empty() || widgets.contains(&widget) {
                continue;
            }
            refused.push(format!(
                "{widget} offers `{attribute}`, and the Renderer keeps {property} only on \
                 {widgets:?}"
            ));
        }
    }
    assert!(
        refused.is_empty(),
        "these properties encode, cross the boundary and are then discarded on arrival, \
         so the widget draws as though the application had never set them:\n  {}",
        refused.join("\n  ")
    );
}
