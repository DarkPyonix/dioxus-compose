//! A document that arrives a piece at a time.
//!
//! The text grows at the end, and almost all of a document is settled long before it is
//! finished: a paragraph that has been followed by another block can never change again.
//! So a block is frozen the moment the next one starts, and only the blocks after the last
//! frozen one are parsed again when a delta lands. The cost of a delta is the size of the
//! block it lands in, not the size of the message.

use crate::model::{BlockRef, number_blocks};
use crate::options::MarkdownOptions;
use crate::parse::{parse_blocks, parse_region};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// How many blocks one chunk component draws.
pub(crate) const CHUNK: usize = 8;
/// How many chunks one group component holds.
pub(crate) const GROUP: usize = 8;

/// Which part of the drawn tree a watcher stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WatchKey {
    Root,
    Group(usize),
    Chunk(usize),
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
        if let Ok(mut watchers) = self.watchers.lock() {
            if let Some(list) = watchers.entries.get_mut(&self.key) {
                list.retain(|(token, _)| *token != self.token);
                if list.is_empty() {
                    watchers.entries.remove(&self.key);
                }
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
            .field("open", &self.open.len())
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
        self.frozen.len() + self.open.len()
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
            self.frozen.get(index)
        } else {
            self.open.get(index - self.frozen.len())
        }
    }

    /// Every block, frozen ones first.
    pub fn blocks(&self) -> impl Iterator<Item = &BlockRef> {
        self.frozen.iter().chain(self.open.iter())
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
        State {
            len: self.len(),
            frozen: self.frozen.len(),
            open: self.open.clone(),
        }
    }

    /// Parses the open part again. With `everything`, every block it holds is frozen.
    fn reparse(&mut self, everything: bool) {
        let region_text = &self.source[self.open_start..];
        let region = parse_region(region_text, self.max_depth, &self.definitions);
        if !region.definitions.is_empty() {
            self.saw_definitions = true;
        }
        let units = region.units;
        let mut freeze = if everything {
            units.len()
        } else {
            settled_units(region_text, &units)
        };
        let mut next_start = if freeze == 0 {
            0
        } else if freeze < units.len() {
            let after_last = next_line_start(region_text, units[freeze - 1].range.end);
            after_last.min(line_start(region_text, units[freeze].range.start))
        } else {
            region_text.len()
        };
        // The text parsed again next time must begin after everything frozen now, or a
        // frozen block would be found a second time. The line arithmetic above guarantees
        // it for every input it was written for; this keeps the guarantee for the rest.
        if freeze > 0 && freeze < units.len() && next_start < units[freeze - 1].range.end {
            freeze = 0;
            next_start = 0;
        }
        for definition in region.definitions {
            if definition.span.end <= next_start {
                self.definitions
                    .entry(definition.label)
                    .or_insert(definition.destination);
            }
        }

        let previous_open = std::mem::take(&mut self.open);
        let mut settled = Vec::new();
        let mut unsettled = Vec::new();
        for (index, unit) in units.into_iter().enumerate() {
            if index < freeze {
                settled.extend(unit.blocks);
            } else {
                unsettled.extend(unit.blocks);
            }
        }
        let settled_count = settled.len();
        settled.extend(unsettled);
        let mut fresh = number_blocks(settled, self.frozen_code);
        // A block that came out the same as last time keeps the value it had, so the
        // screen sees the same shared block and does not look inside it to find out.
        for (index, block) in fresh.iter_mut().enumerate() {
            if let Some(old) = previous_open.get(index) {
                if old == block {
                    *block = old.clone();
                }
            }
        }
        let open = fresh.split_off(settled_count);
        for block in &fresh {
            self.frozen_code += block.block().code_blocks();
        }
        self.frozen.extend(fresh);
        self.open = open;
        self.open_start += next_start;
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
        } else {
            for (index, fresh) in whole.into_iter().enumerate() {
                if self.frozen[index] != fresh {
                    first.get_or_insert(index);
                    self.frozen[index] = fresh;
                }
            }
        }
        self.frozen_code = self.frozen.iter().map(|block| block.block().code_blocks()).sum();
        first
    }

    /// Tells the parts of the screen that draw what changed to draw again, and nobody else.
    fn notify(&self, before: State, rebuilt: Option<usize>) {
        let after_len = self.len();
        // The first position whose block or whose open state is not what it was.
        let mut first_changed = rebuilt;
        let mut mark = |index: usize| {
            first_changed = Some(first_changed.map_or(index, |first: usize| first.min(index)));
        };
        if before.frozen != self.frozen.len() || self.finished {
            mark(before.frozen.min(self.frozen.len()));
        }
        for index in before.frozen..before.len.max(after_len) {
            let old = before.open.get(index - before.frozen);
            let new = self.block(index);
            match (old, new) {
                (Some(old), Some(new)) if old.same(new) => {}
                _ => {
                    mark(index);
                    break;
                }
            }
        }
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
        let end = before.len.max(after_len);
        if let Some(first) = first_changed {
            if end > first {
                for chunk in first / CHUNK..end.div_ceil(CHUNK) {
                    call(WatchKey::Chunk(chunk));
                }
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
    }
}

struct State {
    len: usize,
    frozen: usize,
    open: Vec<BlockRef>,
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
    let last_start = floor_boundary(text, units[count - 1].range.start.min(text.len()));
    if text[last_start..].contains(['\n', '\r']) {
        count - 1
    } else {
        count - 2
    }
}

/// The start of the line holding `position`. Lines end the three ways CommonMark says:
/// a line feed, a carriage return, or both.
fn line_start(text: &str, position: usize) -> usize {
    let position = floor_boundary(text, position.min(text.len()));
    text[..position]
        .rfind(['\n', '\r'])
        .map_or(0, |at| at + 1)
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
