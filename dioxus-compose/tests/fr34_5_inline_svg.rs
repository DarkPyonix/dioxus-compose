//! An inline `<svg>` written in the page is drawn, the way a workbench draws the icons it
//! writes straight into its markup.
//!
//! The page registers the element's markup with the renderer as an SVG asset and draws that
//! asset in the element's box. Like an SVG drawn by an `<img>`, every `currentColor` in it is
//! written as the CSS `color` of the element that uses it, and a change of that colour draws
//! the registration for the new one. No image resolver is involved: the picture is in the
//! page itself.

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{HtmlConfig, TextMeasureRequest, TextMeasurer, TextMetrics};
use dioxus_compose::prelude::WritableExt;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{AssetKind, EventPayload, Modifier, PropertyKind};
use dioxus_hooks::use_signal;

struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        TextMetrics {
            width: request.text.chars().count() as f32 * 10.0,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

fn icon_button() -> Element {
    let mut pressed = use_signal(|| false);
    let colour = if pressed() {
        "rgb(255, 0, 0)"
    } else {
        "rgb(0, 128, 0)"
    };
    rsx! {
        div {
            style: "width: 16px; height: 16px; color: {colour}",
            onclick: move |_| pressed.set(true),
            svg {
                width: "16",
                height: "16",
                "viewBox": "0 0 16 16",
                path { d: "M4 2l8 6-8 6z", fill: "currentColor" }
            }
        }
    }
}

/// What a batch says about the icon: the SVG assets it registers, as text, and the asset ids
/// its nodes are set to draw.
#[derive(Default)]
struct Seen {
    registered: Vec<(u32, String)>,
    drawn: Vec<i64>,
    clickable: Option<(u32, u64)>,
}

fn seen(bytes: &[u8]) -> Seen {
    let mut seen = Seen::default();
    for record in decode_batch(bytes).expect("the batch decodes") {
        match record {
            Mutation::RegisterAsset {
                asset_id,
                kind: AssetKind::Svg,
                bytes,
            } => seen.registered.push((
                asset_id,
                String::from_utf8(bytes.to_vec()).expect("the SVG is UTF-8"),
            )),
            Mutation::SetProp {
                property: PropertyKind::Asset,
                value: PropertyValue::Integer(id),
                ..
            } => seen.drawn.push(id),
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::Clickable { handler_id },
                ..
            } => seen.clickable = Some((node_id, handler_id)),
            _ => {}
        }
    }
    seen
}

/// Whether `svg` names the colour, written either as hex or as CSS's `rgb()`.
fn names(svg: &str, hex: &str, rgb: &str) -> bool {
    svg.contains(hex) || svg.contains(rgb)
}

/// The registered SVG the batch draws the icon from.
#[track_caller]
fn drawn_svg(seen: &Seen) -> String {
    assert_eq!(
        seen.drawn.len(),
        1,
        "one picture is drawn: {:?}",
        seen.drawn
    );
    let id = seen.drawn[0];
    seen.registered
        .iter()
        .find(|(asset_id, _)| i64::from(*asset_id) == id)
        .map(|(_, svg)| svg.clone())
        .expect("the asset the icon is drawn from was not registered in the same batch")
}

#[test]
fn fr34_5_inline_svg_draws() {
    let mut host = Host::html(icon_button, config);
    let first = seen(host.rebuild().unwrap());
    let svg = drawn_svg(&first);
    assert!(
        svg.starts_with("<svg"),
        "the registered asset is not the element's markup: {svg}"
    );
    assert!(
        svg.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "the markup lacks the SVG namespace an SVG reader needs: {svg}"
    );
    assert!(
        svg.contains("M4 2l8 6-8 6z"),
        "the path inside the svg element is not in the registered markup: {svg}"
    );
    assert!(
        !svg.to_ascii_lowercase().contains("currentcolor"),
        "currentColor reached the renderer unresolved: {svg}"
    );
    assert!(
        names(&svg, "#008000", "rgb(0, 128, 0)"),
        "currentColor is not the element's colour rgb(0, 128, 0): {svg}"
    );
}

#[test]
fn fr34_5_inline_svg_follows_its_colour() {
    let mut host = Host::html(icon_button, config);
    let first = seen(host.rebuild().unwrap());
    drawn_svg(&first);

    let (node_id, handler_id) = first.clickable.expect("the button is clickable");
    let (batch, _) = host
        .dispatch(HostEvent::new(node_id, handler_id, EventPayload::Clicked))
        .expect("the click is dispatched");
    let red = drawn_svg(&seen(batch));
    assert!(
        names(&red, "#ff0000", "rgb(255, 0, 0)"),
        "the icon did not follow the colour to rgb(255, 0, 0): {red}"
    );
    assert!(
        !names(&red, "#008000", "rgb(0, 128, 0)"),
        "the old colour is still drawn: {red}"
    );
}
