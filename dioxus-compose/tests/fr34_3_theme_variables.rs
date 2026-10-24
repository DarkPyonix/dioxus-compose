//! The theme's colour roles as `--dxc-<role>` custom properties on the document root, for
//! CSS to use as `var(--dxc-primary)`.

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    HtmlConfig, TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint,
};
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch};
use dioxus_compose::schema::{
    COLOR_ROLE_SCHEMA, ColorScheme, EventPayload, Modifier, Paint,
};
use dioxus_compose::theme_variables::{theme_variable_declarations, theme_variable_names};
use dioxus_compose::tokens::table;
use dioxus_compose::{Color, ColorRole, Host, Palette};

struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let width = match request.width {
            WidthConstraint::AtMost(width) => {
                (request.text.chars().count() as f32 * 10.0).min(width.max(0.0))
            }
            _ => request.text.chars().count() as f32 * 10.0,
        };
        TextMetrics {
            width,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![
            "html, body { margin: 0; padding: 0; } \
             #swatch { width: 100px; height: 20px; background-color: var(--dxc-primary); } \
             #switch { width: 100px; height: 20px; }"
                .to_string(),
        ],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

const BRAND: Color = Color::rgb(0xE8590C);

fn app() -> Element {
    let theme = dioxus_compose::use_theme();
    rsx! {
        div { id: "swatch" }
        div {
            id: "switch",
            onclick: move |_| theme.set_palette(
                Palette::new().with(ColorRole::Primary, BRAND, BRAND),
            ),
        }
    }
}

/// The background colours, then the clickable node, in a decoded batch.
fn backgrounds(bytes: &[u8]) -> Vec<Color> {
    decode_batch(bytes)
        .expect("the batch decodes")
        .into_iter()
        .filter_map(|record| match record {
            Mutation::SetModifier {
                modifier: Modifier::Background(Paint::Literal(color)),
                ..
            } => Some(color),
            _ => None,
        })
        .collect()
}

fn clickable(bytes: &[u8]) -> (u32, u64) {
    decode_batch(bytes)
        .expect("the batch decodes")
        .into_iter()
        .find_map(|record| match record {
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::Clickable { handler_id },
                ..
            } => Some((node_id, handler_id)),
            _ => None,
        })
        .expect("one clickable node")
}

/// `OnSurfaceVariant` written as `on-surface-variant`, by a different route from the
/// generator: split where a capital starts.
fn expected_name(variant: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for ch in variant.chars() {
        if ch.is_ascii_uppercase() || words.is_empty() {
            words.push(String::new());
        }
        words.last_mut().unwrap().push(ch.to_ascii_lowercase());
    }
    format!("--dxc-{}", words.join("-"))
}

#[test]
fn fr34_3_theme_variables_follow_colour_roles() {
    let names = theme_variable_names();
    let wanted: Vec<String> = COLOR_ROLE_SCHEMA
        .iter()
        .map(|variant| expected_name(variant.name))
        .collect();
    assert_eq!(names, wanted, "one variable per ColorRole variant, none more");

    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "no two roles share a variable");

    // Each variable carries its own role's colour in the theme in force.
    let theme = dioxus_compose::theme::current_theme();
    let system = dioxus_compose::design_system();
    let declared = theme_variable_declarations(&theme, system, ColorScheme::Light);
    for variant in COLOR_ROLE_SCHEMA {
        let name = expected_name(variant.name);
        assert!(
            declared.contains(&format!("{name}: ")),
            "{name} is declared in {declared}"
        );
    }
    let primary = table(system).color(ColorRole::Primary, ColorScheme::Light);
    assert!(
        declared.contains(&format!(
            "--dxc-primary: #{:06x};",
            primary.to_argb() & 0x00ff_ffff
        )),
        "Primary is the design system's: {declared}"
    );
}

#[test]
fn fr34_3_theme_switch_same_frame() {
    let mut host = Host::html(app, config);
    let first = host.rebuild().unwrap().to_vec();
    let system = dioxus_compose::design_system();
    let before = table(system).color(ColorRole::Primary, ColorScheme::Light);
    assert_eq!(
        backgrounds(&first),
        vec![before],
        "the div starts in the theme's Primary"
    );

    let (node_id, handler_id) = clickable(&first);
    let (batch, _) = host
        .dispatch(HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::Clicked,
        })
        .expect("the click is dispatched");
    assert_eq!(
        backgrounds(batch),
        vec![BRAND],
        "the same batch that carries the theme change restyles the div"
    );
}
