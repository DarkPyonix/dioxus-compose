//! Reads stylo's computed values into this crate's neutral types.
//!
//! Everything here works on computed values, after the cascade. Custom properties
//! (`--name: value`) have been substituted into the properties that use them by then, so
//! none of them reaches the display list.

use style::color::{AbsoluteColor, ColorSpace};
use style::computed_values::text_wrap_mode::T as TextWrapMode;
use style::computed_values::white_space_collapse::T as StyloWhiteSpaceCollapse;
use style::properties::ComputedValues;
use style::values::computed::font::{
    FontStyle as StyloFontStyle, GenericFontFamily, LineHeight, SingleFontFamily,
};
use style::values::computed::{Length, Margin, TextTransform};
use style::values::specified::TextAlignKeyword;

use crate::paint::display_list::{Rgba, TextAlign};
use crate::layout::measure::{TextLineHeight, TextStyle, TextWhiteSpace, WhiteSpaceCollapse};

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
        StyloWhiteSpaceCollapse::Collapse
    )
}

/// `white-space`, read from its two longhands.
pub(crate) fn white_space(style: &ComputedValues) -> TextWhiteSpace {
    TextWhiteSpace {
        collapse: match style.clone_white_space_collapse() {
            StyloWhiteSpaceCollapse::Collapse => WhiteSpaceCollapse::Collapse,
            StyloWhiteSpaceCollapse::Preserve => WhiteSpaceCollapse::Preserve,
            StyloWhiteSpaceCollapse::PreserveBreaks => WhiteSpaceCollapse::PreserveBreaks,
            StyloWhiteSpaceCollapse::BreakSpaces => WhiteSpaceCollapse::BreakSpaces,
        },
        wrap: !matches!(style.clone_text_wrap_mode(), TextWrapMode::Nowrap),
    }
}

pub(crate) fn text_transform(style: &ComputedValues) -> TextTransform {
    style.clone_text_transform()
}

/// The text as `text-transform` draws it.
///
/// The case transforms follow Unicode's full case mappings, as browsers do (`ß` becomes
/// `SS` under `uppercase`). `capitalize` upper-cases the first letter or digit of every
/// word, a word being what white space separates; punctuation before it (`(`, `"`) is passed
/// over, as CSS asks for the first typographic letter unit.
pub(crate) fn transform_text(text: &str, transform: TextTransform) -> String {
    let mut out = if transform.contains(TextTransform::UPPERCASE) {
        text.to_uppercase()
    } else if transform.contains(TextTransform::LOWERCASE) {
        text.to_lowercase()
    } else if transform.contains(TextTransform::CAPITALIZE) {
        capitalize(text)
    } else {
        text.to_string()
    };
    if transform.contains(TextTransform::FULL_WIDTH) {
        out = out.chars().map(full_width).collect();
    }
    if transform.contains(TextTransform::FULL_SIZE_KANA) {
        out = out.chars().map(full_size_kana).collect();
    }
    out
}

fn capitalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_word_start = true;
    for c in text.chars() {
        if c.is_whitespace() {
            at_word_start = true;
            out.push(c);
        } else if at_word_start && c.is_alphanumeric() {
            at_word_start = false;
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `full-width`: printable ASCII and the space as their full-width forms.
fn full_width(c: char) -> char {
    match c {
        ' ' => '\u{3000}',
        '!'..='~' => char::from_u32(c as u32 + 0xFEE0).unwrap_or(c),
        _ => c,
    }
}

/// `full-size-kana`: small kana as their full-size counterparts.
fn full_size_kana(c: char) -> char {
    match c {
        'ぁ' => 'あ',
        'ぃ' => 'い',
        'ぅ' => 'う',
        'ぇ' => 'え',
        'ぉ' => 'お',
        'っ' => 'つ',
        'ゃ' => 'や',
        'ゅ' => 'ゆ',
        'ょ' => 'よ',
        'ゎ' => 'わ',
        'ゕ' => 'か',
        'ゖ' => 'け',
        'ァ' => 'ア',
        'ィ' => 'イ',
        'ゥ' => 'ウ',
        'ェ' => 'エ',
        'ォ' => 'オ',
        'ッ' => 'ツ',
        'ャ' => 'ヤ',
        'ュ' => 'ユ',
        'ョ' => 'ヨ',
        'ヮ' => 'ワ',
        'ヵ' => 'カ',
        'ヶ' => 'ケ',
        'ㇰ' => 'ク',
        'ㇱ' => 'シ',
        'ㇲ' => 'ス',
        'ㇳ' => 'ト',
        'ㇴ' => 'ヌ',
        'ㇵ' => 'ハ',
        'ㇶ' => 'ヒ',
        'ㇷ' => 'フ',
        'ㇸ' => 'ヘ',
        'ㇹ' => 'ホ',
        'ㇺ' => 'ム',
        'ㇻ' => 'ラ',
        'ㇼ' => 'リ',
        'ㇽ' => 'ル',
        'ㇾ' => 'レ',
        'ㇿ' => 'ロ',
        'ｧ' => 'ｱ',
        'ｨ' => 'ｲ',
        'ｩ' => 'ｳ',
        'ｪ' => 'ｴ',
        'ｫ' => 'ｵ',
        'ｯ' => 'ﾂ',
        'ｬ' => 'ﾔ',
        'ｭ' => 'ﾕ',
        'ｮ' => 'ﾖ',
        '\u{1B132}' => 'こ',
        '\u{1B150}' => 'ゐ',
        '\u{1B151}' => 'ゑ',
        '\u{1B152}' => 'を',
        '\u{1B155}' => 'コ',
        '\u{1B164}' => 'ヰ',
        '\u{1B165}' => 'ヱ',
        '\u{1B166}' => 'ヲ',
        '\u{1B167}' => 'ン',
        _ => c,
    }
}

/// Whether an inline element takes room of its own beside its text on the line: padding, a
/// border or a margin on its left or right. Its text then no longer starts and ends where
/// the run around it does, which one measurement of the whole run cannot describe.
pub(crate) fn has_inline_spacing(style: &ComputedValues) -> bool {
    let padding = style.get_padding();
    let margin = style.get_margin();
    let margin_takes_room = |margin: &Margin| match margin {
        Margin::LengthPercentage(length) => !length.is_definitely_zero(),
        // `auto` side margins of an inline box are zero.
        Margin::Auto => false,
        _ => true,
    };
    !padding.padding_left.0.is_definitely_zero()
        || !padding.padding_right.0.is_definitely_zero()
        || style.clone_border_left_width().to_f32_px() != 0.0
        || style.clone_border_right_width().to_f32_px() != 0.0
        || margin_takes_room(&margin.margin_left)
        || margin_takes_room(&margin.margin_right)
}
