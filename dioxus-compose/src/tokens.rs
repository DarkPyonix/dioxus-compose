//! Design token tables.
//!
//! The tables are authored here, in Rust, because Rust is the single source for everything
//! that has two representations, and because values written in Kotlin by hand would go
//! unverified. They are *executed* in the Renderer: codegen writes them into
//! `Protocol.gen.kt` as Kotlin objects, so they enter the Renderer binary at build time and
//! never cross the boundary at run time. The Host still sends roles and never a token
//! table.
//!
//! What the Renderer implementer still owns is the rules these values feed: how an
//! elevation is drawn, how each `ButtonVariant` is styled, and motion.

use crate::schema::{Color, ColorRole, ColorScheme, DesignSystem, ShapeRole, SpaceRole, TypeRole};

/// One `ColorRole` slot in both schemes, in `ColorRole` tag order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorToken {
    pub role: ColorRole,
    pub light: Color,
    pub dark: Color,
}

/// One rung of the nine-rung type ladder (13.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeToken {
    pub role: TypeRole,
    /// Font size in sp.
    pub size: f32,
    /// 100 to 900.
    pub weight: u16,
    /// Line height in sp.
    pub line_height: f32,
    /// Letter spacing in sp, which may be negative.
    pub letter_spacing: f32,
    /// True when the rung uses the monospace family rather than the default one.
    pub monospace: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeToken {
    pub role: ShapeRole,
    /// Corner radius in dp. `Full` uses a radius large enough to read as a pill.
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceToken {
    pub role: SpaceRole,
    /// Spacing in dp.
    pub value: f32,
}

/// Items 1 to 4 of the 14.6 table for one design system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignTokenTable {
    pub system: DesignSystem,
    /// The published guideline the values come from, recorded as 14.3 asks.
    pub reference: &'static str,
    pub default_family: &'static str,
    pub monospace_family: &'static str,
    pub colors: &'static [ColorToken; 14],
    pub type_scale: &'static [TypeToken; 9],
    pub shapes: &'static [ShapeToken; 6],
    pub spaces: &'static [SpaceToken; 7],
}

impl DesignTokenTable {
    pub fn color(&self, role: ColorRole, scheme: ColorScheme) -> Color {
        let token = self.colors[role as usize - 1];
        // `FollowSystem` is resolved by the Renderer, which is the side that knows.
        // Asking the Host for it can only mean light.
        if scheme == ColorScheme::Dark {
            token.dark
        } else {
            token.light
        }
    }
}

/// Every design system's table, in `DesignSystem` tag order.
///
/// A fourth design system is one entry here plus one Renderer rule implementation.
/// Nothing in `widgets.rs`, the Modifier schema or the Property schema moves (14.1).
pub const DESIGN_TOKENS: &[DesignTokenTable] = &[MATERIAL3, APPLE_HIG, FLUENT];

pub fn table(system: DesignSystem) -> &'static DesignTokenTable {
    &DESIGN_TOKENS[system as usize - 1]
}

macro_rules! colors {
    ($($role:ident: $light:literal / $dark:literal),+ $(,)?) => {
        &[$(ColorToken {
            role: ColorRole::$role,
            light: Color::rgb($light),
            dark: Color::rgb($dark),
        }),+]
    };
}

macro_rules! type_scale {
    ($($role:ident: $size:literal / $weight:literal / $line:literal / $spacing:literal / $mono:literal),+ $(,)?) => {
        &[$(TypeToken {
            role: TypeRole::$role,
            size: $size,
            weight: $weight,
            line_height: $line,
            letter_spacing: $spacing,
            monospace: $mono,
        }),+]
    };
}

macro_rules! shapes {
    ($($role:ident: $radius:literal),+ $(,)?) => {
        &[$(ShapeToken { role: ShapeRole::$role, radius: $radius }),+]
    };
}

macro_rules! spaces {
    ($($role:ident: $value:literal),+ $(,)?) => {
        &[$(SpaceToken { role: SpaceRole::$role, value: $value }),+]
    };
}

const MATERIAL3: DesignTokenTable = DesignTokenTable {
    system: DesignSystem::Material3,
    reference: "Material 3 baseline scheme and type scale, m3.material.io, 2024 baseline",
    default_family: "Roboto",
    monospace_family: "Roboto Mono",
    colors: colors! {
        Primary: 0x6750a4 / 0xd0bcff,
        OnPrimary: 0xffffff / 0x381e72,
        Secondary: 0x625b71 / 0xccc2dc,
        OnSecondary: 0xffffff / 0x332d41,
        Surface: 0xfef7ff / 0x141218,
        OnSurface: 0x1d1b20 / 0xe6e0e9,
        SurfaceVariant: 0xe7e0ec / 0x49454f,
        OnSurfaceVariant: 0x49454f / 0xcac4d0,
        Background: 0xfef7ff / 0x141218,
        OnBackground: 0x1d1b20 / 0xe6e0e9,
        Outline: 0x79747e / 0x938f99,
        OutlineVariant: 0xcac4d0 / 0x49454f,
        Error: 0xb3261e / 0xf2b8b5,
        OnError: 0xffffff / 0x601410,
    },
    type_scale: type_scale! {
        Display: 57.0 / 400 / 64.0 / 0.0 / false,
        Headline: 32.0 / 400 / 40.0 / 0.0 / false,
        Title: 22.0 / 400 / 28.0 / 0.0 / false,
        Subtitle: 16.0 / 500 / 24.0 / 0.15 / false,
        Body: 16.0 / 400 / 24.0 / 0.5 / false,
        BodyStrong: 16.0 / 500 / 24.0 / 0.15 / false,
        Label: 14.0 / 500 / 20.0 / 0.1 / false,
        Caption: 12.0 / 400 / 16.0 / 0.4 / false,
        Mono: 14.0 / 400 / 20.0 / 0.0 / true,
    },
    shapes: shapes! {
        None: 0.0,
        ExtraSmall: 4.0,
        Small: 8.0,
        Medium: 12.0,
        Large: 16.0,
        Full: 1000.0,
    },
    spaces: spaces! {
        None: 0.0,
        Xs: 4.0,
        Sm: 8.0,
        Md: 16.0,
        Lg: 24.0,
        Xl: 32.0,
        Xxl: 48.0,
    },
};

const APPLE_HIG: DesignTokenTable = DesignTokenTable {
    system: DesignSystem::Cupertino,
    reference: "Apple Human Interface Guidelines, system colors and Dynamic Type, 2024",
    default_family: "SF Pro",
    monospace_family: "SF Mono",
    colors: colors! {
        Primary: 0x007aff / 0x0a84ff,
        OnPrimary: 0xffffff / 0xffffff,
        Secondary: 0x5856d6 / 0x5e5ce6,
        OnSecondary: 0xffffff / 0xffffff,
        Surface: 0xffffff / 0x1c1c1e,
        OnSurface: 0x000000 / 0xffffff,
        // The page is the grouped background and the variant is the colour an incoming
        // message bubble has. They used to carry the same value, so anything filled with
        // one on a page of the other was drawn, in the right colour, and invisible.
        SurfaceVariant: 0xe9e9eb / 0x2c2c2e,
        OnSurfaceVariant: 0x3c3c43 / 0xebebf5,
        Background: 0xf2f2f7 / 0x000000,
        OnBackground: 0x000000 / 0xffffff,
        Outline: 0xc6c6c8 / 0x38383a,
        OutlineVariant: 0xe5e5ea / 0x48484a,
        Error: 0xff3b30 / 0xff453a,
        OnError: 0xffffff / 0xffffff,
    },
    type_scale: type_scale! {
        Display: 34.0 / 400 / 41.0 / 0.37 / false,
        Headline: 28.0 / 400 / 34.0 / 0.36 / false,
        Title: 22.0 / 400 / 28.0 / 0.35 / false,
        Subtitle: 17.0 / 600 / 22.0 / -0.41 / false,
        Body: 17.0 / 400 / 22.0 / -0.41 / false,
        BodyStrong: 17.0 / 600 / 22.0 / -0.41 / false,
        Label: 15.0 / 400 / 20.0 / -0.24 / false,
        Caption: 12.0 / 400 / 16.0 / 0.0 / false,
        Mono: 15.0 / 400 / 20.0 / 0.0 / true,
    },
    // HIG corners are continuous curvature. The radius is the same number, but the
    // Renderer is expected to draw it with a squircle rather than a circular arc.
    shapes: shapes! {
        None: 0.0,
        ExtraSmall: 4.0,
        Small: 8.0,
        Medium: 10.0,
        Large: 14.0,
        Full: 1000.0,
    },
    spaces: spaces! {
        None: 0.0,
        Xs: 4.0,
        Sm: 8.0,
        Md: 16.0,
        Lg: 20.0,
        Xl: 32.0,
        Xxl: 44.0,
    },
};

const FLUENT: DesignTokenTable = DesignTokenTable {
    system: DesignSystem::Fluent,
    reference: "WinUI / Fluent 2 web and Windows tokens, fluent2.microsoft.design, 2024",
    default_family: "Segoe UI Variable",
    monospace_family: "Cascadia Mono",
    colors: colors! {
        Primary: 0x0f6cbd / 0x479ef5,
        OnPrimary: 0xffffff / 0x000000,
        Secondary: 0xebf3fc / 0x0c3b5e,
        OnSecondary: 0x0f548c / 0xffffff,
        Surface: 0xffffff / 0x292929,
        OnSurface: 0x242424 / 0xffffff,
        // Fluent layers by lightness rather than by outline, and the page it layers on is
        // the solid background base. The old trio sat within five parts of each other, so
        // the layering was there in the numbers and not on the screen.
        SurfaceVariant: 0xeaeaea / 0x333333,
        OnSurfaceVariant: 0x424242 / 0xd6d6d6,
        Background: 0xf3f3f3 / 0x1f1f1f,
        OnBackground: 0x242424 / 0xffffff,
        Outline: 0xd1d1d1 / 0x666666,
        OutlineVariant: 0xe0e0e0 / 0x3d3d3d,
        Error: 0xc50f1f / 0xdc626d,
        OnError: 0xffffff / 0x000000,
    },
    type_scale: type_scale! {
        Display: 40.0 / 600 / 52.0 / 0.0 / false,
        Headline: 28.0 / 600 / 36.0 / 0.0 / false,
        Title: 20.0 / 600 / 28.0 / 0.0 / false,
        Subtitle: 16.0 / 600 / 22.0 / 0.0 / false,
        Body: 14.0 / 400 / 20.0 / 0.0 / false,
        BodyStrong: 14.0 / 600 / 20.0 / 0.0 / false,
        Label: 12.0 / 400 / 16.0 / 0.0 / false,
        Caption: 12.0 / 400 / 16.0 / 0.0 / false,
        Mono: 13.0 / 400 / 18.0 / 0.0 / true,
    },
    shapes: shapes! {
        None: 0.0,
        ExtraSmall: 2.0,
        Small: 3.0,
        Medium: 4.0,
        Large: 8.0,
        Full: 1000.0,
    },
    spaces: spaces! {
        None: 0.0,
        Xs: 2.0,
        Sm: 4.0,
        Md: 8.0,
        Lg: 12.0,
        Xl: 20.0,
        Xxl: 32.0,
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::DESIGN_SYSTEM_SCHEMA;

    fn channel(value: u32) -> f64 {
        let value = f64::from(value) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: Color) -> f64 {
        let argb = color.to_argb();
        0.2126 * channel((argb >> 16) & 0xff)
            + 0.7152 * channel((argb >> 8) & 0xff)
            + 0.0722 * channel(argb & 0xff)
    }

    fn contrast(first: Color, second: Color) -> f64 {
        let (high, low) = {
            let (a, b) = (luminance(first), luminance(second));
            if a > b { (a, b) } else { (b, a) }
        };
        (high + 0.05) / (low + 0.05)
    }

    /// Every `DesignSystem` variant has a table, so adding a variant without
    /// its values fails here rather than at run time.
    #[test]
    fn fr14_every_design_system_has_a_token_table() {
        assert_eq!(DESIGN_TOKENS.len(), DESIGN_SYSTEM_SCHEMA.len());
        for (index, variant) in DESIGN_SYSTEM_SCHEMA.iter().enumerate() {
            let table = DESIGN_TOKENS[index];
            assert_eq!(table.system as u16, variant.tag);
            assert!(
                !table.reference.is_empty(),
                "{} has no source",
                variant.name
            );
            assert!(!table.default_family.is_empty());
            assert!(!table.monospace_family.is_empty());
        }
    }

    /// The tables are ordered by role tag, so a Renderer can index them directly.
    #[test]
    fn fr14_tokens_are_stored_in_role_tag_order() {
        for table in DESIGN_TOKENS {
            for (index, token) in table.colors.iter().enumerate() {
                assert_eq!(token.role as usize, index + 1);
            }
            for (index, token) in table.type_scale.iter().enumerate() {
                assert_eq!(token.role as usize, index + 1);
            }
            for (index, token) in table.shapes.iter().enumerate() {
                assert_eq!(token.role as usize, index + 1);
            }
            for (index, token) in table.spaces.iter().enumerate() {
                assert_eq!(token.role as usize, index + 1);
            }
        }
    }

    /// A filled thing has to be visible on the page it sits on.
    ///
    /// Cupertino gave `SurfaceVariant` and `Background` the same value, so a chat bubble
    /// filled with the variant on a page of the background was drawn, with the right
    /// colour, and was invisible. Layering is the entire reason three roles exist.
    ///
    /// `Background` and `Surface` are deliberately not compared. Material 3 gives them the
    /// same value on purpose and expresses depth through tonal containers and elevation,
    /// so requiring those to differ would be requiring Material 3 to stop being itself.
    #[test]
    fn fr14_surface_variant_is_visible_against_the_page_and_the_surface() {
        /// Summed channel distance in eight bit terms, which is enough to catch a collapse.
        fn apart(first: Color, second: Color) -> u32 {
            (0..3)
                .map(|channel| {
                    let shift = channel * 8;
                    let a = (first.0 >> shift) & 0xff;
                    let b = (second.0 >> shift) & 0xff;
                    a.abs_diff(b)
                })
                .sum()
        }

        for table in DESIGN_TOKENS {
            for scheme in [ColorScheme::Light, ColorScheme::Dark] {
                let variant = table.color(ColorRole::SurfaceVariant, scheme);
                for under in [ColorRole::Background, ColorRole::Surface] {
                    let distance = apart(variant, table.color(under, scheme));
                    assert!(
                        distance >= 24,
                        "{:?} {:?}: SurfaceVariant and {:?} are {} apart, which reads as \
                         one flat surface. Anything filled with one on a page of the other \
                         disappears.",
                        table.system,
                        scheme,
                        under,
                        distance
                    );
                }
            }
        }
    }

    /// The ladder descends from Display to Caption in every system. `Mono` sits
    /// outside the ladder, because it is a family choice and not a rung.
    #[test]
    fn fr13_type_ladder_is_monotonic() {
        for table in DESIGN_TOKENS {
            let ladder = &table.type_scale[..8];
            for pair in ladder.windows(2) {
                assert!(
                    pair[0].size >= pair[1].size,
                    "{:?}: {:?} is not larger than {:?}",
                    table.system,
                    pair[0].role,
                    pair[1].role
                );
            }
            for token in table.type_scale.iter() {
                assert!(token.size > 0.0);
                assert!((100..=900).contains(&token.weight));
                assert!(token.line_height >= token.size);
            }
            assert!(table.type_scale[TypeRole::Mono as usize - 1].monospace);
            assert!(
                table.type_scale[TypeRole::BodyStrong as usize - 1].weight
                    > table.type_scale[TypeRole::Body as usize - 1].weight
            );
        }
    }

    /// Radii and spacing grow with the role, starting at zero.
    #[test]
    fn fr13_shape_and_space_ladders_grow() {
        for table in DESIGN_TOKENS {
            assert_eq!(table.shapes[0].radius, 0.0);
            for pair in table.shapes.windows(2) {
                assert!(pair[1].radius > pair[0].radius, "{:?}", table.system);
            }
            assert_eq!(table.spaces[0].value, 0.0);
            for pair in table.spaces.windows(2) {
                assert!(pair[1].value > pair[0].value, "{:?}", table.system);
            }
        }
    }

    /// `On*` is readable on its pair in both schemes.
    ///
    /// Reading surfaces hold WCAG AA body text (4.5:1). Accent fills are held to 3:1,
    /// which is the AA large-text and non-text bound: Apple's systemBlue with white
    /// sits at about 4.0:1 and its systemRed at about 3.4:1, and those are the shipped
    /// platform colours. Tightening past that would mean inventing colours that no
    /// platform actually uses, which is worse than the looser bound.
    #[test]
    fn fr14_on_roles_stay_readable() {
        let reading = [
            (ColorRole::Surface, ColorRole::OnSurface),
            (ColorRole::SurfaceVariant, ColorRole::OnSurfaceVariant),
            (ColorRole::Background, ColorRole::OnBackground),
        ];
        let accent = [
            (ColorRole::Primary, ColorRole::OnPrimary),
            (ColorRole::Secondary, ColorRole::OnSecondary),
            (ColorRole::Error, ColorRole::OnError),
        ];
        for table in DESIGN_TOKENS {
            for scheme in [ColorScheme::Light, ColorScheme::Dark] {
                for (base, on) in reading {
                    let ratio = contrast(table.color(base, scheme), table.color(on, scheme));
                    assert!(
                        ratio >= 4.5,
                        "{:?} {scheme:?} {base:?}/{on:?} is {ratio:.2}:1",
                        table.system
                    );
                }
                for (base, on) in accent {
                    let ratio = contrast(table.color(base, scheme), table.color(on, scheme));
                    assert!(
                        ratio >= 3.0,
                        "{:?} {scheme:?} {base:?}/{on:?} is {ratio:.2}:1",
                        table.system
                    );
                }
            }
        }
    }

    /// `FollowSystem` is the Renderer's call, so the Host-side lookup can only
    /// report the light value. This documents that, rather than leaving it a surprise.
    #[test]
    fn fr14_follow_system_reads_as_light_on_the_host() {
        let table = table(DesignSystem::Material3);
        assert_eq!(
            table.color(ColorRole::Surface, ColorScheme::FollowSystem),
            table.color(ColorRole::Surface, ColorScheme::Light)
        );
    }
}
