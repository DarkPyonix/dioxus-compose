//! Terminal output made fit to show: colours kept as roles, everything else removed.
//!
//! A build log or a test run says what failed in red and what passed in green, so those
//! colours are information and are kept. They are kept as colour roles and never as the
//! literal colours the escape codes name, because a literal colour is one the design
//! system never sees and the one that is wrong in dark mode.
//!
//! Every other sequence (cursor movement, erasing, titles, hyperlinks) is removed, and so
//! is every control character except the line break and the tab, so nothing a program
//! sent to its terminal is drawn as a stray glyph.

use crate::code::CodeSpan;
use dioxus_compose::ColorRole;

/// The role a terminal colour is drawn in.
///
/// Terminal colours carry meaning by convention more than by hue, so each is mapped to the
/// code role whose meaning it usually carries rather than to the closest-looking role:
///
/// - red is what failed or was removed, so `DiffRemoved`;
/// - green is what passed or was added, so `DiffAdded`;
/// - yellow is a warning or something changed, so `DiffModified`;
/// - blue is the usual colour of names and paths, so `SyntaxFunction`;
/// - magenta is the usual colour of keywords and markers, so `SyntaxKeyword`;
/// - cyan is the usual colour of types and identifiers, so `SyntaxType`;
/// - black and the dim greys are quiet text, so `SyntaxComment`;
/// - white is the ordinary ink, so no role at all.
///
/// The diff and syntax roles rather than `Error` and friends because these are the roles a
/// design system tunes against the code block's own background.
fn basic(index: u8) -> Option<ColorRole> {
    match index & 7 {
        0 => Some(ColorRole::SyntaxComment),
        1 => Some(ColorRole::DiffRemoved),
        2 => Some(ColorRole::DiffAdded),
        3 => Some(ColorRole::DiffModified),
        4 => Some(ColorRole::SyntaxFunction),
        5 => Some(ColorRole::SyntaxKeyword),
        6 => Some(ColorRole::SyntaxType),
        _ => None,
    }
}

/// A colour given by its components, mapped by hue onto the same table as the basic eight.
fn by_components(red: u8, green: u8, blue: u8) -> Option<ColorRole> {
    let (r, g, b) = (f32::from(red), f32::from(green), f32::from(blue));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max - min < 30.0 {
        return (max < 128.0).then_some(ColorRole::SyntaxComment);
    }
    let delta = max - min;
    let hue = if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    Some(match hue as u32 {
        0..=19 | 330..=360 => ColorRole::DiffRemoved,
        20..=69 => ColorRole::DiffModified,
        70..=169 => ColorRole::DiffAdded,
        170..=199 => ColorRole::SyntaxType,
        200..=259 => ColorRole::SyntaxFunction,
        _ => ColorRole::SyntaxKeyword,
    })
}

/// One of the 256 indexed colours.
fn indexed(index: u16) -> Option<ColorRole> {
    match index {
        0..=7 => basic(index as u8),
        8..=15 => basic((index - 8) as u8),
        16..=231 => {
            const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            let cube = index - 16;
            by_components(
                LEVELS[usize::from(cube / 36)],
                LEVELS[usize::from((cube / 6) % 6)],
                LEVELS[usize::from(cube % 6)],
            )
        }
        232..=243 => Some(ColorRole::SyntaxComment),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Pen {
    color: Option<ColorRole>,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl Pen {
    fn is_plain(&self) -> bool {
        *self == Self::default()
    }

    /// Applies the parameters of one select-graphic-rendition sequence.
    fn apply(&mut self, parameters: &str) {
        let mut values = parameters
            .split([';', ':'])
            .map(|value| value.parse::<u16>().unwrap_or(0));
        if parameters.is_empty() {
            *self = Self::default();
            return;
        }
        while let Some(value) = values.next() {
            match value {
                0 => *self = Self::default(),
                1 => self.bold = true,
                3 => self.italic = true,
                4 => self.underline = true,
                22 => self.bold = false,
                23 => self.italic = false,
                24 => self.underline = false,
                30..=37 => self.color = basic((value - 30) as u8),
                90..=97 => self.color = basic((value - 90) as u8),
                39 => self.color = None,
                38 | 48 => {
                    let color = match values.next() {
                        Some(5) => values.next().and_then(indexed),
                        Some(2) => {
                            let red = values.next().unwrap_or(0).min(255) as u8;
                            let green = values.next().unwrap_or(0).min(255) as u8;
                            let blue = values.next().unwrap_or(0).min(255) as u8;
                            by_components(red, green, blue)
                        }
                        _ => None,
                    };
                    // Background colours are read so their parameters are consumed, and
                    // then dropped: a code block has one background, the design system's.
                    if value == 38 {
                        self.color = color;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Terminal output with its escapes removed and its colours turned into runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cleaned {
    /// The text to show: at most the number of lines asked for.
    pub text: String,
    pub spans: Vec<CodeSpan>,
    /// How many lines the whole output has, shown or not.
    pub total_lines: usize,
}

struct Writer {
    out: String,
    spans: Vec<CodeSpan>,
    pen: Pen,
    run_start: usize,
    max_lines: usize,
    lines: usize,
    emitting: bool,
}

impl Writer {
    fn set_pen(&mut self, pen: Pen) {
        if pen != self.pen {
            self.close_run();
            self.pen = pen;
            self.run_start = self.out.len();
        }
    }

    fn close_run(&mut self) {
        if self.out.len() > self.run_start && !self.pen.is_plain() {
            self.spans.push(CodeSpan {
                start: self.run_start as u32,
                len: (self.out.len() - self.run_start) as u32,
                role: self.pen.color,
                bold: self.pen.bold,
                italic: self.pen.italic,
                underline: self.pen.underline,
            });
        }
        self.run_start = self.out.len();
    }

    fn push(&mut self, character: char) {
        if character == '\n' {
            self.lines += 1;
            if self.lines >= self.max_lines {
                self.close_run();
                self.emitting = false;
            }
        }
        if self.emitting {
            self.out.push(character);
        }
    }
}

/// Cleans terminal output and keeps its first `max_lines` lines.
///
/// A sequence cut off at the end of the text is held back rather than shown, because the
/// rest of it is most likely in the next delta.
pub fn clean(raw: &str, max_lines: usize) -> Cleaned {
    let mut writer = Writer {
        out: String::with_capacity(raw.len().min(64 * 1024)),
        spans: Vec::new(),
        pen: Pen::default(),
        run_start: 0,
        max_lines: max_lines.max(1),
        lines: 0,
        emitting: true,
    };
    let mut chars = raw.char_indices().peekable();
    while let Some((_, character)) = chars.next() {
        match character {
            '\u{1b}' | '\u{9b}' | '\u{9d}' | '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => {
                let kind = if character == '\u{1b}' {
                    match chars.next() {
                        Some((_, next)) => next,
                        None => break,
                    }
                } else {
                    match character {
                        '\u{9b}' => '[',
                        '\u{9d}' => ']',
                        '\u{90}' => 'P',
                        '\u{98}' => 'X',
                        '\u{9e}' => '^',
                        _ => '_',
                    }
                };
                match kind {
                    '[' => {
                        // Parameters, intermediates, then one final byte.
                        let start = chars.peek().map_or(raw.len(), |(index, _)| *index);
                        let mut finished = None;
                        for (index, byte) in chars.by_ref() {
                            if ('\u{40}'..='\u{7e}').contains(&byte) {
                                finished = Some((index, byte));
                                break;
                            }
                        }
                        let Some((end, final_byte)) = finished else {
                            break;
                        };
                        if final_byte == 'm' && writer.emitting {
                            let mut pen = writer.pen;
                            pen.apply(&raw[start..end]);
                            writer.set_pen(pen);
                        }
                    }
                    ']' | 'P' | 'X' | '^' | '_' => {
                        // A string ended by BEL or by ESC backslash.
                        let mut ended = false;
                        while let Some((_, byte)) = chars.next() {
                            if byte == '\u{7}' || byte == '\u{9c}' {
                                ended = true;
                                break;
                            }
                            if byte == '\u{1b}' {
                                if chars.peek().is_some_and(|(_, next)| *next == '\\') {
                                    chars.next();
                                }
                                ended = true;
                                break;
                            }
                        }
                        if !ended {
                            break;
                        }
                    }
                    // Character set designations carry one more character.
                    '(' | ')' | '*' | '+' | '-' | '.' | '/' | '#' | '%' => {
                        if chars.next().is_none() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            '\r' => {
                // A carriage return before a line break is a Windows line ending. A lone one
                // redraws the line in a terminal, which a static text cannot do, so it is
                // dropped and what followed it stays on the same line.
            }
            '\n' | '\t' => writer.push(character),
            control if control.is_control() => {}
            other => writer.push(other),
        }
    }
    writer.close_run();
    let mut text = writer.out;
    // The final line break is not a line of its own.
    let total_lines = if raw.is_empty() {
        0
    } else {
        writer.lines + usize::from(!raw.ends_with('\n'))
    };
    if text.ends_with('\n') {
        text.pop();
    }
    let length = text.len() as u32;
    let mut spans = writer.spans;
    spans.retain_mut(|span| {
        if span.start >= length {
            return false;
        }
        span.len = span.len.min(length - span.start);
        span.len > 0
    });
    Cleaned {
        text,
        spans,
        total_lines,
    }
}

/// Whether the text holds anything [`clean`] would remove.
pub fn needs_cleaning(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_become_roles_and_other_sequences_disappear() {
        let cleaned = clean(
            "\u{1b}[2J\u{1b}[1;31merror\u{1b}[0m: \u{1b}[32mok\u{1b}[39m\u{1b}]0;title\u{7}!\n",
            20,
        );
        assert_eq!(cleaned.text, "error: ok!");
        assert_eq!(cleaned.total_lines, 1);
        assert_eq!(
            cleaned.spans,
            vec![
                CodeSpan {
                    start: 0,
                    len: 5,
                    role: Some(ColorRole::DiffRemoved),
                    bold: true,
                    italic: false,
                    underline: false,
                },
                CodeSpan {
                    start: 7,
                    len: 2,
                    role: Some(ColorRole::DiffAdded),
                    bold: false,
                    italic: false,
                    underline: false,
                },
            ]
        );
    }

    #[test]
    fn a_sequence_cut_off_at_the_end_is_held_back() {
        assert_eq!(clean("done \u{1b}[3", 20).text, "done ");
    }
}
