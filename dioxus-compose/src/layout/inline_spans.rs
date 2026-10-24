//! An inline formatting context whose text is not all in one style, carried as one text
//! with spans.
//!
//! `<p>a <b>bold</b> c</p>` is one paragraph. Wrapped line by line, the bold word shares a
//! line with what surrounds it, so the renderer has to measure and draw the whole paragraph
//! as one text. The paragraph's string is what blitz-dom collapsed from its text nodes, and
//! the spans say which bytes of it each element contributed. This module recovers those
//! ranges, applies `text-transform` to each stretch with the ranges kept in step, and says
//! what each stretch changes about its letters.
//!
//! compose-rust's spans carry bold, italic, the two lines, a colour and a font family. A
//! context that also changes the font size, the letter spacing, the line height or how
//! words break cannot be carried, and [`run_for`] says so by name instead of drawing it
//! wrongly.

use std::collections::HashMap;
use std::ops::Range;

use style::values::computed::TextTransform;

use super::convert;
use super::measure::{SpanRun, TextStyle, TextWhiteSpace};
use crate::paint::display_list::{Rgba, TextDecoration};

/// What one element says about the letters of its own text.
#[derive(Clone, Debug)]
pub(crate) struct Held {
    pub style: TextStyle,
    pub color: Rgba,
    pub transform: TextTransform,
    pub white_space: TextWhiteSpace,
    pub decoration: TextDecoration,
}

/// The text of one DOM text node, as written, and the element that holds it.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub holder: usize,
    pub raw: String,
}

/// A stretch of the collapsed text and the element whose style it is set in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
    pub holder: usize,
    pub range: Range<usize>,
}

fn is_collapsible(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// Finds where each text node's letters went in `collapsed`, the text blitz-dom built from
/// them, and returns the stretches in order, covering all of `collapsed`.
///
/// White space collapses across nodes: the space ending `"a "` and the one starting `" b"`
/// are one space in the result, and it belongs to the first node that wrote it. A node that
/// is only white space has the style of its holder when `known` says the holder's style was
/// read (it holds other text too), and otherwise that of the node before it (of the one
/// after it, at the start), since a space has no style of its own to show. `<br>` puts a newline into the text
/// that no node wrote; it goes to the stretch that follows it.
///
/// `None` when the letters do not line up, which means something other than white space
/// collapsing changed the text and the ranges cannot be trusted.
pub(crate) fn align(
    collapsed: &str,
    pieces: &[Piece],
    collapses: bool,
    known: &dyn Fn(usize) -> bool,
) -> Option<Vec<Segment>> {
    let mut at = 0usize;
    let mut after_space = false;
    let mut starts = Vec::with_capacity(pieces.len());
    for piece in pieces {
        starts.push(at);
        for c in piece.raw.chars() {
            if collapses && is_collapsible(c) {
                if !after_space && collapsed[at..].starts_with(' ') {
                    at += 1;
                    after_space = true;
                }
                continue;
            }
            after_space = false;
            while c != '\n' && collapsed[at..].starts_with('\n') {
                at += 1;
            }
            if !collapsed[at..].starts_with(c) {
                return None;
            }
            at += c.len_utf8();
        }
    }

    let blank = |piece: &Piece| piece.raw.trim().is_empty();
    let first_holder = pieces.iter().find(|piece| !blank(piece))?.holder;
    let mut segments: Vec<Segment> = Vec::new();
    let mut holder = first_holder;
    for (index, piece) in pieces.iter().enumerate() {
        if !blank(piece) || known(piece.holder) {
            holder = piece.holder;
        }
        let start = if index == 0 { 0 } else { starts[index] };
        if segments.last().map(|last| last.holder) != Some(holder) {
            segments.push(Segment {
                holder,
                range: start..start,
            });
        }
    }
    // Each stretch runs to where the next begins, and the last to the end.
    let begins: Vec<usize> = segments.iter().map(|segment| segment.range.start).collect();
    for (index, segment) in segments.iter_mut().enumerate() {
        segment.range.end = begins.get(index + 1).copied().unwrap_or(collapsed.len());
    }
    Some(segments)
}

/// Keeps the part of `segments` inside `keep`, as ranges counted from `keep.start`.
pub(crate) fn clip(segments: Vec<Segment>, keep: Range<usize>) -> Vec<Segment> {
    segments
        .into_iter()
        .filter_map(|segment| {
            let start = segment.range.start.max(keep.start);
            let end = segment.range.end.min(keep.end);
            (start < end).then(|| Segment {
                holder: segment.holder,
                range: start - keep.start..end - keep.start,
            })
        })
        .collect()
}

/// What `held` changes about its letters compared with `base`, the style of the text as a
/// whole. `Ok(None)` when it changes nothing, and `Err` with the reason when it changes
/// something a span cannot say.
pub(crate) fn run_for(base: &Held, held: &Held) -> Result<Option<SpanRun>, &'static str> {
    let (a, b) = (&base.style, &held.style);
    if a.font_size != b.font_size {
        return Err("changes the font size inside a line of text");
    }
    if a.letter_spacing != b.letter_spacing {
        return Err("changes the letter spacing inside a line of text");
    }
    if a.line_height != b.line_height {
        return Err("changes the line height inside a line of text");
    }
    if a.word_break != b.word_break || a.overflow_wrap != b.overflow_wrap {
        return Err("changes how words break inside a line of text");
    }
    if base.white_space != held.white_space {
        return Err("changes white-space inside a line of text");
    }
    // A span can make letters bold, and nothing can make them lighter again.
    let heavy = |weight: f32| weight >= 600.0;
    let bold = match (heavy(a.font_weight), heavy(b.font_weight)) {
        (false, true) => true,
        (true, true) => false,
        (_, false) if a.font_weight == b.font_weight => false,
        (true, false) => return Err("sets a lighter weight inside bold text"),
        (false, false) => return Err("sets a font weight other than bold"),
    };
    let italic = match (a.italic, b.italic) {
        (false, true) => true,
        (true, false) => return Err("sets upright letters inside italic text"),
        _ => false,
    };
    let underline = held.decoration.underline && !base.decoration.underline;
    let line_through = held.decoration.line_through && !base.decoration.line_through;
    if (base.decoration.underline && !held.decoration.underline)
        || (base.decoration.line_through && !held.decoration.line_through)
    {
        return Err("takes a line off text that has one");
    }
    let run = SpanRun {
        start: 0,
        end: 0,
        bold,
        italic,
        underline,
        line_through,
        color: (held.color != base.color).then_some(held.color),
        font_family: (a.font_family != b.font_family).then(|| b.font_family.clone()),
    };
    let plain = !(run.bold
        || run.italic
        || run.underline
        || run.line_through
        || run.color.is_some()
        || run.font_family.is_some());
    Ok((!plain).then_some(run))
}

/// The text as drawn, with `text-transform` applied to each stretch, and the runs that
/// differ from `base`, with their ranges in the transformed text.
///
/// `offset` is how many bytes come before the text in the string the runs index (an inside
/// list marker).
///
/// A transform is applied to a stretch on its own, so a capitalised word that an element
/// boundary cuts in two would be capitalised twice. That is why `capitalize` is only taken
/// when it is the same everywhere: then the whole text is transformed at once and the
/// stretches are cut from the result.
pub(crate) fn build(
    text: &str,
    segments: &[Segment],
    helds: &HashMap<usize, Held>,
    base: &Held,
    offset: usize,
) -> Result<(String, Vec<SpanRun>), &'static str> {
    let transform_of = |segment: &Segment| helds.get(&segment.holder).map(|held| held.transform);
    let first = segments
        .first()
        .and_then(&transform_of)
        .unwrap_or(base.transform);
    let uniform = segments
        .iter()
        .all(|segment| transform_of(segment) == Some(first));
    if !uniform
        && segments.iter().any(|segment| {
            transform_of(segment).is_some_and(|t| t.contains(TextTransform::CAPITALIZE))
        })
    {
        return Err("mixes text-transform: capitalize with other text-transform values");
    }
    let whole = (uniform && first.contains(TextTransform::CAPITALIZE))
        .then(|| convert::transform_text(text, first));

    let mut out = String::with_capacity(text.len());
    let mut runs: Vec<SpanRun> = Vec::new();
    let mut taken = 0usize;
    for segment in segments {
        let held = helds
            .get(&segment.holder)
            .ok_or("holds text the layout could not read the style of")?;
        let piece: String = match &whole {
            Some(whole) => {
                let through = convert::transform_text(&text[..segment.range.end], first).len();
                let piece = whole
                    .get(taken..through)
                    .ok_or("could not keep its text-transform in step with its elements")?;
                taken = through;
                piece.to_string()
            }
            None => convert::transform_text(&text[segment.range.clone()], held.transform),
        };
        if piece.is_empty() {
            continue;
        }
        let start = offset + out.len();
        out.push_str(&piece);
        let Some(mut run) = run_for(base, held)? else {
            continue;
        };
        run.start = start;
        run.end = offset + out.len();
        let joins = runs
            .last()
            .is_some_and(|last| last.end == run.start && last.same_look(&run));
        if joins {
            if let Some(last) = runs.last_mut() {
                last.end = run.end;
            }
        } else {
            runs.push(run);
        }
    }
    Ok((out, runs))
}
