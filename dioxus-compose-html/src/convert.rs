//! Reads stylo's computed values into this crate's neutral types.
//!
//! Everything here works on computed values, after the cascade. Custom properties
//! (`--name: value`) have been substituted into the properties that use them by then, so
//! none of them reaches the display list.

use style::color::{AbsoluteColor, ColorSpace};
use style::properties::ComputedValues;
use style::values::computed::Length;
use style::values::computed::font::{
    FontStyle as StyloFontStyle, GenericFontFamily, LineHeight, SingleFontFamily,
};
use style::values::specified::TextAlignKeyword;

use crate::display_list::{Rgba, TextAlign};
use crate::measure::{TextLineHeight, TextStyle};

/// An absolute stylo colour as 8-bit sRGB.
pub(crate) fn rgba(color: &AbsoluteColor) -> Rgba {
    let srgb = color.to_color_space(ColorSpace::Srgb);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgba {
        r: channel(srgb.components.0),
        g: channel(srgb.components.1),
        b: channel(srgb.components.2),
        a: channel(srgb.alpha),
    }
}

/// A computed `<color>`, with `currentcolor` and `color-mix()` resolved against the
/// element's `color`.
pub(crate) fn resolve_color(
    color: &style::values::computed::Color,
    style: &ComputedValues,
) -> Rgba {
    let current = style.clone_color();
    rgba(&color.resolve_to_absolute(&current))
}

pub(crate) fn text_color(style: &ComputedValues) -> Rgba {
    rgba(&style.clone_color())
}

fn generic_family_name(generic: GenericFontFamily) -> &'static str {
    match generic {
        GenericFontFamily::None => "sans-serif",
        GenericFontFamily::Serif => "serif",
        GenericFontFamily::SansSerif => "sans-serif",
        GenericFontFamily::Monospace => "monospace",
        GenericFontFamily::Cursive => "cursive",
        GenericFontFamily::Fantasy => "fantasy",
        GenericFontFamily::SystemUi => "system-ui",
    }
}

pub(crate) fn font_size(style: &ComputedValues) -> f32 {
    style.clone_font_size().used_size().px()
}

pub(crate) fn text_style(style: &ComputedValues) -> TextStyle {
    let font_size = font_size(style);
    let family = style.clone_font_family();
    let font_family = family
        .families
        .list
        .iter()
        .map(|family| match family {
            SingleFontFamily::FamilyName(name) => {
                let name: &str = name.name.as_ref();
                name.to_string()
            }
            SingleFontFamily::Generic(generic) => generic_family_name(*generic).to_string(),
        })
        .collect();
    let line_height = match style.clone_line_height() {
        LineHeight::Normal => TextLineHeight::Normal,
        LineHeight::Number(number) => TextLineHeight::Px(font_size * number.0),
        LineHeight::Length(length) => TextLineHeight::Px(length.0.px()),
    };
    TextStyle {
        font_family,
        font_size,
        font_weight: style.clone_font_weight().value(),
        italic: style.clone_font_style() != StyloFontStyle::NORMAL,
        letter_spacing: style
            .clone_letter_spacing()
            .0
            .resolve(Length::new(font_size))
            .px(),
        line_height,
    }
}

pub(crate) fn text_align(style: &ComputedValues) -> TextAlign {
    match style.clone_text_align() {
        TextAlignKeyword::Start => TextAlign::Start,
        TextAlignKeyword::End => TextAlign::End,
        TextAlignKeyword::Left | TextAlignKeyword::MozLeft => TextAlign::Left,
        TextAlignKeyword::Right | TextAlignKeyword::MozRight => TextAlign::Right,
        TextAlignKeyword::Center | TextAlignKeyword::MozCenter => TextAlign::Center,
        TextAlignKeyword::Justify => TextAlign::Justify,
    }
}

/// Whether runs of white space collapse to one space (`white-space: normal` and `nowrap`).
pub(crate) fn collapses_white_space(style: &ComputedValues) -> bool {
    matches!(
        style.clone_white_space_collapse(),
        style::computed_values::white_space_collapse::T::Collapse
    )
}
