//! A code editor shaped like Monaco's view, written with HTML elements and CSS.
//!
//! The DOM is Monaco's: `.monaco-editor > .overflow-guard`, a gutter of line numbers in
//! `.margin`, and `.monaco-scrollable-element > .lines-content` holding the overlays
//! (current line, selection), `.view-lines` with one `.view-line` per line and one
//! `span.mtkN` per token, and the cursor layer. A hidden `textarea.inputarea` sits at the
//! cursor and takes the typing, as Monaco's does.
//!
//! Only the lines in view, and a few either side, are in the DOM. Each line is placed
//! absolutely at `index * LINE_HEIGHT`, and scrolling moves `.lines-content` (and the
//! gutter's overlays) by the scroll offset, so a line that stays in view keeps its node and
//! its `top`. That is how Monaco's `ViewLines` and `Margin` do it. [`ScrollMode::Transform`]
//! moves them with `transform: translateY` instead, for comparison.
//!
//! The view keeps no state of its own beyond the [`Editor`] signal, so the measuring
//! harnesses drive it the way a handler does: [`drive`] writes the signal from inside the
//! runtime, and the next render shows it.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use dioxus_compose::html::prelude::dioxus_elements::InteractionElementOffset;
use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::{ReadableExt, Signal, WritableExt};

/// The vendored Monaco rules, then what Monaco writes at run time.
pub const STYLE: &str = concat!(
    include_str!("../monaco/monaco-subset.css"),
    "\n",
    include_str!("style.css"),
);

pub const LINE_HEIGHT: f32 = 19.0;
/// Menlo at 14px advances 8.43px per character. The view places the cursor and the
/// selection with it, as Monaco does with the width it measures once per font.
pub const CHAR_WIDTH: f32 = 8.43;
pub const GUTTER_WIDTH: f32 = 64.0;
/// Lines kept in the DOM above and below the ones in view.
pub const OVERSCAN: usize = 5;
pub const SCROLLBAR_WIDTH: f32 = 14.0;
const MIN_SLIDER: f32 = 20.0;
/// Positions are written relative to a multiple of this many pixels at or above the
/// scroll offset, as Monaco's `bigNumbersDelta` does, so no box and no offset in the page
/// grows with the file. A 100,000 line file is 1,900,000px tall, and Compose cannot size a
/// box past 262,143px.
pub const BIG_NUMBERS_BLOCK: f32 = 100_000.0;
/// The window the editor is measured in: small enough for a 1024x768 CI screen.
pub const VIEWPORT: (f32, f32) = (960.0, 600.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollMode {
    /// `top: -scroll` on `.lines-content`, as Monaco writes it.
    Top,
    /// `transform: translateY(-scroll)` on `.lines-content`.
    Transform,
}

impl ScrollMode {
    pub fn name(self) -> &'static str {
        match self {
            ScrollMode::Top => "top",
            ScrollMode::Transform => "transform",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The editor's size in CSS pixels, which decides how many lines are in the DOM.
    pub viewport: (f32, f32),
    pub scroll_mode: ScrollMode,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            viewport: VIEWPORT,
            scroll_mode: ScrollMode::Top,
        }
    }
}

static OPTIONS: Mutex<Option<Options>> = Mutex::new(None);
static SOURCE: Mutex<Option<Arc<Vec<String>>>> = Mutex::new(None);

/// Sets what every editor made from now on is laid out with.
pub fn set_options(options: Options) {
    *OPTIONS.lock().unwrap() = Some(options);
}

pub fn options() -> Options {
    OPTIONS.lock().unwrap().unwrap_or_default()
}

/// Sets the text every editor made from now on starts with.
pub fn set_source(lines: Vec<String>) {
    *SOURCE.lock().unwrap() = Some(Arc::new(lines));
}

fn source() -> Vec<String> {
    let shared = SOURCE.lock().unwrap().clone();
    match shared {
        Some(lines) => lines.as_ref().clone(),
        None => generate(100_000),
    }
}

/// A file of `count` lines of Rust-like code: a doc comment, a function with a loop, a
/// branch, string and number literals and a line comment, repeated with the numbers
/// changed, so the lines vary in length (0 to about 90 characters) and in tokens.
pub fn generate(count: usize) -> Vec<String> {
    let mut lines = Vec::with_capacity(count);
    let mut block = 0usize;
    while lines.len() < count {
        let k = block;
        let template = [
            format!("/// Handler {k}: collects the digits of `input`, scaled."),
            format!(
                "pub fn handler_{k}(input: &str, count: usize) -> Result<Vec<u32>, ParseError> {{"
            ),
            format!("    let mut values = Vec::with_capacity(count + {});", k % 97),
            "    for (index, ch) in input.chars().enumerate() {".to_string(),
            "        if ch.is_ascii_digit() && index % 3 == 0 {".to_string(),
            format!(
                "            values.push(ch as u32 * 0x{:04x} + {}); // scaled digit",
                k % 65_536,
                k % 13
            ),
            "        } else if ch == '\"' {".to_string(),
            format!(
                "            return Err(ParseError::new(\"unexpected quote in handler {k}\", index));"
            ),
            "        }".to_string(),
            "    }".to_string(),
            "    Ok(values)".to_string(),
            "}".to_string(),
            String::new(),
        ];
        for line in template {
            if lines.len() == count {
                break;
            }
            lines.push(line);
        }
        block += 1;
    }
    lines
}

/// A place in the text: a line index and a character index in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

pub struct Editor {
    pub lines: Vec<String>,
    pub cursor: Position,
    /// The selection, from its anchor to the cursor, when there is one.
    pub anchor: Option<Position>,
    pub scroll_top: f32,
    /// How many characters the hidden textarea has delivered so far. It is uncontrolled,
    /// so it keeps everything typed, and each `input` carries the whole of it.
    typed: usize,
}

impl Editor {
    pub fn new(lines: Vec<String>) -> Self {
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        // A cursor on the third line with four characters selected before it, so the
        // first screen shows the current line, the cursor and a selection.
        let line = 2usize.min(lines.len() - 1);
        let length = lines[line].chars().count();
        Self {
            cursor: Position {
                line,
                column: 8usize.min(length),
            },
            anchor: Some(Position {
                line,
                column: 4usize.min(length),
            }),
            lines,
            scroll_top: 0.0,
            typed: 0,
        }
    }

    pub fn content_height(&self) -> f32 {
        self.lines.len() as f32 * LINE_HEIGHT
    }

    pub fn max_scroll(&self, viewport_height: f32) -> f32 {
        (self.content_height() - viewport_height).max(0.0)
    }

    pub fn scroll_by(&mut self, delta: f32, viewport_height: f32) {
        self.scroll_top = (self.scroll_top + delta).clamp(0.0, self.max_scroll(viewport_height));
    }

    pub fn scroll_to(&mut self, top: f32, viewport_height: f32) {
        self.scroll_top = top.clamp(0.0, self.max_scroll(viewport_height));
    }

    /// Puts the cursor at `line`, `column` with no selection, and scrolls it into view.
    pub fn jump_to(&mut self, line: usize, column: usize, viewport_height: f32) {
        let line = line.min(self.lines.len() - 1);
        let column = column.min(self.lines[line].chars().count());
        self.cursor = Position { line, column };
        self.anchor = None;
        self.reveal_cursor(viewport_height);
    }

    /// Scrolls the least distance that brings the cursor's line into view.
    pub fn reveal_cursor(&mut self, viewport_height: f32) {
        let top = self.cursor.line as f32 * LINE_HEIGHT;
        if top < self.scroll_top {
            self.scroll_to(top, viewport_height);
        } else if top + LINE_HEIGHT > self.scroll_top + viewport_height {
            self.scroll_to(top + LINE_HEIGHT - viewport_height, viewport_height);
        }
    }

    /// Types `text` at the cursor, replacing the selection, the way a keystroke does.
    pub fn insert(&mut self, text: &str, viewport_height: f32) {
        self.delete_selection();
        for ch in text.chars() {
            let Position { line, column } = self.cursor;
            let at = byte_at(&self.lines[line], column);
            if ch == '\n' {
                let rest = self.lines[line].split_off(at);
                self.lines.insert(line + 1, rest);
                self.cursor = Position {
                    line: line + 1,
                    column: 0,
                };
            } else {
                self.lines[line].insert(at, ch);
                self.cursor.column += 1;
            }
        }
        self.reveal_cursor(viewport_height);
    }

    /// Deletes `count` characters before the cursor, joining lines at a line start.
    pub fn backspace(&mut self, count: usize, viewport_height: f32) {
        if self.delete_selection() {
            return;
        }
        for _ in 0..count {
            let Position { line, column } = self.cursor;
            if column > 0 {
                let at = byte_at(&self.lines[line], column - 1);
                self.lines[line].remove(at);
                self.cursor.column -= 1;
            } else if line > 0 {
                let tail = self.lines.remove(line);
                let previous = &mut self.lines[line - 1];
                let column = previous.chars().count();
                previous.push_str(&tail);
                self.cursor = Position {
                    line: line - 1,
                    column,
                };
            }
        }
        self.reveal_cursor(viewport_height);
    }

    fn delete_selection(&mut self) -> bool {
        let Some(anchor) = self.anchor.take() else {
            return false;
        };
        let (start, end) = if anchor <= self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        };
        if start == end {
            return false;
        }
        let tail = {
            let last = &self.lines[end.line];
            last[byte_at(last, end.column)..].to_string()
        };
        let first = &mut self.lines[start.line];
        let at = byte_at(first, start.column);
        first.truncate(at);
        first.push_str(&tail);
        self.lines.drain(start.line + 1..=end.line);
        self.cursor = start;
        true
    }

    /// What the hidden textarea delivered: its whole text. What it gained since the last
    /// delivery is typed at the cursor, and what it lost is deleted before it.
    pub fn accept_typed(&mut self, value: &str, viewport_height: f32) {
        let count = value.chars().count();
        if count > self.typed {
            let added: String = value.chars().skip(self.typed).collect();
            self.insert(&added, viewport_height);
        } else if count < self.typed {
            self.backspace(self.typed - count, viewport_height);
        }
        self.typed = count;
    }
}

/// The byte offset of character `column` in `line`, or its end.
fn byte_at(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map_or(line.len(), |(at, _)| at)
}

thread_local! {
    /// The editor of the page on this thread, for [`drive`].
    static EDITOR: RefCell<Option<Signal<Editor>>> = const { RefCell::new(None) };
}

/// Changes the page's editor the way a handler would. Must run inside the page's runtime
/// (a scope of its `VirtualDom`, or `Runtime::run_in_context`). Returns `None` before the
/// page has rendered once.
pub fn drive<R>(change: impl FnOnce(&mut Editor) -> R) -> Option<R> {
    let signal = EDITOR.with(|slot| *slot.borrow())?;
    let mut signal = signal;
    let mut editor = signal.write();
    Some(change(&mut *editor))
}

/// The token classes, Monaco's names for Dark+'s colours.
const PLAIN: &str = "mtk1";
const KEYWORD: &str = "mtk6";
const NUMBER: &str = "mtk7";
const COMMENT: &str = "mtk8";
const STRING: &str = "mtk9";
const TYPE: &str = "mtk10";
const FUNCTION: &str = "mtk11";
const CONTROL: &str = "mtk12";

const KEYWORDS: &[&str] = &[
    "as", "const", "crate", "dyn", "enum", "extern", "false", "fn", "impl", "in", "let", "mod",
    "move", "mut", "pub", "ref", "self", "Self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where",
];
const CONTROL_KEYWORDS: &[&str] = &[
    "break", "continue", "else", "for", "if", "loop", "match", "return", "while",
];

/// Splits one line into tokens: a class and the text it covers. Line comments, string and
/// char literals, numbers, keywords, capitalised names (types), names followed by `(` or
/// `!` (calls and macros), and everything else as plain text. Runs of the same class are
/// merged, so a line has about as many spans as Monaco gives it.
pub fn tokenize(line: &str) -> Vec<(&'static str, String)> {
    let mut tokens: Vec<(&'static str, String)> = Vec::new();
    let mut push = |class: &'static str, text: &str| {
        if text.is_empty() {
            return;
        }
        match tokens.last_mut() {
            Some((last, run)) if *last == class => run.push_str(text),
            _ => tokens.push((class, text.to_string())),
        }
    };
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &line[i..];
        let b = bytes[i];
        if rest.starts_with("//") {
            push(COMMENT, rest);
            break;
        }
        if b == b'"' || b == b'\'' {
            let mut end = i + 1;
            while end < bytes.len() && bytes[end] != b {
                if bytes[end] == b'\\' {
                    end += 1;
                }
                end += 1;
            }
            let end = (end + 1).min(bytes.len());
            push(STRING, &line[i..end]);
            i = end;
            continue;
        }
        if b.is_ascii_digit() {
            let mut end = i;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
            {
                end += 1;
            }
            push(NUMBER, &line[i..end]);
            i = end;
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let mut end = i;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
            {
                end += 1;
            }
            let word = &line[i..end];
            let next = bytes.get(end).copied();
            let class = if KEYWORDS.contains(&word) {
                KEYWORD
            } else if CONTROL_KEYWORDS.contains(&word) {
                CONTROL
            } else if next == Some(b'(') || next == Some(b'!') {
                FUNCTION
            } else if word.as_bytes()[0].is_ascii_uppercase() {
                TYPE
            } else {
                PLAIN
            };
            push(class, word);
            i = end;
            continue;
        }
        // Anything else up to the next character that may start a token. Non-ASCII text
        // is carried whole, so a slice never splits a character.
        let mut end = i + rest.chars().next().map_or(1, char::len_utf8);
        while end < bytes.len() {
            let c = bytes[end];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'"' || c == b'\'' || c == b'/' {
                break;
            }
            end += line[end..].chars().next().map_or(1, char::len_utf8);
        }
        push(PLAIN, &line[i..end]);
        i = end;
    }
    tokens
}

/// One line in the DOM.
struct LineView {
    index: usize,
    top: String,
    number: String,
    current: bool,
    tokens: Vec<(&'static str, String)>,
    /// The selected stretch of the line, as `left` and `width`, when part of it is.
    selection: Option<String>,
}

fn px(value: f32) -> String {
    // Two decimals is finer than a device pixel at any zoom and keeps the strings short.
    format!("{:.2}px", value)
}

pub fn app() -> Element {
    let mut editor = use_signal(|| Editor::new(source()));
    EDITOR.with(|slot| *slot.borrow_mut() = Some(editor));
    let Options {
        viewport: (width, height),
        scroll_mode,
    } = options();
    let text_width = (width - GUTTER_WIDTH).max(0.0);

    let state = editor.read();
    let scroll = state.scroll_top;
    let delta = (scroll / BIG_NUMBERS_BLOCK).floor() * BIG_NUMBERS_BLOCK;
    let first = ((scroll / LINE_HEIGHT).floor() as usize).saturating_sub(OVERSCAN);
    let in_view = (height / LINE_HEIGHT).ceil() as usize + 1;
    let last = (first + in_view + 2 * OVERSCAN).min(state.lines.len());
    let cursor = state.cursor;
    let selection = state.anchor.filter(|&anchor| anchor != cursor).map(|anchor| {
        if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        }
    });
    let lines: Vec<LineView> = (first..last)
        .map(|index| {
            let text = &state.lines[index];
            let selected = selection.and_then(|(start, end)| {
                if index < start.line || index > end.line {
                    return None;
                }
                let from = if index == start.line { start.column } else { 0 };
                let to = if index == end.line {
                    end.column
                } else {
                    text.chars().count() + 1
                };
                Some(format!(
                    "left: {}; width: {}; height: {}",
                    px(from as f32 * CHAR_WIDTH),
                    px((to.saturating_sub(from)) as f32 * CHAR_WIDTH),
                    px(LINE_HEIGHT)
                ))
            });
            LineView {
                index,
                top: format!(
                    "top: {}; height: {}",
                    px(index as f32 * LINE_HEIGHT - delta),
                    px(LINE_HEIGHT)
                ),
                number: (index + 1).to_string(),
                current: index == cursor.line,
                tokens: tokenize(text),
                selection: selected,
            }
        })
        .collect();
    let content_height = state.content_height();
    let cursor_in_view = (first..last).contains(&cursor.line);
    let max_scroll = state.max_scroll(height);
    drop(state);

    let offset = match scroll_mode {
        ScrollMode::Top => format!("top: {}", px(delta - scroll)),
        ScrollMode::Transform => format!("transform: translateY({})", px(delta - scroll)),
    };
    let lines_content = format!("{offset}; width: {}", px(text_width));
    let margin_overlays = offset.clone();
    let margin = format!("width: {}", px(GUTTER_WIDTH));
    let scrollable = format!("left: {}; width: {}", px(GUTTER_WIDTH), px(text_width));
    let cursor_top = cursor.line as f32 * LINE_HEIGHT;
    let cursor_left = cursor.column as f32 * CHAR_WIDTH;
    let cursor_style = format!(
        "top: {}; left: {}; width: 2px; height: {}",
        px(cursor_top - delta),
        px(cursor_left),
        px(LINE_HEIGHT)
    );
    // The textarea sits on the cursor, in the overflow guard's own coordinates. Above the
    // lines rather than Monaco's `z-index: -10`, so a click on the cursor reaches it: the
    // page cannot move the focus into a field from a handler.
    // A cursor out of view leaves the textarea at the nearest edge, as Monaco's does.
    let input_style = format!(
        "top: {}; left: {}; width: 2px; height: {}; z-index: 10",
        px((cursor_top - scroll).clamp(0.0, (height - LINE_HEIGHT).max(0.0))),
        px(GUTTER_WIDTH + cursor_left),
        px(LINE_HEIGHT)
    );
    let slider_height = if content_height > 0.0 {
        (height * height / content_height).clamp(MIN_SLIDER, height)
    } else {
        height
    };
    let slider_top = if max_scroll > 0.0 {
        scroll / max_scroll * (height - slider_height)
    } else {
        0.0
    };
    let slider = format!("top: {}; height: {}", px(slider_top), px(slider_height));

    rsx! {
        div { id: "editor", class: "monaco-editor no-user-select mac vs-dark",
            div { class: "overflow-guard",
                div { class: "margin", style: "{margin}",
                    div { class: "margin-view-overlays", style: "{margin_overlays}",
                        for line in lines.iter() {
                            div { key: "{line.index}", style: "{line.top}",
                                if line.current {
                                    div { class: "current-line current-line-margin" }
                                }
                                div {
                                    class: if line.current { "line-numbers active-line-number" } else { "line-numbers" },
                                    "{line.number}"
                                }
                            }
                        }
                    }
                }
                div {
                    id: "scrollable",
                    class: "monaco-scrollable-element editor-scrollable vs-dark mac",
                    style: "{scrollable}",
                    onwheel: move |event| {
                        let delta = event.delta().strip_units();
                        editor.write().scroll_by(delta.y as f32, height);
                        event.prevent_default();
                    },
                    onclick: move |event| {
                        let at = event.element_coordinates();
                        let mut editor = editor.write();
                        let line = ((at.y as f32 + editor.scroll_top) / LINE_HEIGHT).max(0.0) as usize;
                        let column = (at.x as f32 / CHAR_WIDTH).round().max(0.0) as usize;
                        editor.jump_to(line, column, height);
                    },
                    div { class: "lines-content monaco-editor-background", style: "{lines_content}",
                        div { class: "view-overlays focused",
                            for line in lines.iter() {
                                div { key: "{line.index}", style: "{line.top}",
                                    if line.current {
                                        div { class: "current-line" }
                                    }
                                    for selected in line.selection.iter() {
                                        div { class: "cslr selected-text", style: "{selected}" }
                                    }
                                }
                            }
                        }
                        div { class: "view-lines monaco-mouse-cursor-text",
                            for line in lines.iter() {
                                div { key: "{line.index}", class: "view-line", style: "{line.top}",
                                    span {
                                        for (class, text) in line.tokens.iter() {
                                            span { class: "{class}", "{text}" }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "cursors-layer cursor-line-style",
                            if cursor_in_view {
                                div { class: "cursor", style: "{cursor_style}" }
                            }
                        }
                    }
                    div { class: "scrollbar vertical",
                        div { class: "slider", style: "{slider}" }
                    }
                }
                textarea {
                    id: "inputarea",
                    class: "inputarea monaco-mouse-cursor-text",
                    style: "{input_style}",
                    oninput: move |event| {
                        let value = event.value();
                        editor.write().accept_typed(&value, height);
                    },
                }
            }
        }
    }
}

/// The page's configuration: the stylesheet, and the renderer's text measurement (the
/// default).
pub fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![STYLE.to_string()],
        ..HtmlConfig::default()
    }
}

/// Percentiles and the rest over a set of durations in nanoseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub count: usize,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    pub mean: f64,
}

impl Stats {
    /// Over `samples` in nanoseconds; the fields are in milliseconds.
    pub fn of(samples: &[u64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let at = |q: f64| {
            let rank = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
            sorted[rank - 1] as f64 / 1e6
        };
        let total: u128 = sorted.iter().map(|&s| u128::from(s)).sum();
        Self {
            count: sorted.len(),
            p50: at(0.50),
            p95: at(0.95),
            max: *sorted.last().unwrap() as f64 / 1e6,
            mean: total as f64 / sorted.len() as f64 / 1e6,
        }
    }

    /// `p50 / p95 / max (n)` in milliseconds, for a table cell.
    pub fn cell(&self) -> String {
        if self.count == 0 {
            return "-".to_string();
        }
        format!(
            "{:.2} / {:.2} / {:.2} (n={})",
            self.p50, self.p95, self.max, self.count
        )
    }
}

/// The same for counts (records, bytes), which are not durations.
pub fn count_cell(samples: &[u64]) -> String {
    if samples.is_empty() {
        return "-".to_string();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let at = |q: f64| {
        let rank = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
        sorted[rank - 1]
    };
    format!(
        "{} / {} / {}",
        at(0.50),
        at(0.95),
        sorted.last().copied().unwrap_or(0)
    )
}

/// Reads the file named by `path` as lines, or generates `generated` lines when there is
/// no path. Returns the lines and a label for the tables.
pub fn load(path: Option<&str>, generated: usize) -> (Vec<String>, String) {
    match path {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
            let lines: Vec<String> = text.lines().map(str::to_string).collect();
            let name = std::path::Path::new(path)
                .file_name()
                .map_or_else(|| path.to_string(), |name| name.to_string_lossy().into_owned());
            let label = format!("{name} ({} lines)", lines.len());
            (lines, label)
        }
        None => (
            generate(generated),
            format!("generated ({generated} lines)"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_covers_the_whole_line() {
        for line in generate(26) {
            let joined: String = tokenize(&line).into_iter().map(|(_, text)| text).collect();
            assert_eq!(joined, line);
        }
    }

    #[test]
    fn typing_and_deleting_round_trip() {
        let mut editor = Editor::new(vec!["fn main() {}".to_string()]);
        editor.jump_to(0, 3, 800.0);
        editor.accept_typed("ab", 800.0);
        assert_eq!(editor.lines[0], "fn abmain() {}");
        editor.accept_typed("ab\n", 800.0);
        assert_eq!(editor.lines, vec!["fn ab", "main() {}"]);
        editor.accept_typed("a", 800.0);
        assert_eq!(editor.lines, vec!["fn amain() {}"]);
    }
}
