//! From markdown text to [`Block`]s.
//!
//! pulldown-cmark does the reading. This module turns its events into the neutral block
//! list, and it does so in one pass with an explicit stack, never by recursion: the input
//! is an agent's output and cannot be trusted to be shallow, so ten thousand nested quotes
//! must cost ten thousand steps and not ten thousand stack frames.

use crate::model::{Block, Code, ColumnAlign, Inline, InlineImage, ListItem, Run, Style};
use pulldown_cmark::{Alignment, BrokenLink, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::ops::Range;

/// How deep quotes and lists may nest, counted together, before deeper content is shown
/// flat inside the deepest level.
pub const DEFAULT_MAX_DEPTH: usize = 16;

/// CommonMark plus the three GitHub extensions the crate draws: tables, task lists and
/// strikethrough.
fn parser_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

/// One top-level block of the source, with the bytes it came from.
///
/// Usually one block. A document nested deeper than the cap allows at the very top can
/// flatten a single top-level quote into several blocks, and they share the range.
#[derive(Debug)]
pub(crate) struct Unit {
    pub range: Range<usize>,
    pub blocks: Vec<Block>,
}

/// A link reference definition, `[label]: destination`.
#[derive(Debug)]
pub(crate) struct Definition {
    /// The label, normalised the way CommonMark matches labels.
    pub label: String,
    pub destination: String,
    pub span: Range<usize>,
}

/// What one parse of a piece of source produced.
#[derive(Debug, Default)]
pub(crate) struct Region {
    pub units: Vec<Unit>,
    pub definitions: Vec<Definition>,
}

/// CommonMark matches reference labels case-insensitively and with runs of white space
/// collapsed to one space.
pub(crate) fn normalize_label(label: &str) -> String {
    let mut normalized = String::with_capacity(label.len());
    for word in label.split_whitespace() {
        if !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.extend(word.chars().flat_map(char::to_lowercase));
    }
    normalized
}

/// Parses `text` as a whole document.
///
/// `known` holds reference definitions that appeared before `text` began, for a parse
/// that starts part way into a document. A reference `text` cannot resolve on its own is
/// looked up there.
pub(crate) fn parse_region(
    text: &str,
    max_depth: usize,
    known: &HashMap<String, String>,
) -> Region {
    let callback = |link: BrokenLink| {
        known
            .get(&normalize_label(link.reference.as_ref()))
            .map(|destination| (destination.clone().into(), "".into()))
    };
    let mut events =
        Parser::new_with_broken_link_callback(text, parser_options(), Some(callback))
            .into_offset_iter();
    let mut converter = Converter::new(max_depth);
    for (event, range) in events.by_ref() {
        converter.event(event, range);
    }
    let mut units = converter.finish(text.len());
    // A unit that produced nothing still marks where a block was, but nothing is drawn
    // for it, so it is dropped here rather than carried as an empty entry.
    units.retain(|unit| !unit.blocks.is_empty());
    let definitions = events
        .reference_definitions()
        .iter()
        .map(|(label, definition)| Definition {
            label: normalize_label(label),
            destination: definition.dest.to_string(),
            span: definition.span.clone(),
        })
        .collect();
    Region { units, definitions }
}

/// The blocks of a whole document, in order.
pub(crate) fn parse_blocks(text: &str, max_depth: usize) -> Vec<Block> {
    parse_region(text, max_depth, &HashMap::new())
        .units
        .into_iter()
        .flat_map(|unit| unit.blocks)
        .collect()
}

/// The kind of string an inline accumulator is building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineKind {
    Paragraph,
    /// Text that arrived without a paragraph around it, which is what a tight list item
    /// holds. It ends at the next block boundary.
    Implicit,
    Heading(u8),
    Cell,
}

struct InlineBuilder {
    kind: InlineKind,
    inline: Inline,
    bold: u32,
    italic: u32,
    strikethrough: u32,
    links: Vec<u32>,
    images: Vec<(usize, String)>,
}

impl InlineBuilder {
    fn new(kind: InlineKind) -> Self {
        Self {
            kind,
            inline: Inline::default(),
            bold: 0,
            italic: 0,
            strikethrough: 0,
            links: Vec::new(),
            images: Vec::new(),
        }
    }

    fn style(&self, code: bool) -> Style {
        Style {
            bold: self.bold > 0,
            italic: self.italic > 0,
            strikethrough: self.strikethrough > 0,
            code,
        }
    }

    fn push(&mut self, text: &str, code: bool) {
        let style = self.style(code);
        let link = self.links.last().copied();
        self.push_styled(text, style, link);
    }

    fn push_styled(&mut self, text: &str, style: Style, link: Option<u32>) {
        if text.is_empty() {
            return;
        }
        let start = self.inline.text.len() as u32;
        self.inline.text.push_str(text);
        if style.is_plain() && link.is_none() {
            return;
        }
        let len = text.len() as u32;
        if let Some(last) = self.inline.runs.last_mut() {
            if last.start + last.len == start && last.style == style && last.link == link {
                last.len += len;
                return;
            }
        }
        self.inline.runs.push(Run {
            start,
            len,
            style,
            link,
        });
    }

    fn open_link(&mut self, destination: &str) {
        let index = self.inline.links.len() as u32;
        self.inline.links.push(destination.to_owned());
        self.links.push(index);
    }

    fn close_link(&mut self) {
        self.links.pop();
    }

    fn open_image(&mut self, destination: &str) {
        self.images
            .push((self.inline.text.len(), destination.to_owned()));
    }

    /// An image becomes its description and its address, and the address is a link. The
    /// crate never fetches anything, so the words are what the reader gets unless the
    /// application resolves the image itself.
    fn close_image(&mut self) {
        let Some((start, url)) = self.images.pop() else {
            return;
        };
        let alt = self.inline.text[start..].to_owned();
        let index = self.inline.links.len() as u32;
        self.inline.links.push(url.clone());
        if alt.is_empty() {
            self.push_styled(&url, Style::default(), Some(index));
        } else {
            self.push_styled(" (", Style::default(), None);
            self.push_styled(&url, Style::default(), Some(index));
            self.push_styled(")", Style::default(), None);
        }
        let len = self.inline.text.len() - start;
        self.inline.images.push(InlineImage {
            start: start as u32,
            len: len as u32,
            url,
            alt,
        });
    }

    fn finish(mut self) -> Inline {
        apply_bare_links(&mut self.inline);
        self.inline
    }
}

/// Turns addresses written without angle brackets into links, the way GitHub does.
///
/// Only `http://` and `https://`, only outside code and existing links, and never right
/// after `](`, which is a link whose closing parenthesis has not arrived yet and must stay
/// the plain text it was written as.
fn apply_bare_links(inline: &mut Inline) {
    let mut found = Vec::new();
    let text = inline.text.as_str();
    let mut from = 0;
    while let Some(offset) = text[from..].find("http") {
        let start = from + offset;
        from = start + 4;
        let rest = &text[start..];
        let scheme = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            continue;
        };
        let preceded_by_word = text[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
        if preceded_by_word || text[..start].ends_with("](") {
            continue;
        }
        let mut end = start
            + rest
                .find(|character: char| character.is_whitespace() || character == '<')
                .unwrap_or(rest.len());
        // Trailing punctuation belongs to the sentence, not the address.
        loop {
            let Some(last) = text[..end].chars().next_back() else {
                break;
            };
            if end <= start + scheme {
                break;
            }
            let candidate = &text[start..end];
            let trim = match last {
                '?' | '!' | '.' | ',' | ':' | '*' | '_' | '~' | '\'' | '"' => true,
                ')' => candidate.matches(')').count() > candidate.matches('(').count(),
                _ => false,
            };
            if !trim {
                break;
            }
            end -= last.len_utf8();
        }
        if end <= start + scheme {
            continue;
        }
        let covered = inline.runs.iter().any(|run| {
            let run_start = run.start as usize;
            let run_end = run_start + run.len as usize;
            (run.link.is_some() || run.style.code) && run_start < end && start < run_end
        });
        if !covered {
            found.push(start..end);
        }
        from = end.max(from);
    }
    for range in found {
        let index = inline.links.len() as u32;
        inline.links.push(inline.text[range.clone()].to_owned());
        inline.runs = link_range(&inline.runs, range, index);
    }
}

/// The runs with every byte of `range` linked to `index`, splitting runs at its edges.
fn link_range(runs: &[Run], range: Range<usize>, index: u32) -> Vec<Run> {
    let (start, end) = (range.start as u32, range.end as u32);
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut cursor = start;
    for run in runs {
        let run_end = run.start + run.len;
        if run_end <= start || run.start >= end {
            if run.start >= end && cursor < end {
                out.push(Run {
                    start: cursor,
                    len: end - cursor,
                    style: Style::default(),
                    link: Some(index),
                });
                cursor = end;
            }
            out.push(run.clone());
            continue;
        }
        if run.start < start {
            out.push(Run {
                start: run.start,
                len: start - run.start,
                style: run.style,
                link: run.link,
            });
        }
        let inner_start = run.start.max(start);
        if cursor < inner_start {
            out.push(Run {
                start: cursor,
                len: inner_start - cursor,
                style: Style::default(),
                link: Some(index),
            });
        }
        let inner_end = run_end.min(end);
        out.push(Run {
            start: inner_start,
            len: inner_end - inner_start,
            style: run.style,
            link: Some(index),
        });
        cursor = inner_end;
        if run_end > end {
            out.push(Run {
                start: end,
                len: run_end - end,
                style: run.style,
                link: run.link,
            });
        }
    }
    if cursor < end {
        out.push(Run {
            start: cursor,
            len: end - cursor,
            style: Style::default(),
            link: Some(index),
        });
    }
    out
}

struct Table {
    alignments: Vec<ColumnAlign>,
    head: Vec<Inline>,
    rows: Vec<Vec<Inline>>,
    row: Vec<Inline>,
}

enum Frame {
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Item(ListItem),
    Table(Table),
}

struct Converter {
    max_depth: usize,
    frames: Vec<Frame>,
    /// One entry per quote, list and item start still open: whether it got a frame of its
    /// own, or was flattened into its parent because the nesting cap was reached.
    opened: Vec<bool>,
    /// How many quote and list frames are open.
    depth: usize,
    inline: Option<InlineBuilder>,
    code: Option<Code>,
    html: Option<String>,
    /// Blocks finished at the top level since the current unit began.
    out: Vec<Block>,
    units: Vec<Unit>,
    open_tags: usize,
    unit_start: usize,
}

impl Converter {
    fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            frames: Vec::new(),
            opened: Vec::new(),
            depth: 0,
            inline: None,
            code: None,
            html: None,
            out: Vec::new(),
            units: Vec::new(),
            open_tags: 0,
            unit_start: 0,
        }
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        match event {
            Event::Start(tag) => {
                if self.open_tags == 0 {
                    self.unit_start = range.start;
                }
                self.open_tags += 1;
                self.start(tag);
            }
            Event::End(tag) => {
                self.end(tag);
                self.open_tags = self.open_tags.saturating_sub(1);
                if self.open_tags == 0 {
                    self.close_unit(range.end);
                }
            }
            Event::Text(text) => {
                if let Some(code) = self.code.as_mut() {
                    code.text.push_str(&text);
                } else if let Some(html) = self.html.as_mut() {
                    html.push_str(&text);
                } else {
                    self.inline().push(&text, false);
                }
            }
            Event::Code(text) => self.inline().push(&text, true),
            Event::Html(text) => {
                if let Some(html) = self.html.as_mut() {
                    html.push_str(&text);
                } else {
                    self.inline().push(&text, false);
                }
            }
            Event::InlineHtml(text) | Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.inline().push(&text, false);
            }
            Event::FootnoteReference(label) => {
                let inline = self.inline();
                inline.push("[^", false);
                inline.push(&label, false);
                inline.push("]", false);
            }
            Event::SoftBreak => self.inline().push(" ", false),
            Event::HardBreak => self.inline().push("\n", false),
            Event::Rule => {
                if self.open_tags == 0 {
                    self.unit_start = range.start;
                }
                self.flush_implicit();
                self.push_block(Block::Rule);
                if self.open_tags == 0 {
                    self.close_unit(range.end);
                }
            }
            Event::TaskListMarker(checked) => match self.frames.last_mut() {
                Some(Frame::Item(item)) if self.inline.is_none() => item.task = Some(checked),
                Some(Frame::Item(item))
                    if self.inline.as_ref().is_some_and(|inline| {
                        inline.inline.text.is_empty() && inline.kind == InlineKind::Paragraph
                    }) =>
                {
                    item.task = Some(checked)
                }
                // An item flattened by the nesting cap has no checkbox to show, so the
                // marker is shown as the characters that were written.
                _ => self
                    .inline()
                    .push(if checked { "[x] " } else { "[ ] " }, false),
            },
        }
    }

    fn close_unit(&mut self, end: usize) {
        self.flush_implicit();
        let blocks = std::mem::take(&mut self.out);
        self.units.push(Unit {
            range: self.unit_start..end,
            blocks,
        });
    }

    fn finish(mut self, end: usize) -> Vec<Unit> {
        // Events always balance, but the input is untrusted and a parser bug must not
        // become a panic here: anything still open is closed where the text ends.
        if let Some(code) = self.code.take() {
            self.push_block(Block::Code(code));
        }
        if let Some(html) = self.html.take() {
            self.push_block(Block::Html(html));
        }
        self.flush_inline();
        while let Some(frame) = self.frames.pop() {
            let block = match frame {
                Frame::Quote(blocks) => Some(Block::Quote(blocks)),
                Frame::List { start, items } => Some(Block::List { start, items }),
                Frame::Item(item) => {
                    self.attach_item(item);
                    None
                }
                Frame::Table(table) => Some(Block::Table {
                    alignments: table.alignments,
                    head: table.head,
                    rows: table.rows,
                }),
            };
            if let Some(block) = block {
                self.push_block(block);
            }
        }
        if !self.out.is_empty() {
            let blocks = std::mem::take(&mut self.out);
            self.units.push(Unit {
                range: self.unit_start..end,
                blocks,
            });
        }
        self.units
    }

    fn inline(&mut self) -> &mut InlineBuilder {
        self.inline
            .get_or_insert_with(|| InlineBuilder::new(InlineKind::Implicit))
    }

    fn begin_inline(&mut self, kind: InlineKind) {
        self.flush_implicit();
        self.inline = Some(InlineBuilder::new(kind));
    }

    /// Ends text that arrived with no paragraph around it, as a paragraph.
    fn flush_implicit(&mut self) {
        if self
            .inline
            .as_ref()
            .is_some_and(|inline| inline.kind == InlineKind::Implicit)
        {
            self.flush_inline();
        }
    }

    /// Ends whatever inline is being built and places it where its kind belongs.
    fn flush_inline(&mut self) {
        let Some(builder) = self.inline.take() else {
            return;
        };
        let kind = builder.kind;
        let inline = builder.finish();
        match kind {
            InlineKind::Paragraph | InlineKind::Implicit => {
                self.push_block(Block::Paragraph(inline));
            }
            InlineKind::Heading(level) => self.push_block(Block::Heading {
                level,
                content: inline,
            }),
            InlineKind::Cell => {
                if let Some(Frame::Table(table)) = self.frames.last_mut() {
                    table.row.push(inline);
                }
            }
        }
    }

    /// Places a finished block in the innermost container that holds blocks.
    fn push_block(&mut self, block: Block) {
        match self.frames.last_mut() {
            Some(Frame::Quote(blocks)) => blocks.push(block),
            Some(Frame::Item(item)) => item.blocks.push(block),
            Some(Frame::List { items, .. }) => match items.last_mut() {
                Some(item) => item.blocks.push(block),
                None => items.push(ListItem {
                    task: None,
                    blocks: vec![block],
                }),
            },
            // A table holds cells, never blocks. Nothing a parser emits lands here, and
            // if something did, dropping it is safer than guessing where it goes.
            Some(Frame::Table(_)) => {}
            None => self.out.push(block),
        }
    }

    fn attach_item(&mut self, item: ListItem) {
        match self.frames.last_mut() {
            Some(Frame::List { items, .. }) => items.push(item),
            _ => {
                for block in item.blocks {
                    self.push_block(block);
                }
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.begin_inline(InlineKind::Paragraph),
            Tag::Heading { level, .. } => self.begin_inline(InlineKind::Heading(level as u8)),
            Tag::BlockQuote(_) => {
                self.flush_implicit();
                if self.depth < self.max_depth {
                    self.frames.push(Frame::Quote(Vec::new()));
                    self.depth += 1;
                    self.opened.push(true);
                } else {
                    self.opened.push(false);
                }
            }
            Tag::CodeBlock(kind) => {
                self.flush_implicit();
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().map(str::to_owned)
                    }
                    CodeBlockKind::Indented => None,
                };
                self.code = Some(Code {
                    language,
                    text: String::new(),
                });
            }
            Tag::HtmlBlock => {
                self.flush_implicit();
                self.html = Some(String::new());
            }
            Tag::List(start) => {
                self.flush_implicit();
                if self.depth < self.max_depth {
                    self.frames.push(Frame::List {
                        start,
                        items: Vec::new(),
                    });
                    self.depth += 1;
                    self.opened.push(true);
                } else {
                    self.opened.push(false);
                }
            }
            Tag::Item => {
                self.flush_implicit();
                let in_list = self.opened.last() == Some(&true)
                    && matches!(self.frames.last(), Some(Frame::List { .. }));
                if in_list {
                    self.frames.push(Frame::Item(ListItem {
                        task: None,
                        blocks: Vec::new(),
                    }));
                }
                self.opened.push(in_list);
            }
            Tag::Table(alignments) => {
                self.flush_implicit();
                self.frames.push(Frame::Table(Table {
                    alignments: alignments
                        .iter()
                        .map(|alignment| match alignment {
                            Alignment::None => ColumnAlign::None,
                            Alignment::Left => ColumnAlign::Left,
                            Alignment::Center => ColumnAlign::Center,
                            Alignment::Right => ColumnAlign::Right,
                        })
                        .collect(),
                    head: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                }));
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(Frame::Table(table)) = self.frames.last_mut() {
                    table.row.clear();
                }
            }
            Tag::TableCell => self.begin_inline(InlineKind::Cell),
            Tag::Emphasis => self.inline().italic += 1,
            Tag::Strong => self.inline().bold += 1,
            Tag::Strikethrough => self.inline().strikethrough += 1,
            Tag::Link { dest_url, .. } => self.inline().open_link(&dest_url),
            Tag::Image { dest_url, .. } => self.inline().open_image(&dest_url),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell => self.flush_inline(),
            TagEnd::BlockQuote(_) => {
                self.flush_implicit();
                if self.opened.pop() == Some(true) {
                    if let Some(Frame::Quote(blocks)) = self.frames.pop() {
                        self.depth = self.depth.saturating_sub(1);
                        self.push_block(Block::Quote(blocks));
                    }
                }
            }
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    self.push_block(Block::Code(code));
                }
            }
            TagEnd::HtmlBlock => {
                if let Some(mut html) = self.html.take() {
                    let kept = html.trim_end_matches(['\n', '\r']).len();
                    html.truncate(kept);
                    self.push_block(Block::Html(html));
                }
            }
            TagEnd::List(_) => {
                self.flush_implicit();
                if self.opened.pop() == Some(true) {
                    if let Some(Frame::List { start, items }) = self.frames.pop() {
                        self.depth = self.depth.saturating_sub(1);
                        self.push_block(Block::List { start, items });
                    }
                }
            }
            TagEnd::Item => {
                self.flush_implicit();
                if self.opened.pop() == Some(true) {
                    if let Some(Frame::Item(item)) = self.frames.pop() {
                        self.attach_item(item);
                    }
                }
            }
            TagEnd::Table => {
                self.flush_inline();
                if let Some(Frame::Table(table)) = self.frames.pop() {
                    self.push_block(Block::Table {
                        alignments: table.alignments,
                        head: table.head,
                        rows: table.rows,
                    });
                }
            }
            TagEnd::TableHead => {
                if let Some(Frame::Table(table)) = self.frames.last_mut() {
                    table.head = std::mem::take(&mut table.row);
                }
            }
            TagEnd::TableRow => {
                if let Some(Frame::Table(table)) = self.frames.last_mut() {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
            TagEnd::Emphasis => {
                if let Some(inline) = self.inline.as_mut() {
                    inline.italic = inline.italic.saturating_sub(1);
                }
            }
            TagEnd::Strong => {
                if let Some(inline) = self.inline.as_mut() {
                    inline.bold = inline.bold.saturating_sub(1);
                }
            }
            TagEnd::Strikethrough => {
                if let Some(inline) = self.inline.as_mut() {
                    inline.strikethrough = inline.strikethrough.saturating_sub(1);
                }
            }
            TagEnd::Link => {
                if let Some(inline) = self.inline.as_mut() {
                    inline.close_link();
                }
            }
            TagEnd::Image => {
                if let Some(inline) = self.inline.as_mut() {
                    inline.close_image();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_addresses_become_links_and_keep_their_sentence_punctuation_out() {
        let blocks = parse_blocks("see https://example.com/a_(b). and more", 16);
        let Block::Paragraph(inline) = &blocks[0] else {
            panic!("not a paragraph: {blocks:?}");
        };
        assert_eq!(inline.links, ["https://example.com/a_(b)"]);
        let run = &inline.runs[0];
        assert_eq!(
            &inline.text[run.start as usize..(run.start + run.len) as usize],
            "https://example.com/a_(b)"
        );
    }

    #[test]
    fn an_unclosed_link_stays_plain_text() {
        let blocks = parse_blocks("[text](https://exa", 16);
        let Block::Paragraph(inline) = &blocks[0] else {
            panic!("not a paragraph: {blocks:?}");
        };
        assert_eq!(inline.text, "[text](https://exa");
        assert!(inline.runs.is_empty(), "{:?}", inline.runs);
    }
}
