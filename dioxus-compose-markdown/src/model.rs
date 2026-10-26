//! What a document says, with nothing in it that belongs to a screen.
//!
//! The parser produces these and the `Markdown` component turns them into widgets. Nothing
//! here holds a handler, a signal or a node id, which is what lets a network worker parse
//! a reply and hand the result to the UI thread: every type is `Send` and `Sync`.

use std::sync::Arc;

/// How a run of text is set, apart from where it links to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    /// Inline code: set in the monospace face.
    pub code: bool,
}

impl Style {
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

/// A run of one treatment inside an [`Inline`], in bytes of its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub start: u32,
    pub len: u32,
    pub style: Style,
    /// Which of the inline's [`Inline::links`] this run is, if it is a link.
    pub link: Option<u32>,
}

/// An image, kept as the words that stand in for it.
///
/// `start..start + len` is the stand-in in the inline's text: the description, then the
/// address. An application that supplies an image resolver gets the picture in that place
/// instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineImage {
    pub start: u32,
    pub len: u32,
    pub url: String,
    pub alt: String,
}

/// One string with its runs: a paragraph, a heading, a table cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inline {
    pub text: String,
    pub runs: Vec<Run>,
    /// Every address a run links to, in the order they appear. A reference link holds
    /// the address its definition gave, already resolved.
    pub links: Vec<String>,
    pub images: Vec<InlineImage>,
}

impl Inline {
    /// The part of this inline between two byte offsets, with its runs cut to fit.
    pub fn slice(&self, start: usize, end: usize) -> Inline {
        let start = start.min(self.text.len());
        let end = end.clamp(start, self.text.len());
        let mut runs = Vec::new();
        for run in &self.runs {
            let run_start = run.start as usize;
            let run_end = run_start + run.len as usize;
            let from = run_start.max(start);
            let to = run_end.min(end);
            if from < to {
                runs.push(Run {
                    start: (from - start) as u32,
                    len: (to - from) as u32,
                    style: run.style,
                    link: run.link,
                });
            }
        }
        Inline {
            text: self.text[start..end].to_owned(),
            runs,
            links: self.links.clone(),
            images: Vec::new(),
        }
    }
}

/// How a table column lines its cells up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnAlign {
    None,
    Left,
    Center,
    Right,
}

/// A fenced or indented code block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Code {
    /// The first word of the fence's info string, exactly as written. `None` for an
    /// indented block and for a fence that names nothing.
    pub language: Option<String>,
    /// What lies between the fences, as CommonMark defines it, final newline included.
    pub text: String,
}

/// One item of a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// `Some` for a task list item, saying whether it is ticked.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

/// One block of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        content: Inline,
    },
    Paragraph(Inline),
    Quote(Vec<Block>),
    List {
        /// The number the first item carries, for an ordered list.
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Rule,
    Code(Code),
    Table {
        alignments: Vec<ColumnAlign>,
        head: Vec<Inline>,
        rows: Vec<Vec<Inline>>,
    },
    /// Raw HTML, kept as the characters that were written and never interpreted.
    Html(String),
}

impl Block {
    /// How many code blocks this block holds, itself included.
    pub fn code_blocks(&self) -> usize {
        match self {
            Block::Code(_) => 1,
            Block::Quote(blocks) => blocks.iter().map(Block::code_blocks).sum(),
            Block::List { items, .. } => items
                .iter()
                .flat_map(|item| item.blocks.iter())
                .map(Block::code_blocks)
                .sum(),
            _ => 0,
        }
    }
}

/// A block as the document holds it: shared, so a block that has not changed is the same
/// value from one delta to the next and compares equal without looking inside.
#[derive(Clone, Debug)]
pub struct BlockRef {
    block: Arc<Block>,
    /// How many code blocks come before this one in the document. A code block's place in
    /// that count is the name its fold state is kept under.
    first_code: usize,
}

impl BlockRef {
    pub(crate) fn new(block: Block, first_code: usize) -> Self {
        Self {
            block: Arc::new(block),
            first_code,
        }
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    /// The number of code blocks before this block in its document.
    pub fn first_code(&self) -> usize {
        self.first_code
    }

    /// Whether the two are the same shared value, without comparing contents.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.block, &other.block) && self.first_code == other.first_code
    }
}

impl PartialEq for BlockRef {
    fn eq(&self, other: &Self) -> bool {
        self.first_code == other.first_code
            && (Arc::ptr_eq(&self.block, &other.block) || *self.block == *other.block)
    }
}

/// Assigns each block the number of code blocks before it.
pub(crate) fn number_blocks(blocks: Vec<Block>, mut first_code: usize) -> Vec<BlockRef> {
    blocks
        .into_iter()
        .map(|block| {
            let count = block.code_blocks();
            let numbered = BlockRef::new(block, first_code);
            first_code += count;
            numbered
        })
        .collect()
}
