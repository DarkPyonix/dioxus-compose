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

use compose_rust::measure::{
    MeasureRequests, MeasureResult, MeasureStatus, TextConstraint, TextStyle as WireStyle,
    measurer,
};
use compose_rust::schema::{OverflowWrap as WireOverflowWrap, WordBreak as WireWordBreak};
use compose_rust::spans::TextSpans;

use super::measure::{
    OverflowWrap, ParleyMeasurer, TextLineHeight, TextMeasureRequest, TextMeasurer, TextMetrics,
    TextStyle, TextWhiteSpace, WidthConstraint, WordBreak,
};

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
}

impl Key {
    fn of(request: &TextMeasureRequest<'_>) -> Self {
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
        }
    }
}

/// A question written down during a pass, owned so it outlives the pass.
struct Question {
    key: Key,
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
        }
    }

    /// How many measure calls this measurer has made.
    pub fn calls(&self) -> usize {
        self.calls
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
        // A page zoom plugs in where the requests are built in `settle`: the zoom is set on
        // `self.requests` before the questions are added, the cache key gains the zoom, and
        // a change of zoom calls `clear`.
        absolute_size: true,
        ..WireStyle::default()
    }
}

impl TextMeasurer for RendererMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let key = Key::of(request);
        if let Some(metrics) = self.answers.get(&key) {
            return *metrics;
        }
        let provisional = self.fallback.measure(request);
        if !self.asked.iter().any(|question| question.key == key) {
            self.asked.push(Question {
                key,
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
        let spans = TextSpans::default();
        for question in &questions {
            let constraint = match question.width {
                WidthConstraint::MinContent => TextConstraint::MinContent,
                WidthConstraint::MaxContent => TextConstraint::MaxContent,
                WidthConstraint::AtMost(width) => TextConstraint::AtMost(width),
            };
            self.requests.text(
                &question.key.text,
                &spans,
                &wire_style(&question.style, question.white_space),
                constraint,
            );
        }
        if asker.measure(&mut self.requests, &mut self.results).is_err() {
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
