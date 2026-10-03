//! Text measurement, supplied by the caller.
//!
//! Box layout is Taffy's, run over blitz-dom's styled tree. How wide and how tall a piece of
//! text is, though, is a question for whoever draws it: if the layout and the painter
//! disagree about a label's width, glyphs overlap or leave gaps. So the layout pass asks a
//! [`TextMeasurer`] for every run of text it places, and the answer decides the size of the
//! box that holds it.
//!
//! [`ParleyMeasurer`] is the default and measures with Parley, the shaper blitz-dom itself
//! uses. A measurer backed by the renderer's own text engine plugs in through the same trait.

use std::borrow::Cow;

use parley::{
    FontContext, FontStack, FontStyle, FontWeight, Layout, LayoutContext, LineHeight, StyleProperty,
};

/// The font properties that decide how large a run of text is.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// The `font-family` list in CSS order. Generic families keep their CSS keyword
    /// (`sans-serif`, `monospace`, `system-ui`).
    pub font_family: Vec<String>,
    /// Font size in CSS pixels.
    pub font_size: f32,
    /// Numeric weight, 1 to 1000 (`normal` is 400, `bold` is 700).
    pub font_weight: f32,
    /// `font-style: italic` or `oblique`.
    pub italic: bool,
    /// Extra space after every character, in CSS pixels.
    pub letter_spacing: f32,
    /// Height of one line.
    pub line_height: TextLineHeight,
}

impl TextStyle {
    /// The family list as a CSS `font-family` value, names with spaces quoted.
    pub fn font_family_css(&self) -> String {
        let mut out = String::new();
        for (index, family) in self.font_family.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            if is_generic_family(family) || !family.contains(char::is_whitespace) {
                out.push_str(family);
            } else {
                out.push('"');
                out.push_str(&family.replace('"', "\\\""));
                out.push('"');
            }
        }
        out
    }
}

fn is_generic_family(name: &str) -> bool {
    matches!(
        name,
        "serif" | "sans-serif" | "monospace" | "cursive" | "fantasy" | "system-ui"
    )
}

/// `line-height`, resolved to pixels where CSS gives a length or a number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextLineHeight {
    /// `normal`: the font's own metrics decide.
    Normal,
    /// A fixed line height in CSS pixels.
    Px(f32),
}

/// How much horizontal room the text has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WidthConstraint {
    /// Break at every opportunity: the answer's width is the widest unbreakable piece.
    MinContent,
    /// Never wrap except at forced breaks.
    MaxContent,
    /// Wrap so no line is wider than this many CSS pixels.
    AtMost(f32),
}

impl WidthConstraint {
    /// The maximum line width, or `None` when lines may be as long as they like.
    /// `MinContent` has no fixed number; a measurer answers it by breaking everywhere.
    pub fn max_width(&self) -> Option<f32> {
        match self {
            WidthConstraint::AtMost(width) => Some(*width),
            WidthConstraint::MinContent | WidthConstraint::MaxContent => None,
        }
    }
}

/// One question to a [`TextMeasurer`].
#[derive(Clone, Copy, Debug)]
pub struct TextMeasureRequest<'a> {
    /// The text with CSS white space already collapsed. A `\n` is a forced line break.
    pub text: &'a str,
    pub style: &'a TextStyle,
    pub width: WidthConstraint,
}

/// A [`TextMeasurer`]'s answer, in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    /// Width of the widest line, trailing white space excluded.
    pub width: f32,
    /// Height of all lines together.
    pub height: f32,
    /// Distance from the top of the first line to its baseline.
    pub first_baseline: f32,
    /// How many lines the text took.
    pub line_count: u32,
}

/// Measures runs of text for the layout pass.
///
/// Called during layout, possibly several times for the same text under different width
/// constraints (a flex item is asked for its min-content and max-content widths before it
/// is given its final one). The answer must depend only on the request.
pub trait TextMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics;
}

impl<F> TextMeasurer for F
where
    F: FnMut(&TextMeasureRequest<'_>) -> TextMetrics,
{
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        self(request)
    }
}

/// Measures text with Parley.
///
/// This is the shaper blitz-dom uses. It does not apply a font's optical size axis, so for a
/// variable system font such as SF it answers narrower than Chromium or Compose would; a
/// measurer backed by the renderer replaces it where that matters.
pub struct ParleyMeasurer {
    font_ctx: FontContext,
    layout_ctx: LayoutContext<()>,
}

impl ParleyMeasurer {
    /// A measurer over the system's fonts.
    pub fn new() -> Self {
        Self::with_font_context(FontContext::default())
    }

    /// A measurer over the fonts already registered in `font_ctx`.
    pub fn with_font_context(font_ctx: FontContext) -> Self {
        Self {
            font_ctx,
            layout_ctx: LayoutContext::new(),
        }
    }
}

impl Default for ParleyMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextMeasurer for ParleyMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let style = request.style;
        let family = style.font_family_css();
        let mut builder =
            self.layout_ctx
                .ranged_builder(&mut self.font_ctx, request.text, 1.0, true);
        builder.push_default(StyleProperty::FontStack(FontStack::Source(Cow::Owned(
            family,
        ))));
        builder.push_default(StyleProperty::FontSize(style.font_size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            style.font_weight,
        )));
        builder.push_default(StyleProperty::FontStyle(if style.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }));
        builder.push_default(StyleProperty::LetterSpacing(style.letter_spacing));
        builder.push_default(StyleProperty::LineHeight(match style.line_height {
            // The same reading of `normal` blitz-dom applies, so the default measurer and
            // blitz-dom's own text agree.
            TextLineHeight::Normal => LineHeight::FontSizeRelative(1.2),
            TextLineHeight::Px(px) => LineHeight::Absolute(px),
        }));
        let mut layout: Layout<()> = builder.build(request.text);

        let max_advance = match request.width {
            WidthConstraint::MinContent => Some(layout.calculate_content_widths().min),
            WidthConstraint::MaxContent => None,
            WidthConstraint::AtMost(width) => Some(width),
        };
        layout.break_all_lines(max_advance);

        let mut metrics = TextMetrics {
            height: layout.height(),
            ..TextMetrics::default()
        };
        for line in layout.lines() {
            let line_metrics = line.metrics();
            if metrics.line_count == 0 {
                metrics.first_baseline = line_metrics.baseline;
            }
            metrics.line_count += 1;
            metrics.width = metrics
                .width
                .max(line_metrics.advance - line_metrics.trailing_whitespace);
        }
        metrics
    }
}
