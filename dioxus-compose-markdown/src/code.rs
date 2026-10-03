//! What a code block shows: which lines, and which runs of them are coloured.

use dioxus_compose::ColorRole;
use std::borrow::Cow;

/// A coloured run of code, in bytes of the text it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeSpan {
    pub start: u32,
    pub len: u32,
    pub role: Option<ColorRole>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl CodeSpan {
    pub fn colored(start: u32, len: u32, role: ColorRole) -> Self {
        Self {
            start,
            len,
            role: Some(role),
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

/// How a block's text is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodeKind {
    /// No language: raw output, whose terminal colours are kept and whose other escapes
    /// are removed.
    Output,
    /// A unified diff, coloured line by line here and now.
    Diff,
    /// A named language, coloured by the highlighter on a worker thread. A language the
    /// highlighter does not know comes back with no colour, which is a normal outcome.
    Language,
}

impl CodeKind {
    pub(crate) fn of(language: Option<&str>) -> Self {
        match language {
            None => Self::Output,
            Some(language) if is_diff(language) => Self::Diff,
            Some(language) if language.trim().is_empty() => Self::Output,
            Some(_) => Self::Language,
        }
    }
}

/// Whether a fence's language word names a unified diff.
pub fn is_diff(language: &str) -> bool {
    language.eq_ignore_ascii_case("diff") || language.eq_ignore_ascii_case("patch")
}

/// The role a diff line is drawn in, decided by how it starts.
///
/// File headers and hunk headers say where a change is rather than what it is, so they get
/// the quiet comment role. The `+` and `-` are left in the text: a screen that tells added
/// from removed by colour alone tells nothing to someone who cannot see the colour.
fn diff_role(line: &str) -> Option<ColorRole> {
    if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
        Some(ColorRole::SyntaxComment)
    } else if line.starts_with('+') {
        Some(ColorRole::DiffAdded)
    } else if line.starts_with('-') {
        Some(ColorRole::DiffRemoved)
    } else {
        None
    }
}

/// The runs of a diff: one per coloured line, not counting its line break.
pub fn diff_spans(text: &str) -> Vec<CodeSpan> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        if let Some(role) = diff_role(content) {
            if !content.is_empty() {
                spans.push(CodeSpan::colored(
                    offset as u32,
                    content.len() as u32,
                    role,
                ));
            }
        }
        offset += line.len();
    }
    spans
}

/// Control characters removed, the line break and the tab kept. Borrowed when there is
/// nothing to remove, which is nearly always.
pub(crate) fn without_controls(text: &str) -> Cow<'_, str> {
    if crate::ansi::needs_cleaning(text) {
        Cow::Owned(crate::ansi::clean(text, usize::MAX).text_with_final_newline(text))
    } else {
        Cow::Borrowed(text)
    }
}

impl crate::ansi::Cleaned {
    /// The cleaned text with the original's final line break restored, so its line count
    /// is the original's.
    fn text_with_final_newline(self, original: &str) -> String {
        let mut text = self.text;
        if original.ends_with('\n') {
            text.push('\n');
        }
        text
    }
}

/// What a code block shows once folding and colouring are applied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Prepared {
    /// The shown lines, without a final line break.
    pub text: String,
    /// Runs known now. A named language's runs arrive later, from the highlighter.
    pub spans: Vec<CodeSpan>,
    pub total_lines: usize,
    pub shown_lines: usize,
}

/// How many lines a text has, not counting an empty one after a final line break.
fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let breaks = text.bytes().filter(|byte| *byte == b'\n').count();
    breaks + usize::from(!text.ends_with('\n'))
}

/// The first `lines` lines of `text`, without the line break after the last of them.
fn first_lines(text: &str, lines: usize) -> &str {
    let mut end = text.len();
    let mut seen = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            seen += 1;
            if seen == lines {
                end = index;
                break;
            }
        }
    }
    let shown = &text[..end];
    shown.strip_suffix('\n').unwrap_or(shown)
}

pub(crate) fn prepare(text: &str, kind: CodeKind, max_lines: usize) -> Prepared {
    let max_lines = max_lines.max(1);
    match kind {
        CodeKind::Output => {
            let cleaned = crate::ansi::clean(text, max_lines);
            Prepared {
                shown_lines: cleaned.total_lines.min(max_lines),
                total_lines: cleaned.total_lines,
                text: cleaned.text,
                spans: cleaned.spans,
            }
        }
        CodeKind::Diff | CodeKind::Language => {
            let shown_text = without_controls(text);
            let total_lines = line_count(&shown_text);
            let shown = first_lines(&shown_text, max_lines).to_owned();
            let spans = if kind == CodeKind::Diff {
                diff_spans(&shown)
            } else {
                Vec::new()
            };
            Prepared {
                shown_lines: total_lines.min(max_lines),
                total_lines,
                text: shown,
                spans,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_keeps_the_first_lines_and_counts_the_rest() {
        let text: String = (1..=25).map(|line| format!("line {line}\n")).collect();
        let prepared = prepare(&text, CodeKind::Language, 20);
        assert_eq!(prepared.total_lines, 25);
        assert_eq!(prepared.shown_lines, 20);
        assert!(prepared.text.ends_with("line 20"));
        let all = prepare(&text, CodeKind::Language, usize::MAX);
        assert!(all.text.ends_with("line 25"));
    }

    #[test]
    fn diff_lines_are_coloured_by_their_first_character() {
        let spans = diff_spans("--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n same\n");
        let roles: Vec<_> = spans.iter().map(|span| span.role).collect();
        assert_eq!(
            roles,
            vec![
                Some(ColorRole::SyntaxComment),
                Some(ColorRole::SyntaxComment),
                Some(ColorRole::SyntaxComment),
                Some(ColorRole::DiffRemoved),
                Some(ColorRole::DiffAdded),
            ]
        );
    }
}
