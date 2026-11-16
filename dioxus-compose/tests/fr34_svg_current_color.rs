//! An SVG drawn by an `<img>` takes the element's CSS `color` wherever it says
//! `currentColor`, the way an icon in a browser follows the colour of its button.
//!
//! The resolver hands over the SVG's source. The page registers it with every
//! `currentColor` written as the element's colour and draws that registration, and a change
//! of the colour draws the registration for the new one. One SVG keeps at most sixteen
//! colours registered, letting go of the one drawn longest ago, so a colour animation does
//! not grow the renderer's pictures without bound.

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    AssetId, HtmlConfig, ImageResolver, ImageSize, TextMeasureRequest, TextMeasurer, TextMetrics,
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

/// Distinct colours the cap test steps through, well past what one SVG keeps.
const STEPS: u8 = 40;

/// The most colours one SVG keeps registered at once.
const KEPT: usize = 16;

fn stepping_button() -> Element {
    let mut step = use_signal(|| 0u8);
    let red = step() * 6;
    rsx! {
        div {
            style: "width: 16px; height: 16px; color: rgb({red}, 0, 0)",
            onclick: move |_| step += 1,
            img { src: "run.svg", alt: "" }
        }
    }
}

/// The registrations and releases of one batch, and the asset it draws.
#[derive(Default)]
struct Turnover {
    registered: Vec<(u32, String)>,
    released: Vec<u32>,
    drawn: Vec<i64>,
    clickable: Option<(u32, u64)>,
}

fn turnover(bytes: &[u8]) -> Turnover {
    let mut seen = Turnover::default();
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
            Mutation::ReleaseAsset { asset_id } => seen.released.push(asset_id),
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

#[test]
fn fr34_svg_img_keeps_at_most_sixteen_colours_registered() {
    let mut host = Host::html(stepping_button, config);
    // Every registration still live, by id, with the SVG it holds.
    let mut live: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    let mut every_id = std::collections::HashSet::new();
    let mut drawn_id = None;

    let mut batch = turnover(host.rebuild().unwrap());
    for step in 0..=STEPS {
        for (id, svg) in &batch.registered {
            assert!(
                every_id.insert(*id),
                "asset {id} was registered twice; a released colour must come back under a new id"
            );
            live.insert(*id, svg.clone());
        }
        for id in &batch.released {
            assert!(
                live.remove(id).is_some(),
                "step {step}: asset {id} was released without being live"
            );
        }
        assert!(
            live.len() <= KEPT,
            "step {step}: {} colours of one SVG are registered, past {KEPT}",
            live.len()
        );
        if let Some(&id) = batch.drawn.last() {
            drawn_id = Some(u32::try_from(id).expect("an asset id"));
        }
        let drawn = drawn_id.expect("the icon is drawn");
        assert!(
            !batch.released.contains(&drawn),
            "step {step}: the colour drawn in this batch, asset {drawn}, was released in it"
        );
        let svg = live
            .get(&drawn)
            .unwrap_or_else(|| panic!("step {step}: the drawn asset {drawn} is not registered"));
        let want = format!("#{:02x}0000", step * 6);
        assert_eq!(
            svg.matches(want.as_str()).count(),
            2,
            "step {step}: the icon is not drawn in {want}: {svg}"
        );

        if step == STEPS {
            break;
        }
        let (node_id, handler_id) = batch.clickable.expect("the button is clickable");
        let (bytes, _) = host
            .dispatch(HostEvent {
                node_id,
                handler_id,
                payload: EventPayload::Clicked,
            })
            .expect("the click is dispatched");
        let clickable = batch.clickable;
        batch = turnover(bytes);
        batch.clickable = batch.clickable.or(clickable);
    }
    assert!(
        every_id.len() > KEPT,
        "the test never went past {KEPT} colours, so it checked no eviction"
    );
}
