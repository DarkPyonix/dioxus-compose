//! The theme's colour roles as CSS custom properties.
//!
//! Every `ColorRole` variant in the schema becomes `--dxc-<role>`, with the variant name in
//! kebab-case: `Primary` is `--dxc-primary` and `OnSurface` is `--dxc-on-surface`. The list
//! is read from the schema's role table, so a role added there gets a variable here with no
//! edit to this file.

use compose_rust::Palette;
use compose_rust::schema::{COLOR_ROLE_SCHEMA, Color, ColorRole, ColorScheme, DesignSystem, Theme};

/// `OnSurfaceVariant` as `on-surface-variant`.
fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// The custom property that carries one role, such as `--dxc-on-surface`.
fn property_name(variant_name: &str) -> String {
    format!("--dxc-{}", kebab(variant_name))
}

/// Every custom property the Host sets, in role order. One per `ColorRole` variant.
pub fn theme_variable_names() -> Vec<String> {
    COLOR_ROLE_SCHEMA
        .iter()
        .map(|variant| property_name(variant.name))
        .collect()
}

fn css_colour(color: Color) -> String {
    let argb = color.to_argb();
    let (alpha, red, green, blue) = (
        (argb >> 24) & 0xff,
        (argb >> 16) & 0xff,
        (argb >> 8) & 0xff,
        argb & 0xff,
    );
    if alpha == 0xff {
        format!("#{red:02x}{green:02x}{blue:02x}")
    } else {
        format!("rgba({red}, {green}, {blue}, {:.4})", alpha as f32 / 255.0)
    }
}

/// The declarations, `--dxc-primary: #6750a4; ...`, for `theme` drawn with `system`.
/// `scheme` stands in for a theme that follows the system, which only the renderer can
/// answer.
pub fn theme_variable_declarations(
    theme: &Theme,
    system: DesignSystem,
    scheme: ColorScheme,
) -> String {
    let scheme = match theme.color_scheme {
        ColorScheme::FollowSystem => scheme,
        named => named,
    };
    let resolved = match theme.palette {
        Some(palette) => palette.resolve(system, scheme),
        None => Palette::new().resolve(system, scheme),
    };
    let mut out = String::new();
    for variant in COLOR_ROLE_SCHEMA {
        let role = ColorRole::try_from(variant.tag).expect("a role from the role schema");
        out.push_str(&property_name(variant.name));
        out.push_str(": ");
        out.push_str(&css_colour(resolved[role as usize - 1]));
        out.push_str("; ");
    }
    out
}
