//! A document that arrives a piece at a time.
//!
//! The text grows at the end, and almost all of a document is settled long before it is
//! finished: a paragraph that has been followed by another block can never change again,
//! and finished items of a long list or finished rows of a long table can never change
//! again either. So a block (or list item, or table row) is frozen the moment the next one
//! settles, and only the open tail after the last frozen piece is parsed again when a delta
//! lands. The cost of a delta is the size of the open piece it lands in, not the size of
//! the message.

use crate::model::{Block, BlockRef, ColumnAlign, Inline, ListItem, number_blocks};
use crate::options::MarkdownOptions;
use crate::parse::{Definition, Unit, parse_blocks, parse_region};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// How many blocks (or list items, or table rows) one chunk component draws.
pub(crate) const CHUNK: usize = 8;
/// How many chunks one group component holds.
pub(crate) const GROUP: usize = 8;

/// Which part of the drawn tree a watcher stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WatchKey {
    Root,
    Group(usize),
    Chunk(usize),
    SubRoot(usize),
    SubGroup(usize, usize),
    SubChunk(usize, usize),
    /// One list item or table row: the block, then the item's position in it.
    SubItem(usize, usize),
}

/// A shared list item, compared by pointer identity first so frozen items never re-render
/// when later items grow.
#[derive(Clone, Debug)]
pub(crate) struct ListItemRef {
    pub item: Arc<ListItem>,
    pub first_code: usize,
}

impl PartialEq for ListItemRef {
    fn eq(&self, other: &Self) -> bool {
        self.first_code == other.first_code
            && (Arc::ptr_eq(&self.item, &other.item) || *self.item == *other.item)
    }
}

impl ListItemRef {
    pub fn same(&self, other: &Self) -> bool {
        self.first_code == other.first_code && Arc::ptr_eq(&self.item, &other.item)
    }
}

/// Shared column alignments for a streamed table.
#[derive(Clone, Debug)]
pub(crate) struct AlignmentsRef(pub Arc<Vec<ColumnAlign>>);

impl PartialEq for AlignmentsRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

/// Shared header cells and alignments for a streamed table.
#[derive(Clone, Debug)]
pub(crate) struct TableHeaderRef {
    pub alignments: AlignmentsRef,
    pub head: Arc<Vec<Inline>>,
}

impl PartialEq for TableHeaderRef {
    fn eq(&self, other: &Self) -> bool {
        self.alignments == other.alignments
            && (Arc::ptr_eq(&self.head, &other.head) || *self.head == *other.head)
    }
}

/// A shared table row, compared by pointer identity first so frozen rows never re-render
/// when later rows grow.
#[derive(Clone, Debug)]
pub(crate) struct TableRowRef(pub Arc<Vec<Inline>>);

impl PartialEq for TableRowRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

impl TableRowRef {
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// How a top-level position in a document is drawn.
#[derive(Clone, PartialEq)]
pub(crate) enum StreamSlot {
    Block(BlockRef, bool),
    List { start: Option<u64> },
    Table { header: TableHeaderRef },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListMarker {
    Bullet(u8),
    Ordered(u8),
}

struct OpenList {
    marker: ListMarker,
    start: Option<u64>,
    first_code: usize,
    frozen_items: Vec<ListItemRef>,
    frozen_code: usize,
    open_items: Vec<ListItemRef>,
    materialized: OnceLock<BlockRef>,
}

impl OpenList {
    fn total_len(&self) -> usize {
        self.frozen_items.len() + self.open_items.len()
    }

    fn total_code(&self) -> usize {
        self.frozen_code
            + self
                .open_items
                .iter()
                .map(|item| item.item.code_blocks())
                .sum::<usize>()
    }

    fn item(&self, index: usize) -> Option<&ListItemRef> {
        if index < self.frozen_items.len() {
            self.frozen_items.get(index)
        } else {
            self.open_items.get(index - self.frozen_items.len())
        }
    }

    fn materialize(&self) -> &BlockRef {
        self.materialized.get_or_init(|| {
            let mut items = Vec::with_capacity(self.total_len());
            for item in self.frozen_items.iter().chain(self.open_items.iter()) {
                items.push((*item.item).clone());
            }
            BlockRef::new(
                Block::List {
                    start: self.start,
                    items,
                },
                self.first_code,
            )
        })
    }

    fn into_frozen(mut self) -> (BlockRef, FrozenItems, usize) {
        self.frozen_items.extend(self.open_items);
        let mut items = Vec::with_capacity(self.frozen_items.len());
        let mut code = 0;
        for item in &self.frozen_items {
            code += item.item.code_blocks();
            items.push((*item.item).clone());
        }
        let block = BlockRef::new(
            Block::List {
                start: self.start,
                items,
            },
            self.first_code,
        );
        let frozen_items = FrozenItems::List {
            start: self.start,
            items: self.frozen_items,
        };
        (block, frozen_items, code)
    }
}

struct OpenTable {
    first_code: usize,
    header: TableHeaderRef,
    synthetic_header: String,
    /// Whether `open_start` has already moved past the table's header and delimiter rows.
    past_header: bool,
    frozen_rows: Vec<TableRowRef>,
    open_rows: Vec<TableRowRef>,
    materialized: OnceLock<BlockRef>,
}

impl OpenTable {
    fn total_len(&self) -> usize {
        self.frozen_rows.len() + self.open_rows.len()
    }

    fn row(&self, index: usize) -> Option<&TableRowRef> {
        if index < self.frozen_rows.len() {
            self.frozen_rows.get(index)
        } else {
            self.open_rows.get(index - self.frozen_rows.len())
        }
    }

    fn materialize(&self) -> &BlockRef {
        self.materialized.get_or_init(|| {
            let mut rows = Vec::with_capacity(self.total_len());
            for row in self.frozen_rows.iter().chain(self.open_rows.iter()) {
                rows.push((*row.0).clone());
            }
            BlockRef::new(
                Block::Table {
                    alignments: (*self.header.alignments.0).clone(),
                    head: (*self.header.head).clone(),
                    rows,
                },
                self.first_code,
            )
        })
    }

    fn into_frozen(mut self) -> (BlockRef, FrozenItems) {
        self.frozen_rows.extend(self.open_rows);
        let mut rows = Vec::with_capacity(self.frozen_rows.len());
        for row in &self.frozen_rows {
            rows.push((*row.0).clone());
        }
        let block = BlockRef::new(
            Block::Table {
                alignments: (*self.header.alignments.0).clone(),
                head: (*self.header.head).clone(),
                rows,
            },
            self.first_code,
        );
        let frozen_items = FrozenItems::Table {
            header: self.header,
            rows: self.frozen_rows,
        };
        (block, frozen_items)
    }
}

enum OpenContainer {
    List(OpenList),
    Table(OpenTable),
}

impl OpenContainer {
    fn materialize(&self) -> &BlockRef {
        match self {
            OpenContainer::List(list) => list.materialize(),
            OpenContainer::Table(table) => table.materialize(),
        }
    }
}

enum FrozenItems {
    List {
        start: Option<u64>,
        items: Vec<ListItemRef>,
    },
    Table {
        header: TableHeaderRef,
        rows: Vec<TableRowRef>,
    },
}

type Updater = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Watchers {
    next: u64,
    entries: HashMap<WatchKey, Vec<(u64, Updater)>>,
}

/// Keeps one watcher registered for as long as it lives.
pub(crate) struct WatchGuard {
    watchers: Arc<Mutex<Watchers>>,
    key: WatchKey,
    token: u64,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        if let Ok(mut watchers) = self.watchers.lock()
            && let Some(list) = watchers.entries.get_mut(&self.key)
        {
            list.retain(|(token, _)| *token != self.token);
            if list.is_empty() {
                watchers.entries.remove(&self.key);
            }
        }
    }
}

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// A markdown document fed in pieces, typically from the thread a reply streams in on.
///
/// ```ignore
/// let stream = use_signal_sync(|| MarkdownStream::new(MarkdownOptions::default()));
/// // on the network worker:
/// stream.write().push_delta(&delta);
/// // when the reply is complete:
/// stream.write().finish();
/// // on screen:
/// rsx! { Markdown { stream, on_link: open_link } }
/// ```
///
/// It is `Send`, and the result it holds is a list of neutral blocks with no handler or
/// node in it, so the parse can happen wherever the text arrives. The `Markdown`
/// component attaches the handlers on the UI thread.
pub struct MarkdownStream {
    max_depth: usize,
    source: String,
    frozen: Vec<BlockRef>,
    /// Preserved shared items and rows for lists and tables that streamed incrementally
    /// before closing, so their components stay mounted with the same shared references.
    frozen_containers: HashMap<usize, FrozenItems>,
    /// The first open block when it is a list or table whose finished items or rows freeze
    /// incrementally. When `Some`, it occupies position `self.frozen.len()`.
    open_container: Option<OpenContainer>,
    open: Vec<BlockRef>,
    /// Where the text that is still parsed again begins.
    open_start: usize,
    /// How many code blocks the frozen blocks hold.
    frozen_code: usize,
    /// Reference definitions from the frozen part, keyed by normalised label. The first
    /// definition of a label wins, as CommonMark says.
    definitions: HashMap<String, String>,
    saw_definitions: bool,
    finished: bool,
    generation: u64,
    watchers: Arc<Mutex<Watchers>>,
}

impl std::fmt::Debug for MarkdownStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MarkdownStream")
            .field("bytes", &self.source.len())
            .field("frozen", &self.frozen.len())
            .field(
                "open",
                &(self.open.len() + usize::from(self.open_container.is_some())),
            )
            .field("finished", &self.finished)
            .finish()
    }
}

impl Default for MarkdownStream {
    fn default() -> Self {
        Self::new(MarkdownOptions::default())
    }
}

impl MarkdownStream {
    pub fn new(options: MarkdownOptions) -> Self {
        Self {
            max_depth: options.max_depth,
            source: String::new(),
            frozen: Vec::new(),
            frozen_containers: HashMap::new(),
            open_container: None,
            open: Vec::new(),
            open_start: 0,
            frozen_code: 0,
            definitions: HashMap::new(),
            saw_definitions: false,
            finished: false,
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
            watchers: Arc::new(Mutex::new(Watchers::default())),
        }
    }

    /// Adds text to the end of the document.
    ///
    /// Safe to call from any thread. A delta after [`finish`](Self::finish) is ignored:
    /// a finished document has no open block to put it in.
    pub fn push_delta(&mut self, delta: &str) {
        if self.finished || delta.is_empty() {
            return;
        }
        self.source.push_str(delta);
        let before = self.state();
        self.reparse(false);
        self.notify(before, None);
    }

    /// Says the document is complete. Every block is frozen after this.
    ///
    /// The one case where a frozen block is built again is here: a reference definition
    /// that appeared after a link that uses it. CommonMark resolves that link, the stream
    /// could not while the definition had not arrived, and this is where the two agree.
    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        let before = self.state();
        self.finished = true;
        self.reparse(true);
        let mut first_rebuilt = None;
        if self.saw_definitions {
            first_rebuilt = self.resolve_late_definitions();
        }
        self.notify(before, first_rebuilt);
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Everything pushed so far.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// How many blocks the document has now.
    pub fn len(&self) -> usize {
        self.frozen.len() + usize::from(self.open_container.is_some()) + self.open.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many blocks, counted from the start, can no longer change.
    pub fn frozen_len(&self) -> usize {
        self.frozen.len()
    }

    /// One block, by its position in the document.
    pub fn block(&self, index: usize) -> Option<&BlockRef> {
        if index < self.frozen.len() {
            return self.frozen.get(index);
        }
        let mut offset = index - self.frozen.len();
        if let Some(container) = &self.open_container {
            if offset == 0 {
                return Some(container.materialize());
            }
            offset -= 1;
        }
        self.open.get(offset)
    }

    /// Every block, frozen ones first.
    pub fn blocks(&self) -> impl Iterator<Item = &BlockRef> {
        self.frozen
            .iter()
            .chain(self.open_container.iter().map(OpenContainer::materialize))
            .chain(self.open.iter())
    }

    /// Whether the block at `index` may still change.
    pub fn is_open(&self, index: usize) -> bool {
        !self.finished && index >= self.frozen.len()
    }

    /// Different for every stream ever made, so a screen can tell that the stream it was
    /// drawing has been replaced by another.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn slot(&self, index: usize) -> Option<StreamSlot> {
        if index < self.frozen.len() {
            if let Some(container) = self.frozen_containers.get(&index) {
                return Some(match container {
                    FrozenItems::List { start, .. } => StreamSlot::List { start: *start },
                    FrozenItems::Table { header, .. } => StreamSlot::Table {
                        header: header.clone(),
                    },
                });
            }
            return Some(StreamSlot::Block(self.frozen[index].clone(), false));
        }
        let mut offset = index - self.frozen.len();
        if let Some(container) = &self.open_container {
            if offset == 0 {
                return Some(match container {
                    OpenContainer::List(list) => StreamSlot::List { start: list.start },
                    OpenContainer::Table(table) => StreamSlot::Table {
                        header: table.header.clone(),
                    },
                });
            }
            offset -= 1;
        }
        self.open
            .get(offset)
            .map(|block| StreamSlot::Block(block.clone(), !self.finished))
    }

    pub(crate) fn list_len(&self, index: usize) -> usize {
        if index < self.frozen.len() {
            match self.frozen_containers.get(&index) {
                Some(FrozenItems::List { items, .. }) => items.len(),
                _ => 0,
            }
        } else if index == self.frozen.len() {
            match &self.open_container {
                Some(OpenContainer::List(list)) => list.total_len(),
                _ => 0,
            }
        } else {
            0
        }
    }

    pub(crate) fn list_items(
        &self,
        index: usize,
        first: usize,
        count: usize,
    ) -> Vec<(usize, ListItemRef, bool)> {
        if index < self.frozen.len() {
            let Some(FrozenItems::List { items, .. }) = self.frozen_containers.get(&index) else {
                return Vec::new();
            };
            return items
                .iter()
                .enumerate()
                .skip(first)
                .take(count)
                .map(|(pos, item)| (pos, item.clone(), false))
                .collect();
        }
        if index == self.frozen.len()
            && let Some(OpenContainer::List(list)) = &self.open_container
        {
            return (first..first + count)
                .filter_map(|pos| {
                    list.item(pos).map(|item| {
                        (
                            pos,
                            item.clone(),
                            !self.finished && pos >= list.frozen_items.len(),
                        )
                    })
                })
                .collect();
        }
        Vec::new()
    }

    pub(crate) fn table_len(&self, index: usize) -> usize {
        if index < self.frozen.len() {
            match self.frozen_containers.get(&index) {
                Some(FrozenItems::Table { rows, .. }) => rows.len(),
                _ => 0,
            }
        } else if index == self.frozen.len() {
            match &self.open_container {
                Some(OpenContainer::Table(table)) => table.total_len(),
                _ => 0,
            }
        } else {
            0
        }
    }

    pub(crate) fn table_rows(
        &self,
        index: usize,
        first: usize,
        count: usize,
    ) -> Vec<(usize, TableRowRef)> {
        if index < self.frozen.len() {
            let Some(FrozenItems::Table { rows, .. }) = self.frozen_containers.get(&index) else {
                return Vec::new();
            };
            return rows
                .iter()
                .enumerate()
                .skip(first)
                .take(count)
                .map(|(pos, row)| (pos, row.clone()))
                .collect();
        }
        if index == self.frozen.len()
            && let Some(OpenContainer::Table(table)) = &self.open_container
        {
            return (first..first + count)
                .filter_map(|pos| table.row(pos).map(|row| (pos, row.clone())))
                .collect();
        }
        Vec::new()
    }

    pub(crate) fn watch(&self, key: WatchKey, updater: Updater) -> WatchGuard {
        let token = match self.watchers.lock() {
            Ok(mut watchers) => {
                watchers.next += 1;
                let token = watchers.next;
                watchers
                    .entries
                    .entry(key)
                    .or_default()
                    .push((token, updater));
                token
            }
            Err(_) => 0,
        };
        WatchGuard {
            watchers: self.watchers.clone(),
            key,
            token,
        }
    }

    fn state(&self) -> State {
        let container = self.open_container.as_ref().map(|active| match active {
            OpenContainer::List(list) => ContainerState::List {
                start: list.start,
                frozen_len: list.frozen_items.len(),
                open_items: list.open_items.clone(),
            },
            OpenContainer::Table(table) => ContainerState::Table {
                header: table.header.clone(),
                frozen_len: table.frozen_rows.len(),
                open_rows: table.open_rows.clone(),
            },
        });
        State {
            len: self.len(),
            frozen: self.frozen.len(),
            open: self.open.clone(),
            container,
        }
    }

    /// Parses the open part again. With `everything`, every block it holds is frozen.
    fn reparse(&mut self, everything: bool) {
        let active = self.open_container.take();
        let (combined, table_prefix_len) = match &active {
            Some(OpenContainer::Table(table)) if table.past_header => {
                let region_text = &self.source[self.open_start..];
                let mut buf =
                    String::with_capacity(table.synthetic_header.len() + region_text.len());
                buf.push_str(&table.synthetic_header);
                buf.push_str(region_text);
                (Some(buf), table.synthetic_header.len())
            }
            _ => (None, 0),
        };
        let parse_text: &str = combined
            .as_deref()
            .unwrap_or(&self.source[self.open_start..]);
        let region = parse_region(parse_text, self.max_depth, &self.definitions);
        if !region.definitions.is_empty() {
            self.saw_definitions = true;
        }
        let units = region.units;
        let definitions = region.definitions;
        let mut freeze = if everything {
            units.len()
        } else {
            settled_units(parse_text, &units)
        };
        let mut next_start = if freeze == 0 {
            0
        } else if freeze < units.len() {
            let after_last = next_line_start(parse_text, units[freeze - 1].range.end);
            after_last.min(line_start(parse_text, units[freeze].range.start))
        } else {
            parse_text.len()
        };
        // The text parsed again next time must begin after everything frozen now, or a
        // frozen block would be found a second time. The line arithmetic above guarantees
        // it for every input it was written for; this keeps the guarantee for the rest.
        if freeze > 0 && freeze < units.len() && next_start < units[freeze - 1].range.end {
            freeze = 0;
            next_start = 0;
        }

        let previous_open = std::mem::take(&mut self.open);

        match active {
            Some(OpenContainer::List(mut list)) if freeze == 0 => {
                let is_list = units.first().is_some_and(|u| {
                    u.blocks.len() == 1 && matches!(u.blocks[0], Block::List { .. })
                });
                if is_list {
                    let mut iter = units.into_iter();
                    let first_unit = iter.next().expect("checked non-empty");
                    let Some(Block::List { start, items }) = first_unit.blocks.into_iter().next()
                    else {
                        unreachable!();
                    };
                    let rest_units: Vec<Unit> = iter.collect();
                    if list.frozen_items.is_empty() {
                        let ls = line_start(parse_text, first_unit.range.start);
                        if let Some(marker) = list_marker_at(parse_text, ls) {
                            list.marker = marker;
                        }
                        list.start = start;
                    }
                    let prev_items = std::mem::take(&mut list.open_items);
                    let mut fresh_items =
                        number_list_items(items, list.first_code + list.frozen_code, &prev_items);
                    if rest_units.is_empty()
                        && first_unit.sub_ranges.len() == fresh_items.len()
                        && !has_bare_carriage_return(parse_text)
                    {
                        let (freeze_items, sub_next) =
                            settled_list_items(parse_text, &first_unit.sub_ranges, list.marker);
                        if freeze_items > 0 {
                            record_definitions(&mut self.definitions, &definitions, sub_next);
                            let open_items = fresh_items.split_off(freeze_items);
                            for item in &fresh_items {
                                list.frozen_code += item.item.code_blocks();
                            }
                            list.frozen_items.extend(fresh_items);
                            list.open_items = open_items;
                            self.open_start += sub_next;
                        } else {
                            list.open_items = fresh_items;
                        }
                    } else {
                        list.open_items = fresh_items;
                    }
                    list.materialized = OnceLock::new();
                    let after_code = list.first_code + list.total_code();
                    self.open_container = Some(OpenContainer::List(list));
                    self.open = finish_open_blocks(rest_units, after_code, &previous_open);
                    return;
                }
                if !list.frozen_items.is_empty() {
                    let (block, frozen_items, code) = list.into_frozen();
                    let pos = self.frozen.len();
                    self.frozen.push(block);
                    self.frozen_containers.insert(pos, frozen_items);
                    self.frozen_code += code;
                }
                self.open = finish_open_blocks(units, self.frozen_code, &previous_open);
                return;
            }
            Some(OpenContainer::Table(mut table)) if freeze == 0 => {
                let is_table = units.first().is_some_and(|u| {
                    u.blocks.len() == 1 && matches!(u.blocks[0], Block::Table { .. })
                });
                if is_table {
                    let mut iter = units.into_iter();
                    let first_unit = iter.next().expect("checked non-empty");
                    let Some(Block::Table {
                        alignments,
                        head,
                        rows,
                    }) = first_unit.blocks.into_iter().next()
                    else {
                        unreachable!();
                    };
                    let rest_units: Vec<Unit> = iter.collect();
                    if !table.past_header
                        && (*table.header.alignments.0 != alignments || *table.header.head != head)
                    {
                        table.synthetic_header = synthetic_table_header(&alignments, head.len());
                        if *table.header.alignments.0 != alignments {
                            table.header.alignments = AlignmentsRef(Arc::new(alignments));
                        }
                        if *table.header.head != head {
                            table.header.head = Arc::new(head);
                        }
                    }
                    let prev_rows = std::mem::take(&mut table.open_rows);
                    let mut fresh_rows = wrap_table_rows(rows, &prev_rows);
                    if rest_units.is_empty()
                        && first_unit.sub_ranges.len() == fresh_rows.len()
                        && !has_bare_carriage_return(parse_text)
                        && !table.header.head.is_empty()
                    {
                        let (freeze_rows, sub_next) =
                            settled_table_rows(parse_text, &first_unit.sub_ranges);
                        if freeze_rows > 0 && sub_next >= table_prefix_len {
                            record_definitions(&mut self.definitions, &definitions, sub_next);
                            let open_rows = fresh_rows.split_off(freeze_rows);
                            table.frozen_rows.extend(fresh_rows);
                            table.open_rows = open_rows;
                            table.past_header = true;
                            self.open_start += sub_next - table_prefix_len;
                        } else {
                            table.open_rows = fresh_rows;
                        }
                    } else {
                        table.open_rows = fresh_rows;
                    }
                    table.materialized = OnceLock::new();
                    let after_code = table.first_code;
                    self.open_container = Some(OpenContainer::Table(table));
                    self.open = finish_open_blocks(rest_units, after_code, &previous_open);
                    return;
                }
                if table.past_header {
                    let (block, frozen_items) = table.into_frozen();
                    let pos = self.frozen.len();
                    self.frozen.push(block);
                    self.frozen_containers.insert(pos, frozen_items);
                }
                self.open = finish_open_blocks(units, self.frozen_code, &previous_open);
                return;
            }
            Some(OpenContainer::List(mut list)) => {
                // freeze > 0: the active list is finishing.
                record_definitions(&mut self.definitions, &definitions, next_start);
                let mut iter = units.into_iter().enumerate();
                let mut settled_blocks = Vec::new();
                let mut open_units = Vec::new();
                if let Some((index, unit)) = iter.next() {
                    let is_list =
                        unit.blocks.len() == 1 && matches!(unit.blocks[0], Block::List { .. });
                    if is_list {
                        let Some(Block::List { start, items }) = unit.blocks.into_iter().next()
                        else {
                            unreachable!();
                        };
                        if list.frozen_items.is_empty() {
                            list.start = start;
                        }
                        let prev = std::mem::take(&mut list.open_items);
                        list.open_items =
                            number_list_items(items, list.first_code + list.frozen_code, &prev);
                        let (block, frozen_items, code) = list.into_frozen();
                        let pos = self.frozen.len();
                        self.frozen.push(block);
                        self.frozen_containers.insert(pos, frozen_items);
                        self.frozen_code += code;
                    } else {
                        if !list.frozen_items.is_empty() {
                            let (block, frozen_items, code) = list.into_frozen();
                            let pos = self.frozen.len();
                            self.frozen.push(block);
                            self.frozen_containers.insert(pos, frozen_items);
                            self.frozen_code += code;
                        }
                        if index < freeze {
                            settled_blocks.extend(unit.blocks);
                        } else {
                            open_units.push(unit);
                        }
                    }
                } else if !list.frozen_items.is_empty() {
                    let (block, frozen_items, code) = list.into_frozen();
                    let pos = self.frozen.len();
                    self.frozen.push(block);
                    self.frozen_containers.insert(pos, frozen_items);
                    self.frozen_code += code;
                }
                for (index, unit) in iter {
                    if index < freeze {
                        settled_blocks.extend(unit.blocks);
                    } else {
                        open_units.push(unit);
                    }
                }
                let settled_count = settled_blocks.len();
                self.freeze_plain_blocks(settled_blocks, &previous_open);
                self.open_start += next_start.saturating_sub(table_prefix_len);
                let remaining_prev = previous_open.get(settled_count..).unwrap_or(&[]);
                let after_open = self.open_start;
                self.install_open_units(
                    open_units,
                    everything,
                    next_start,
                    after_open,
                    &definitions,
                    remaining_prev,
                );
                return;
            }
            Some(OpenContainer::Table(mut table)) => {
                // freeze > 0: the active table is finishing.
                record_definitions(&mut self.definitions, &definitions, next_start);
                let mut iter = units.into_iter().enumerate();
                let mut settled_blocks = Vec::new();
                let mut open_units = Vec::new();
                if let Some((index, unit)) = iter.next() {
                    let is_table =
                        unit.blocks.len() == 1 && matches!(unit.blocks[0], Block::Table { .. });
                    if is_table {
                        let Some(Block::Table {
                            alignments,
                            head,
                            rows,
                        }) = unit.blocks.into_iter().next()
                        else {
                            unreachable!();
                        };
                        if !table.past_header {
                            if *table.header.alignments.0 != alignments {
                                table.header.alignments = AlignmentsRef(Arc::new(alignments));
                            }
                            if *table.header.head != head {
                                table.header.head = Arc::new(head);
                            }
                        }
                        let prev = std::mem::take(&mut table.open_rows);
                        table.open_rows = wrap_table_rows(rows, &prev);
                        let (block, frozen_items) = table.into_frozen();
                        let pos = self.frozen.len();
                        self.frozen.push(block);
                        self.frozen_containers.insert(pos, frozen_items);
                    } else {
                        if table.past_header {
                            let (block, frozen_items) = table.into_frozen();
                            let pos = self.frozen.len();
                            self.frozen.push(block);
                            self.frozen_containers.insert(pos, frozen_items);
                        }
                        if index < freeze {
                            settled_blocks.extend(unit.blocks);
                        } else {
                            open_units.push(unit);
                        }
                    }
                } else if table.past_header {
                    let (block, frozen_items) = table.into_frozen();
                    let pos = self.frozen.len();
                    self.frozen.push(block);
                    self.frozen_containers.insert(pos, frozen_items);
                }
                for (index, unit) in iter {
                    if index < freeze {
                        settled_blocks.extend(unit.blocks);
                    } else {
                        open_units.push(unit);
                    }
                }
                let settled_count = settled_blocks.len();
                self.freeze_plain_blocks(settled_blocks, &previous_open);
                self.open_start += next_start.saturating_sub(table_prefix_len);
                let remaining_prev = previous_open.get(settled_count..).unwrap_or(&[]);
                let after_open = self.open_start;
                self.install_open_units(
                    open_units,
                    everything,
                    next_start,
                    after_open,
                    &definitions,
                    remaining_prev,
                );
                return;
            }
            None => {}
        }

        // No active container before this parse.
        record_definitions(&mut self.definitions, &definitions, next_start);
        let mut settled_blocks = Vec::new();
        let mut open_units = Vec::new();
        for (index, unit) in units.into_iter().enumerate() {
            if index < freeze {
                settled_blocks.extend(unit.blocks);
            } else {
                open_units.push(unit);
            }
        }
        let settled_count = settled_blocks.len();
        self.freeze_plain_blocks(settled_blocks, &previous_open);
        self.open_start += next_start;
        let remaining_prev = previous_open.get(settled_count..).unwrap_or(&[]);
        let after_open = self.open_start;
        self.install_open_units(
            open_units,
            everything,
            next_start,
            after_open,
            &definitions,
            remaining_prev,
        );
    }

    fn freeze_plain_blocks(&mut self, blocks: Vec<Block>, previous_open: &[BlockRef]) {
        if blocks.is_empty() {
            return;
        }
        let mut fresh = number_blocks(blocks, self.frozen_code);
        for (index, block) in fresh.iter_mut().enumerate() {
            if let Some(old) = previous_open.get(index)
                && old == block
            {
                *block = old.clone();
            }
        }
        for block in &fresh {
            self.frozen_code += block.block().code_blocks();
        }
        self.frozen.extend(fresh);
    }

    /// Places the unsettled units into `self.open` or, when there is a single unsettled
    /// list or table, opens an incremental container for it and freezes any already
    /// settled items or rows immediately.
    fn install_open_units(
        &mut self,
        mut open_units: Vec<Unit>,
        everything: bool,
        base_in_parse: usize,
        region_start: usize,
        definitions: &[Definition],
        previous_open: &[BlockRef],
    ) {
        if !everything && open_units.len() == 1 && open_units[0].blocks.len() == 1 {
            let region_text = &self.source[region_start..];
            let can_open_list = matches!(open_units[0].blocks[0], Block::List { .. })
                && list_marker_at(
                    region_text,
                    line_start(
                        region_text,
                        open_units[0].range.start.saturating_sub(base_in_parse),
                    ),
                )
                .is_some();
            if can_open_list {
                let unit = open_units.pop().expect("len == 1");
                let marker = list_marker_at(
                    region_text,
                    line_start(region_text, unit.range.start.saturating_sub(base_in_parse)),
                )
                .expect("checked marker");
                let Some(Block::List { start, items }) = unit.blocks.into_iter().next() else {
                    unreachable!();
                };
                let mut fresh_items = number_list_items(items, self.frozen_code, &[]);
                let adjusted_ranges: Vec<Range<usize>> = unit
                    .sub_ranges
                    .iter()
                    .map(|r| {
                        r.start.saturating_sub(base_in_parse)..r.end.saturating_sub(base_in_parse)
                    })
                    .collect();
                let mut list = OpenList {
                    marker,
                    start,
                    first_code: self.frozen_code,
                    frozen_items: Vec::new(),
                    frozen_code: 0,
                    open_items: Vec::new(),
                    materialized: OnceLock::new(),
                };
                if adjusted_ranges.len() == fresh_items.len()
                    && !has_bare_carriage_return(region_text)
                {
                    let (freeze_items, sub_next) =
                        settled_list_items(region_text, &adjusted_ranges, marker);
                    if freeze_items > 0 {
                        record_definitions(
                            &mut self.definitions,
                            definitions,
                            base_in_parse + sub_next,
                        );
                        let open_items = fresh_items.split_off(freeze_items);
                        for item in &fresh_items {
                            list.frozen_code += item.item.code_blocks();
                        }
                        list.frozen_items = fresh_items;
                        list.open_items = open_items;
                        self.open_start += sub_next;
                    } else {
                        list.open_items = fresh_items;
                    }
                } else {
                    list.open_items = fresh_items;
                }
                self.open_container = Some(OpenContainer::List(list));
                self.open.clear();
                return;
            }

            let can_open_table = matches!(
                &open_units[0].blocks[0],
                Block::Table { head, .. } if !head.is_empty()
            );
            if can_open_table {
                let unit = open_units.pop().expect("len == 1");
                let Some(Block::Table {
                    alignments,
                    head,
                    rows,
                }) = unit.blocks.into_iter().next()
                else {
                    unreachable!();
                };
                let synthetic_header = synthetic_table_header(&alignments, head.len());
                let header = TableHeaderRef {
                    alignments: AlignmentsRef(Arc::new(alignments)),
                    head: Arc::new(head),
                };
                let mut fresh_rows = wrap_table_rows(rows, &[]);
                let adjusted_ranges: Vec<Range<usize>> = unit
                    .sub_ranges
                    .iter()
                    .map(|r| {
                        r.start.saturating_sub(base_in_parse)..r.end.saturating_sub(base_in_parse)
                    })
                    .collect();
                let mut table = OpenTable {
                    first_code: self.frozen_code,
                    header,
                    synthetic_header,
                    past_header: false,
                    frozen_rows: Vec::new(),
                    open_rows: Vec::new(),
                    materialized: OnceLock::new(),
                };
                if adjusted_ranges.len() == fresh_rows.len()
                    && !has_bare_carriage_return(region_text)
                {
                    let (freeze_rows, sub_next) = settled_table_rows(region_text, &adjusted_ranges);
                    if freeze_rows > 0 {
                        record_definitions(
                            &mut self.definitions,
                            definitions,
                            base_in_parse + sub_next,
                        );
                        let open_rows = fresh_rows.split_off(freeze_rows);
                        table.frozen_rows = fresh_rows;
                        table.open_rows = open_rows;
                        table.past_header = true;
                        self.open_start += sub_next;
                    } else {
                        table.open_rows = fresh_rows;
                    }
                } else {
                    table.open_rows = fresh_rows;
                }
                self.open_container = Some(OpenContainer::Table(table));
                self.open.clear();
                return;
            }
        }
        self.open = finish_open_blocks(open_units, self.frozen_code, previous_open);
    }

    /// Builds every block again from the whole text and replaces the frozen blocks that
    /// came out differently. Returns the first position that changed.
    fn resolve_late_definitions(&mut self) -> Option<usize> {
        let whole = number_blocks(parse_blocks(&self.source, self.max_depth), 0);
        let mut first = None;
        if whole.len() != self.frozen.len() {
            first = Some(
                whole
                    .iter()
                    .zip(self.frozen.iter())
                    .position(|(fresh, held)| fresh != held)
                    .unwrap_or(whole.len().min(self.frozen.len())),
            );
            self.frozen = whole;
            self.frozen_containers.clear();
        } else {
            for (index, fresh) in whole.into_iter().enumerate() {
                if self.frozen[index] != fresh {
                    first.get_or_insert(index);
                    self.frozen[index] = fresh;
                    self.frozen_containers.remove(&index);
                }
            }
        }
        self.frozen_code = self
            .frozen
            .iter()
            .map(|block| block.block().code_blocks())
            .sum();
        first
    }

    /// Tells the parts of the screen that draw what changed to draw again, and nobody else.
    fn notify(&self, before: State, rebuilt: Option<usize>) {
        let Ok(watchers) = self.watchers.lock() else {
            return;
        };
        let call = |key: WatchKey| {
            if let Some(list) = watchers.entries.get(&key) {
                for (_, updater) in list {
                    updater();
                }
            }
        };

        // Fast path: only the active open list or table changed its items or rows, and
        // its header, start number, and surrounding top-level blocks stayed identical.
        if rebuilt.is_none()
            && !self.finished
            && before.frozen == self.frozen.len()
            && before.len == self.len()
            && before.open.len() == self.open.len()
            && before
                .open
                .iter()
                .zip(self.open.iter())
                .all(|(old, new)| old.same(new))
        {
            match (&before.container, &self.open_container) {
                (
                    Some(ContainerState::List {
                        start: before_start,
                        frozen_len: before_frozen,
                        open_items: before_open,
                    }),
                    Some(OpenContainer::List(list)),
                ) if *before_start == list.start => {
                    let block_index = self.frozen.len();
                    notify_sub_list(
                        block_index,
                        *before_frozen,
                        before_open,
                        list.frozen_items.len(),
                        list.total_len(),
                        false,
                        |i| list.item(i),
                        &call,
                    );
                    return;
                }
                (
                    Some(ContainerState::Table {
                        header: before_header,
                        frozen_len: before_frozen,
                        open_rows: before_open,
                    }),
                    Some(OpenContainer::Table(table)),
                ) if *before_header == table.header => {
                    let block_index = self.frozen.len();
                    notify_sub_table(
                        block_index,
                        *before_frozen,
                        before_open,
                        table.frozen_rows.len(),
                        table.total_len(),
                        |i| table.row(i),
                        &call,
                    );
                    return;
                }
                _ => {}
            }
        }

        // General path: top-level blocks changed, or a container opened, closed, or
        // changed its header.
        let after_len = self.len();
        let mut first_changed = rebuilt;
        let mut mark = |index: usize| {
            first_changed = Some(first_changed.map_or(index, |first: usize| first.min(index)));
        };
        if before.frozen != self.frozen.len() || self.finished {
            mark(before.frozen.min(self.frozen.len()));
        }
        let before_has_container = before.container.is_some();
        let after_has_container = self.open_container.is_some();
        if before_has_container != after_has_container {
            mark(before.frozen.min(self.frozen.len()));
        } else if before_has_container {
            // Both have an active container; check if header or start changed.
            let same_container = match (&before.container, &self.open_container) {
                (Some(ContainerState::List { start: a, .. }), Some(OpenContainer::List(b))) => {
                    *a == b.start
                }
                (Some(ContainerState::Table { header: a, .. }), Some(OpenContainer::Table(b))) => {
                    *a == b.header
                }
                _ => false,
            };
            if !same_container {
                mark(before.frozen.min(self.frozen.len()));
            }
        }
        let open_base_before = before.frozen + usize::from(before_has_container);
        let open_base_after = self.frozen.len() + usize::from(after_has_container);
        if open_base_before != open_base_after {
            mark(open_base_before.min(open_base_after));
        } else {
            for index in open_base_before..before.len.max(after_len) {
                let old = before.open.get(index - open_base_before);
                let new = self.open.get(index - open_base_after);
                match (old, new) {
                    (Some(old), Some(new)) if old.same(new) => {}
                    _ => {
                        mark(index);
                        break;
                    }
                }
            }
        }

        let end = before.len.max(after_len);
        if let Some(first) = first_changed
            && end > first
        {
            for chunk in first / CHUNK..end.div_ceil(CHUNK) {
                call(WatchKey::Chunk(chunk));
            }
        }
        let chunks_before = before.len.div_ceil(CHUNK);
        let chunks_after = after_len.div_ceil(CHUNK);
        if chunks_before != chunks_after {
            let low = chunks_before.min(chunks_after);
            let high = chunks_before.max(chunks_after);
            // The chunk before the first added or removed one belongs to a group that may
            // be the one gaining or losing it.
            let mut last_group = None;
            for chunk in low.saturating_sub(1)..high {
                let group = chunk / GROUP;
                if last_group != Some(group) {
                    call(WatchKey::Group(group));
                    last_group = Some(group);
                }
            }
        }
        if chunks_before.div_ceil(GROUP) != chunks_after.div_ceil(GROUP) {
            call(WatchKey::Root);
        }

        // Notify sub-watchers for an active container that either stayed open (while
        // another top-level block changed) or just closed into `self.frozen_containers`.
        let block_index = before.frozen;
        match (&before.container, &self.open_container) {
            (
                Some(ContainerState::List {
                    frozen_len: before_frozen,
                    open_items: before_open,
                    ..
                }),
                Some(OpenContainer::List(list)),
            ) if block_index == self.frozen.len() => {
                notify_sub_list(
                    block_index,
                    *before_frozen,
                    before_open,
                    list.frozen_items.len(),
                    list.total_len(),
                    self.finished,
                    |i| list.item(i),
                    &call,
                );
            }
            (
                Some(ContainerState::List {
                    frozen_len: before_frozen,
                    open_items: before_open,
                    ..
                }),
                _,
            ) => {
                if let Some(FrozenItems::List { items, .. }) =
                    self.frozen_containers.get(&block_index)
                {
                    notify_sub_list(
                        block_index,
                        *before_frozen,
                        before_open,
                        items.len(),
                        items.len(),
                        true,
                        |i| items.get(i),
                        &call,
                    );
                }
            }
            (
                Some(ContainerState::Table {
                    frozen_len: before_frozen,
                    open_rows: before_open,
                    ..
                }),
                Some(OpenContainer::Table(table)),
            ) if block_index == self.frozen.len() => {
                notify_sub_table(
                    block_index,
                    *before_frozen,
                    before_open,
                    table.frozen_rows.len(),
                    table.total_len(),
                    |i| table.row(i),
                    &call,
                );
            }
            (
                Some(ContainerState::Table {
                    frozen_len: before_frozen,
                    open_rows: before_open,
                    ..
                }),
                _,
            ) => {
                if let Some(FrozenItems::Table { rows, .. }) =
                    self.frozen_containers.get(&block_index)
                {
                    notify_sub_table(
                        block_index,
                        *before_frozen,
                        before_open,
                        rows.len(),
                        rows.len(),
                        |i| rows.get(i),
                        &call,
                    );
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn notify_sub_list<'a>(
    block_index: usize,
    before_frozen: usize,
    before_open: &[ListItemRef],
    after_frozen: usize,
    after_len: usize,
    finished: bool,
    get: impl Fn(usize) -> Option<&'a ListItemRef>,
    call: &impl Fn(WatchKey),
) {
    let before_len = before_frozen + before_open.len();
    let mut first_changed = None;
    let mut mark = |index: usize| {
        first_changed = Some(first_changed.map_or(index, |first: usize| first.min(index)));
    };
    if before_frozen != after_frozen || finished {
        mark(before_frozen.min(after_frozen));
    }
    for index in before_frozen..before_len.max(after_len) {
        let old = before_open.get(index - before_frozen);
        let new = get(index);
        match (old, new) {
            (Some(old), Some(new)) if old.same(new) => {}
            _ => {
                mark(index);
                break;
            }
        }
    }
    notify_sub_range(block_index, before_len, after_len, first_changed, call);
}

fn notify_sub_table<'a>(
    block_index: usize,
    before_frozen: usize,
    before_open: &[TableRowRef],
    after_frozen: usize,
    after_len: usize,
    get: impl Fn(usize) -> Option<&'a TableRowRef>,
    call: &impl Fn(WatchKey),
) {
    let before_len = before_frozen + before_open.len();
    let mut first_changed = None;
    let mut mark = |index: usize| {
        first_changed = Some(first_changed.map_or(index, |first: usize| first.min(index)));
    };
    if before_frozen != after_frozen {
        mark(before_frozen.min(after_frozen));
    }
    for index in before_frozen..before_len.max(after_len) {
        let old = before_open.get(index - before_frozen);
        let new = get(index);
        match (old, new) {
            (Some(old), Some(new)) if old.same(new) => {}
            _ => {
                mark(index);
                break;
            }
        }
    }
    notify_sub_range(block_index, before_len, after_len, first_changed, call);
}

fn notify_sub_range(
    block_index: usize,
    before_len: usize,
    after_len: usize,
    first_changed: Option<usize>,
    call: &impl Fn(WatchKey),
) {
    // An item or row that was there before and is there now is drawn again on its own, so
    // a delta on the open one wakes nothing else: not the finished items beside it in its
    // chunk, which would otherwise be compared again on every delta. A chunk is drawn
    // again only when the items in it come or go.
    let end = before_len.max(after_len);
    let common = before_len.min(after_len);
    if let Some(first) = first_changed {
        for index in first..common {
            call(WatchKey::SubItem(block_index, index));
        }
    }
    if before_len != after_len {
        for chunk in common / CHUNK..end.div_ceil(CHUNK) {
            call(WatchKey::SubChunk(block_index, chunk));
        }
    }
    let chunks_before = before_len.div_ceil(CHUNK);
    let chunks_after = after_len.div_ceil(CHUNK);
    if chunks_before != chunks_after {
        let low = chunks_before.min(chunks_after);
        let high = chunks_before.max(chunks_after);
        let mut last_group = None;
        for chunk in low.saturating_sub(1)..high {
            let group = chunk / GROUP;
            if last_group != Some(group) {
                call(WatchKey::SubGroup(block_index, group));
                last_group = Some(group);
            }
        }
    }
    if chunks_before.div_ceil(GROUP) != chunks_after.div_ceil(GROUP) {
        call(WatchKey::SubRoot(block_index));
    }
}

struct State {
    len: usize,
    frozen: usize,
    open: Vec<BlockRef>,
    container: Option<ContainerState>,
}

enum ContainerState {
    List {
        start: Option<u64>,
        frozen_len: usize,
        open_items: Vec<ListItemRef>,
    },
    Table {
        header: TableHeaderRef,
        frozen_len: usize,
        open_rows: Vec<TableRowRef>,
    },
}

fn finish_open_blocks(
    units: Vec<Unit>,
    first_code: usize,
    previous_open: &[BlockRef],
) -> Vec<BlockRef> {
    let blocks: Vec<Block> = units.into_iter().flat_map(|unit| unit.blocks).collect();
    let mut fresh = number_blocks(blocks, first_code);
    for (index, block) in fresh.iter_mut().enumerate() {
        if let Some(old) = previous_open.get(index)
            && old == block
        {
            *block = old.clone();
        }
    }
    fresh
}

fn number_list_items(
    items: Vec<ListItem>,
    mut first_code: usize,
    previous: &[ListItemRef],
) -> Vec<ListItemRef> {
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let count = item.code_blocks();
            let fresh = ListItemRef {
                item: Arc::new(item),
                first_code,
            };
            first_code += count;
            match previous.get(index) {
                Some(old) if *old == fresh => old.clone(),
                _ => fresh,
            }
        })
        .collect()
}

fn wrap_table_rows(rows: Vec<Vec<Inline>>, previous: &[TableRowRef]) -> Vec<TableRowRef> {
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let fresh = TableRowRef(Arc::new(row));
            match previous.get(index) {
                Some(old) if *old == fresh => old.clone(),
                _ => fresh,
            }
        })
        .collect()
}

fn record_definitions(
    target: &mut HashMap<String, String>,
    definitions: &[Definition],
    up_to: usize,
) {
    for definition in definitions {
        if definition.span.end <= up_to {
            target
                .entry(definition.label.clone())
                .or_insert_with(|| definition.destination.clone());
        }
    }
}

/// Which bullet or ordered delimiter starts a top-level list item at `start` (a line start).
fn list_marker_at(text: &str, start: usize) -> Option<ListMarker> {
    let bytes = text.as_bytes().get(start..)?;
    let mut i = 0;
    while i < bytes.len() && i < 3 && bytes[i] == b' ' {
        i += 1;
    }
    let byte = *bytes.get(i)?;
    match byte {
        b'-' | b'*' | b'+' => Some(ListMarker::Bullet(byte)),
        b'0'..=b'9' => {
            let mut digits = 0;
            while i < bytes.len() && digits < 9 && bytes[i].is_ascii_digit() {
                i += 1;
                digits += 1;
            }
            match bytes.get(i).copied() {
                Some(delim @ (b'.' | b')')) => Some(ListMarker::Ordered(delim)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// A minimal two-line table header and delimiter row that matches `alignments` and `cols`,
/// prepended when reparsing the open tail of a table whose header is already frozen.
fn synthetic_table_header(alignments: &[ColumnAlign], cols: usize) -> String {
    let mut out = String::with_capacity(cols * 6 + 4);
    out.push('|');
    for _ in 0..cols {
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for col in 0..cols {
        let align = alignments.get(col).copied().unwrap_or(ColumnAlign::None);
        match align {
            ColumnAlign::None => out.push_str("---|"),
            ColumnAlign::Left => out.push_str(":---|"),
            ColumnAlign::Center => out.push_str(":---:|"),
            ColumnAlign::Right => out.push_str("---:|"),
        }
    }
    out.push('\n');
    out
}

/// How many leading list items are settled, and where the remaining open tail begins.
///
/// An item is settled once a following sibling item with the same marker has a complete
/// first line: nothing appended after that line can turn the sibling into a continuation
/// of the earlier item or into a different block kind.
fn settled_list_items(text: &str, ranges: &[Range<usize>], marker: ListMarker) -> (usize, usize) {
    let count = ranges.len();
    if count < 2 {
        return (0, 0);
    }
    let last_start = floor_boundary(text, ranges[count - 1].start.min(text.len()));
    let mut freeze = if text[last_start..].contains(['\n', '\r']) {
        count - 1
    } else {
        count - 2
    };
    while freeze > 0 {
        let next_item_start = floor_boundary(text, ranges[freeze].start.min(text.len()));
        let next_start = line_start(text, next_item_start);
        let prev_end = ranges[freeze - 1].end;
        let leading_spaces = text[next_start..next_item_start]
            .bytes()
            .all(|byte| byte == b' ');
        if next_start > 0
            && next_start >= prev_end
            && leading_spaces
            && list_marker_at(text, next_start) == Some(marker)
        {
            return (freeze, next_start);
        }
        freeze -= 1;
    }
    (0, 0)
}

/// How many leading table body rows are settled, and where the remaining open tail begins.
///
/// Every body row of a GFM table is a single line, so once a row's line ends with a line
/// terminator nothing appended on a later line can alter that row.
fn settled_table_rows(text: &str, ranges: &[Range<usize>]) -> (usize, usize) {
    let mut freeze = ranges.len();
    while freeze > 0 {
        let last_range = &ranges[freeze - 1];
        let row_start = floor_boundary(text, last_range.start.min(text.len()));
        if text[row_start..].contains(['\n', '\r']) {
            let row_end = floor_boundary(text, last_range.end.min(text.len()));
            let next_start = next_line_start(text, row_end);
            let before_next = ranges
                .get(freeze)
                .is_none_or(|next| next_start <= line_start(text, next.start));
            if next_start > 0 && next_start >= row_end && before_next {
                return (freeze, next_start);
            }
        }
        freeze -= 1;
    }
    (0, 0)
}

/// How many of the units can no longer change.
///
/// A top-level block is settled once a later block has begun on a line that is complete:
/// with that whole line in hand, nothing that arrives afterwards can make the line part of
/// the block before it. A line still being written can, so the block before it waits:
/// `para` followed by `#` might be a heading starting, or `#hashtag` continuing the
/// paragraph.
fn settled_units(text: &str, units: &[crate::parse::Unit]) -> usize {
    let count = units.len();
    if count < 2 {
        return 0;
    }
    // A carriage return that is not followed by a line feed ends a line in CommonMark, but
    // it is the one line ending the block parser does not scan the same way from every
    // starting point: text before it and text after it can come out differently once more
    // arrives. Nothing is frozen while the text holds one; `finish` builds it all in one
    // pass. Agent output ends its lines with line feeds, so this costs nothing in practice.
    if has_bare_carriage_return(text) {
        return 0;
    }
    let last_start = floor_boundary(text, units[count - 1].range.start.min(text.len()));
    let mut settled = if text[last_start..].contains(['\n', '\r']) {
        count - 1
    } else if plain_before_line_start(text, units, count) {
        // The last line is still being written, but the block before it is a paragraph,
        // heading or rule and the line begins in column zero, so no later character can
        // make the line continue a list, quote or code block above it. Without this the
        // previous paragraph is parsed again on every delta while the next one grows.
        count - 1
    } else {
        count - 2
    };
    // A block is settled only once a blank line separates it from the next one. Without
    // the blank line, text that arrives later can still turn the next block's first line
    // into a continuation of this one: the closing end of an inline code span, a
    // strikethrough or an entity, a lazy quote line, a setext underline, a table row. The
    // blank line is what CommonMark ends every one of those on, so it is the one point
    // after which nothing that follows can reach back.
    while settled > 0 {
        let from = floor_boundary(text, units[settled - 1].range.start.min(text.len()));
        let to = floor_boundary(text, units[settled].range.start.min(text.len()));
        if from <= to && ends_with_blank_line(&text[from..to]) {
            break;
        }
        settled -= 1;
    }
    settled
}

/// Whether a carriage return stands without a line feed after it.
fn has_bare_carriage_return(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(at, byte)| *byte == b'\r' && bytes.get(at + 1) != Some(&b'\n'))
}

/// Whether the text ends with a blank line: a line terminator, then a line holding
/// nothing or only spaces and tabs, then a line terminator.
fn ends_with_blank_line(text: &str) -> bool {
    let blank = |character: char| character == ' ' || character == '\t';
    let text = text.trim_end_matches(blank);
    let text = if let Some(rest) = text.strip_suffix("\r\n") {
        rest
    } else if let Some(rest) = text.strip_suffix(['\n', '\r']) {
        rest
    } else {
        return false;
    };
    text.trim_end_matches(blank).ends_with(['\n', '\r'])
}

/// The start of the line holding `position`. Lines end the three ways CommonMark says:
/// a line feed, a carriage return, or both.
fn line_start(text: &str, position: usize) -> usize {
    let position = floor_boundary(text, position.min(text.len()));
    text[..position].rfind(['\n', '\r']).map_or(0, |at| at + 1)
}

/// The start of the first line that begins at or after `position`.
fn next_line_start(text: &str, position: usize) -> usize {
    let position = floor_boundary(text, position.min(text.len()));
    let bytes = text.as_bytes();
    if position == 0 {
        return 0;
    }
    match bytes[position - 1] {
        b'\n' => return position,
        b'\r' if bytes.get(position) != Some(&b'\n') => return position,
        _ => {}
    }
    match text[position..].find(['\n', '\r']) {
        None => text.len(),
        Some(at) => {
            let at = position + at;
            if bytes[at] == b'\r' && bytes.get(at + 1) == Some(&b'\n') {
                at + 2
            } else {
                at + 1
            }
        }
    }
}

fn floor_boundary(text: &str, mut position: usize) -> usize {
    while position > 0 && !text.is_char_boundary(position) {
        position -= 1;
    }
    position
}

/// Whether the unit before the last holds only paragraphs, headings and rules, and the last
/// unit starts at the beginning of its line. Lists, quotes, HTML and code can continue
/// across a blank line, so they are not in the set.
fn plain_before_line_start(text: &str, units: &[crate::parse::Unit], count: usize) -> bool {
    let last = units[count - 1].range.start.min(text.len());
    let last = floor_boundary(text, last);
    last == line_start(text, last)
        && units[count - 2].blocks.iter().all(|block| {
            matches!(
                block,
                Block::Paragraph(_) | Block::Heading { .. } | Block::Rule
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send + Sync>() {}

    #[test]
    fn fr37_a_stream_can_be_handed_to_a_worker() {
        assert_send::<MarkdownStream>();
    }

    #[test]
    fn a_paragraph_followed_by_an_unfinished_line_is_not_frozen() {
        let mut stream = MarkdownStream::default();
        stream.push_delta("para\n#");
        assert_eq!(stream.frozen_len(), 0);
        stream.push_delta("hashtag\n");
        assert_eq!(stream.len(), 1, "{:?}", stream.blocks().collect::<Vec<_>>());
    }
}
