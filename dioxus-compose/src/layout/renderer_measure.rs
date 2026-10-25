//! Text measured by the renderer, through the measure call.
//!
//! The renderer draws the text, so it is the one that knows how wide the text is: its
//! fonts, its optical sizes and its line breaking decide where every letter lands. A
//! layout that sized the same text with another shaper would leave gaps and overlaps
//! between the box and the glyphs in it.
//!
//! The layout pass asks for text sizes one run at a time and in the middle of Taffy's own
//! work, which is no place for a boundary crossing per run. So [`RendererMeasurer`] answers
//! from what it already knows, and for a run it does not know it answers with Parley and
//! writes the question down. When the pass is over, [`TextMeasurer::settle`] sends every
//! question written down in one call and keeps the answers, and the page is laid out again
//! with them. The second pass finds its answers in the cache, so a page costs one call, and
//! one more only when the first answers moved a width enough to ask something new.
//!
//! Without a renderer to answer (a call made outside the Host, a renderer that has not
//! composed its first frame, a build with no renderer) the Parley answers stand, and
//! nothing is asked again until the next pass.

use std::collections::HashMap;

use compose_rust::fonts::{FontRef, FontRefs};
use compose_rust::measure::{
    MeasureRequests, MeasureResult, MeasureStatus, TextConstraint, TextStyle as WireStyle, measurer,
};
use compose_rust::schema::GenericFamily;
use compose_rust::schema::{OverflowWrap as WireOverflowWrap, WordBreak as WireWordBreak};
use compose_rust::spans::{TextSpan, TextSpans};

use super::measure::{
    OverflowWrap, ParleyMeasurer, SpanRun, TextLineHeight, TextMeasureRequest, TextMeasurer,
    TextMetrics, TextStyle, TextWhiteSpace, WidthConstraint, WordBreak,
};
use crate::paint::display_list::Rgba;

/// What the whole text says about its letters, which a run inherits and a gap between runs
/// is drawn with.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SpanBase {
    pub italic: bool,
    pub underline: bool,
    pub line_through: bool,
}

/// The CSS font family list as compose-rust's font candidates, in the order CSS tries them.
/// `cursive` and `fantasy` have no counterpart there and are left out, so the next
/// candidate in the list is the one tried.
fn font_refs(family: &[String]) -> FontRefs {
    FontRefs::new(family.iter().filter_map(|name| {
        Some(match name.as_str() {
            "system-ui" => FontRef::Generic(GenericFamily::SystemUi),
            "sans-serif" => FontRef::Generic(GenericFamily::SansSerif),
            "serif" => FontRef::Generic(GenericFamily::Serif),
            "monospace" => FontRef::Generic(GenericFamily::Monospace),
            "cursive" | "fantasy" => return None,
            other => FontRef::System(other.to_string()),
        })
    }))
}

/// The spans compose-rust takes for `runs` laid over a text of `len` bytes.
///
/// The text is drawn and measured with the very same list, so what the renderer measures is
/// what it draws. A run says only what differs from the text, and a stretch between runs is
/// left out unless the whole text is slanted or has a line through it, which a text style
/// cannot say and a span can; then the stretch carries that. `colour` turns a run's colour
/// into the paint it travels as; the measure request passes one that leaves colours out,
/// because a colour does not change a size.
pub(crate) fn wire_spans(
    len: usize,
    base: SpanBase,
    runs: &[SpanRun],
    colour: &mut dyn FnMut(Rgba) -> Option<compose_rust::schema::Paint>,
) -> TextSpans {
    let lined = base.italic || base.underline || base.line_through;
    let mut spans: Vec<(TextSpan, FontRefs)> = Vec::new();
    let gap = |from: usize, to: usize, spans: &mut Vec<(TextSpan, FontRefs)>| {
        if lined && to > from {
            let mut span = TextSpan::new(from as u32, (to - from) as u32);
            span.italic = base.italic;
            span.underline = base.underline;
            span.strikethrough = base.line_through;
            spans.push((span, FontRefs::default()));
        }
    };
    let mut at = 0;
    for run in runs {
        gap(at, run.start, &mut spans);
        let mut span = TextSpan::new(run.start as u32, (run.end - run.start) as u32);
        span.bold = run.bold;
        span.italic = base.italic || run.italic;
        span.underline = base.underline || run.underline;
        span.strikethrough = base.line_through || run.line_through;
        span.color = run.color.and_then(&mut *colour);
        let fonts = run
            .font_family
            .as_deref()
            .map(font_refs)
            .unwrap_or_default();
        spans.push((span, fonts));
        at = run.end;
    }
    gap(at, len, &mut spans);
    TextSpans::new_with_fonts(spans)
}

/// How many answers are kept before the cache starts over. A page asks for a few hundred.
const CACHE_LIMIT: usize = 20_000;

/// Everything the renderer's answer depends on. The font family is not part of it: the
/// page sends no family to the renderer, which draws the design system's face.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    text: String,
    font_size: u32,
    font_weight: u32,
    italic: bool,
    letter_spacing: u32,
    /// `None` is `line-height: normal`.
    line_height: Option<u32>,
    word_break: u8,
    overflow_wrap: u8,
    /// 0 min-content, 1 max-content, 2 at most `width`.
    constraint: u8,
    width: u32,
    wrap: bool,
    collapse: u8,
    /// The encoded runs and their font table; both empty for text in one style.
    spans: Vec<u8>,
    span_fonts: Vec<u8>,
    /// The bits of the zoom the answer was measured at.
    zoom: u32,
}

impl Key {
    fn of(request: &TextMeasureRequest<'_>, spans: &TextSpans, zoom: f32) -> Self {
        let style = request.style;
        let (constraint, width) = match request.width {
            WidthConstraint::MinContent => (0, 0.0),
            WidthConstraint::MaxContent => (1, 0.0),
            WidthConstraint::AtMost(width) => (2, width),
        };
        Self {
            text: request.text.to_owned(),
            font_size: style.font_size.to_bits(),
            font_weight: style.font_weight.to_bits(),
            italic: style.italic,
            letter_spacing: style.letter_spacing.to_bits(),
            line_height: match style.line_height {
                TextLineHeight::Normal => None,
                TextLineHeight::Px(px) => Some(px.to_bits()),
            },
            word_break: style.word_break as u8,
            overflow_wrap: style.overflow_wrap as u8,
            constraint,
            width: width.to_bits(),
            wrap: request.white_space.wrap,
            collapse: request.white_space.collapse as u8,
            spans: spans.as_bytes().to_vec(),
            span_fonts: spans.font_table().to_vec(),
            zoom: zoom.to_bits(),
        }
    }
}

/// A question written down during a pass, owned so it outlives the pass.
struct Question {
    key: Key,
    spans: TextSpans,
    style: TextStyle,
    width: WidthConstraint,
    white_space: TextWhiteSpace,
}

/// A [`TextMeasurer`] that asks the renderer, with Parley standing in for what the
/// renderer has not answered.
pub struct RendererMeasurer {
    fallback: ParleyMeasurer,
    answers: HashMap<Key, TextMetrics>,
    asked: Vec<Question>,
    requests: MeasureRequests,
    results: Vec<MeasureResult>,
    calls: usize,
    /// The factor the page is drawn at, which the renderer measures at as well.
    zoom: f32,
}

impl RendererMeasurer {
    pub fn new() -> Self {
        Self::with_fallback(ParleyMeasurer::new())
    }

    /// A measurer whose stand-in answers come from `fallback`, which carries the fonts
    /// the page registered.
    pub fn with_fallback(fallback: ParleyMeasurer) -> Self {
        Self {
            fallback,
            answers: HashMap::new(),
            asked: Vec::new(),
            requests: MeasureRequests::new(),
            results: Vec::new(),
            calls: 0,
            zoom: 1.0,
        }
    }

    /// How many measure calls this measurer has made.
    pub fn calls(&self) -> usize {
        self.calls
    }

    /// The zoom the text is measured at.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Forgets every answer, for when what the renderer measures with changed.
    pub fn clear(&mut self) {
        self.answers.clear();
        self.asked.clear();
    }
}

impl Default for RendererMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

/// The style the renderer measures with: the properties the page sends when it draws the
/// run, in the same units, so the measured size is the drawn size.
fn wire_style(style: &TextStyle, white_space: TextWhiteSpace) -> WireStyle {
    WireStyle {
        font_size: Some(style.font_size),
        font_weight: Some(style.font_weight.round().clamp(1.0, 1000.0) as u16),
        italic: style.italic,
        letter_spacing: Some(style.letter_spacing),
        // `normal` is the multiple the page draws it with.
        line_height: Some(match style.line_height {
            TextLineHeight::Px(px) => px,
            TextLineHeight::Normal => style.font_size * 1.2,
        }),
        wrap: white_space.wrap,
        word_break: Some(match style.word_break {
            WordBreak::Normal => WireWordBreak::Normal,
            WordBreak::BreakAll => WireWordBreak::BreakAll,
            WordBreak::KeepAll => WireWordBreak::KeepAll,
        }),
        overflow_wrap: Some(match style.overflow_wrap {
            OverflowWrap::Normal => WireOverflowWrap::Normal,
            OverflowWrap::Anywhere => WireOverflowWrap::Anywhere,
            OverflowWrap::BreakWord => WireOverflowWrap::BreakWord,
        }),
        // The page's sizes are CSS pixels, which the system's font scale does not enlarge.
        // The page's zoom travels beside the style: `settle` sets it on the requests before
        // the questions are added, the cache key holds it, and a change of zoom clears the
        // answers.
        absolute_size: true,
        ..WireStyle::default()
    }
}

impl TextMeasurer for RendererMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        self.measure_spans(request, &[])
    }

    fn measure_spans(&mut self, request: &TextMeasureRequest<'_>, runs: &[SpanRun]) -> TextMetrics {
        // Only what changes a size goes in the question: a colour does not.
        let spans = if runs.is_empty() {
            TextSpans::default()
        } else {
            let base = SpanBase {
                italic: request.style.italic,
                ..SpanBase::default()
            };
            wire_spans(request.text.len(), base, runs, &mut |_| None)
        };
        let key = Key::of(request, &spans, self.zoom);
        if let Some(metrics) = self.answers.get(&key) {
            return *metrics;
        }
        let provisional = self.fallback.measure(request);
        if !self.asked.iter().any(|question| question.key == key) {
            self.asked.push(Question {
                key,
                spans,
                style: request.style.clone(),
                width: request.width,
                white_space: request.white_space,
            });
        }
        provisional
    }

    fn invalidate(&mut self) {
        self.clear();
    }

    fn set_zoom(&mut self, zoom: f32) {
        let zoom = if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 };
        if zoom.to_bits() != self.zoom.to_bits() {
            self.zoom = zoom;
            self.clear();
        }
    }

    fn settle(&mut self) -> bool {
        if self.asked.is_empty() {
            return false;
        }
        let questions = std::mem::take(&mut self.asked);
        // Outside a Host call there is no renderer standing still to answer.
        let Ok(mut asker) = measurer() else {
            return false;
        };
        self.requests.clear();
        self.requests.set_zoom(self.zoom);
        for question in &questions {
            let constraint = match question.width {
                WidthConstraint::MinContent => TextConstraint::MinContent,
                WidthConstraint::MaxContent => TextConstraint::MaxContent,
                WidthConstraint::AtMost(width) => TextConstraint::AtMost(width),
            };
            self.requests.text(
                &question.key.text,
                &question.spans,
                &wire_style(&question.style, question.white_space),
                constraint,
            );
        }
        if asker
            .measure(&mut self.requests, &mut self.results)
            .is_err()
        {
            return false;
        }
        self.calls += 1;
        if self.answers.len() + questions.len() > CACHE_LIMIT {
            self.answers.clear();
        }
        let mut learned = false;
        for (question, result) in questions.into_iter().zip(self.results.iter()) {
            if result.status() != MeasureStatus::Ok {
                continue;
            }
            self.answers.insert(
                question.key,
                TextMetrics {
                    width: result.width,
                    height: result.height,
                    first_baseline: if result.first_baseline.is_nan() {
                        0.0
                    } else {
                        result.first_baseline
                    },
                    line_count: result.line_count,
                },
            );
            learned = true;
        }
        learned
    }
}
