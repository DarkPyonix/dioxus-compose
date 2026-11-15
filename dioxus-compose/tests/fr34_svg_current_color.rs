//! An SVG drawn by an `<img>` takes the element's CSS `color` wherever it says
//! `currentColor`, the way an icon in a browser follows the colour of its button.
//!
//! The resolver hands over the SVG's source. The page registers it with every
//! `currentColor` written as the element's colour and draws that registration, and a change
//! of the colour draws the registration for the new one.

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    AssetId, HtmlConfig, ImageResolver, ImageSize, TextMeasureRequest, TextMeasurer,
    TextMetrics,
};
use dioxus_compose::prelude::WritableExt;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{AssetKind, EventPayload, Modifier, PropertyKind};
use dioxus_hooks::use_signal;

static ICON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path d="M4 2l8 6-8 6z" fill="none" stroke="currentColor" stroke-width="2"/><circle cx="8" cy="8" r="1" fill="CurrentColor"/></svg>"#;

/// The asset the resolver names for the icon. An SVG with `currentColor` is drawn from its
/// own registration instead, so this one appearing on the wire is the failure.
const UNTINTED: AssetId = AssetId(9_999);

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

struct Icons;

impl ImageResolver for Icons {
    fn resolve(&mut self, url: &str) -> Option<AssetId> {
        (url == "run.svg").then_some(UNTINTED)
    }

    fn size(&mut self, url: &str) -> Option<ImageSize> {
        (url == "run.svg").then_some(ImageSize::new(16, 16))
    }

    fn svg(&mut self, url: &str) -> Option<&'static [u8]> {
        (url == "run.svg").then_some(ICON)
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        measurer: Some(Box::new(FixedAdvance)),
        images: Some(Box::new(Icons)),
        ..HtmlConfig::default()
    }
}

fn run_button() -> Element {
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
            img { src: "run.svg", alt: "" }
        }
    }
}

/// What a batch says about the icon: the assets it registers, as text, and the asset ids
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

/// The registered SVG the batch draws the icon from.
#[track_caller]
fn drawn_svg(seen: &Seen) -> String {
    assert_eq!(seen.drawn.len(), 1, "one picture is drawn: {:?}", seen.drawn);
    let id = seen.drawn[0];
    assert_ne!(
        id,
        i64::from(UNTINTED.0),
        "the icon was drawn from the untinted asset the resolver named"
    );
    seen.registered
        .iter()
        .find(|(asset_id, _)| i64::from(*asset_id) == id)
        .map(|(_, svg)| svg.clone())
        .expect("the asset the icon is drawn from was not registered in the same batch")
}

#[test]
fn fr34_svg_img_takes_current_color_from_its_element() {
    let mut host = Host::html(run_button, config);
    let first = seen(host.rebuild().unwrap());
    let green = drawn_svg(&first);
    assert!(
        !green.to_ascii_lowercase().contains("currentcolor"),
        "currentColor reached the renderer unresolved: {green}"
    );
    assert_eq!(
        green.matches("#008000").count(),
        2,
        "both currentColor uses are not rgb(0, 128, 0): {green}"
    );

    let (node_id, handler_id) = first.clickable.expect("the button is clickable");
    let (batch, _) = host
        .dispatch(HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::Clicked,
        })
        .expect("the click is dispatched");
    let after = seen(batch);
    let red = drawn_svg(&after);
    assert_eq!(
        red.matches("#ff0000").count(),
        2,
        "the icon did not follow the colour to rgb(255, 0, 0): {red}"
    );
    assert!(!red.contains("#008000"), "the old colour is still drawn: {red}");
}
