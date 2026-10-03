//! From blocks to widgets.
//!
//! Nothing here is new to the Renderer: a document is Columns, Rows, Texts with runs,
//! Dividers, Checkboxes, Buttons, Images, ScrollRows and one SelectionContainer, and every
//! colour is a role.
//!
//! The blocks are drawn through three levels of components, groups of chunks of blocks,
//! and each level returns a fragment, so on screen every block is a direct child of one
//! Column. The levels exist for the streaming case. A delta changes the last block or
//! two, and only the chunk holding them is drawn again: the stream tells exactly that
//! chunk, and nothing above it runs. The work a delta causes on this thread is therefore
//! the same for the first paragraph of a reply and for the hundredth.

use crate::code::{CodeKind, CodeSpan, prepare, without_controls};
use crate::highlight::{self, key_of};
use crate::model::{Block, BlockRef, ColumnAlign, Inline, ListItem};
use crate::options::{CopyRequest, Labels, MarkdownOptions};
use crate::stream::{CHUNK, GROUP, MarkdownStream, WatchGuard, WatchKey};
use dioxus_compose::prelude::*;
use dioxus_compose::spans::{TextSpan, TextSpans};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

/// Where the blocks come from: parsed once from a finished text, or held by a stream.
#[derive(Clone)]
pub(crate) enum Doc {
    Static(Arc<Vec<BlockRef>>),
    Stream(SyncSignal<MarkdownStream>),
}

impl PartialEq for Doc {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Doc::Static(left), Doc::Static(right)) => Arc::ptr_eq(left, right),
            (Doc::Stream(left), Doc::Stream(right)) => left == right,
            _ => false,
        }
    }
}

impl Doc {
    /// Read without subscribing: the stream says which parts changed, so nothing here
    /// should be woken by every write to it.
    fn len(&self) -> usize {
        match self {
            Doc::Static(blocks) => blocks.len(),
            Doc::Stream(stream) => stream.peek().len(),
        }
    }

    fn collect(&self, first: usize, count: usize) -> Vec<(usize, BlockRef, bool)> {
        match self {
            Doc::Static(blocks) => blocks
                .iter()
                .enumerate()
                .skip(first)
                .take(count)
                .map(|(index, block)| (index, block.clone(), false))
                .collect(),
            Doc::Stream(stream) => {
                let stream = stream.peek();
                (first..first + count)
                    .filter_map(|index| {
                        stream
                            .block(index)
                            .map(|block| (index, block.clone(), stream.is_open(index)))
                    })
                    .collect()
            }
        }
    }

    fn watch(&self, key: WatchKey) -> Option<WatchGuard> {
        match self {
            Doc::Static(_) => None,
            Doc::Stream(stream) => Some(
                stream
                    .peek()
                    .watch(key, dioxus_core::schedule_update()),
            ),
        }
    }
}

/// Everything about drawing that is the same for every block of one document.
#[derive(Clone, PartialEq)]
pub(crate) struct View {
    pub on_link: Option<EventHandler<String>>,
    pub on_copy: Option<EventHandler<CopyRequest>>,
    pub options: MarkdownOptions,
    pub expanded: Signal<BTreeSet<usize>>,
}

/// A shared [`View`], compared by identity first so that the common case costs nothing.
#[derive(Clone)]
pub(crate) struct ViewRef(Rc<View>);

impl PartialEq for ViewRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

impl std::ops::Deref for ViewRef {
    type Target = View;

    fn deref(&self) -> &View {
        &self.0
    }
}

/// Registers a component to be drawn again when its part of a stream changes, and keeps
/// the registration for as long as the component lives.
fn use_watch(doc: &Doc, key: WatchKey, generation: u64) {
    let slot = use_hook(|| Rc::new(RefCell::new(None::<(u64, Option<WatchGuard>)>)));
    let mut slot = slot.borrow_mut();
    if slot.as_ref().map(|(seen, _)| *seen) != Some(generation) {
        *slot = Some((generation, doc.watch(key)));
    }
}

/// The document's root: what the `Markdown` component draws.
pub(crate) fn document(doc: Doc, view: ViewRef, generation: u64) -> Element {
    rsx! {
        SelectionContainer {
            Column {
                fill_max_width: true,
                space_role: SpaceRole::Md,
                Groups { doc, view, generation }
            }
        }
    }
}

#[component]
fn Groups(doc: Doc, view: ViewRef, generation: u64) -> Element {
    use_watch(&doc, WatchKey::Root, generation);
    let groups = doc.len().div_ceil(CHUNK).div_ceil(GROUP);
    rsx! {
        for index in 0..groups {
            Group {
                key: "{index}",
                doc: doc.clone(),
                view: view.clone(),
                generation,
                index,
            }
        }
    }
}

#[component]
fn Group(doc: Doc, view: ViewRef, generation: u64, index: usize) -> Element {
    use_watch(&doc, WatchKey::Group(index), generation);
    let chunks = doc.len().div_ceil(CHUNK);
    let first = index * GROUP;
    let last = chunks.min(first + GROUP);
    rsx! {
        for chunk in first..last {
            Chunk {
                key: "{chunk}",
                doc: doc.clone(),
                view: view.clone(),
                generation,
                index: chunk,
            }
        }
    }
}

#[component]
fn Chunk(doc: Doc, view: ViewRef, generation: u64, index: usize) -> Element {
    use_watch(&doc, WatchKey::Chunk(index), generation);
    let blocks = doc.collect(index * CHUNK, CHUNK);
    rsx! {
        for (position, block, open) in blocks {
            BlockView {
                key: "{position}",
                block,
                open,
                view: view.clone(),
            }
        }
    }
}

/// One top-level block. Its props compare equal for a frozen block, so a frozen block is
/// never drawn again.
#[component]
fn BlockView(block: BlockRef, open: bool, view: ViewRef) -> Element {
    let counter = Cell::new(block.first_code());
    let context = Context {
        view: &view,
        open,
        muted: false,
        code: &counter,
    };
    render_block(block.block(), &context)
}

#[derive(Clone, Copy)]
struct Context<'a> {
    view: &'a ViewRef,
    open: bool,
    /// Inside a quote, where text is set in the secondary ink.
    muted: bool,
    /// The fold-state number the next code block takes.
    code: &'a Cell<usize>,
}

impl Context<'_> {
    fn ink(&self) -> Option<Paint> {
        self.muted
            .then_some(Paint::Role(ColorRole::OnSurfaceVariant))
    }
}

#[derive(Clone, Copy, Default)]
struct Look {
    role: Option<TypeRole>,
    align: Option<TextAlign>,
    weight: Option<f32>,
}

fn heading_role(level: u8) -> TypeRole {
    match level {
        1 => TypeRole::Headline,
        2 => TypeRole::Title,
        3 => TypeRole::Subtitle,
        _ => TypeRole::BodyStrong,
    }
}

fn render_block(block: &Block, context: &Context<'_>) -> Element {
    match block {
        Block::Heading { level, content } => inline_text(
            content,
            context,
            Look {
                role: Some(heading_role(*level)),
                ..Look::default()
            },
        ),
        Block::Paragraph(inline) => paragraph(inline, context),
        Block::Rule => rsx! {
            Divider {}
        },
        Block::Html(raw) => {
            let color = context.ink();
            rsx! {
                Text { text: raw.clone(), color, streaming: true }
            }
        }
        Block::Code(code) => {
            let ordinal = context.code.get();
            context.code.set(ordinal + 1);
            rsx! {
                CodeView {
                    language: code.language.clone(),
                    text: code.text.clone(),
                    open: context.open,
                    ordinal,
                    view: context.view.clone(),
                }
            }
        }
        Block::Quote(blocks) => {
            let inner = Context {
                muted: true,
                ..*context
            };
            let children = blocks.iter().map(|block| render_block(block, &inner));
            rsx! {
                Row {
                    fill_max_width: true,
                    space_role: SpaceRole::Sm,
                    Divider { vertical: true, fill_max_height: true }
                    Column { weight: 1.0, space_role: SpaceRole::Sm, {children} }
                }
            }
        }
        Block::List { start, items } => {
            let rows = items
                .iter()
                .enumerate()
                .map(|(position, item)| list_item(*start, position, item, context));
            rsx! {
                Column { fill_max_width: true, space_role: SpaceRole::Xs, {rows} }
            }
        }
        Block::Table {
            alignments,
            head,
            rows,
        } => {
            let alignment = |column: usize| match alignments.get(column) {
                Some(ColumnAlign::Left) => Some(TextAlign::Start),
                Some(ColumnAlign::Center) => Some(TextAlign::Center),
                Some(ColumnAlign::Right) => Some(TextAlign::End),
                _ => None,
            };
            let cell = |column: usize, inline: &Inline, header: bool| {
                inline_text(
                    inline,
                    context,
                    Look {
                        role: header.then_some(TypeRole::BodyStrong),
                        align: alignment(column),
                        weight: Some(1.0),
                    },
                )
            };
            let header = head
                .iter()
                .enumerate()
                .map(|(column, inline)| cell(column, inline, true));
            let body = rows.iter().map(|row| {
                let cells = row
                    .iter()
                    .enumerate()
                    .map(|(column, inline)| cell(column, inline, false));
                rsx! {
                    Row { fill_max_width: true, space_role: SpaceRole::Md, {cells} }
                }
            });
            rsx! {
                ScrollRow {
                    fill_max_width: true,
                    Column {
                        fill_max_width: true,
                        space_role: SpaceRole::Xs,
                        Row { fill_max_width: true, space_role: SpaceRole::Md, {header} }
                        Divider {}
                        {body}
                    }
                }
            }
        }
    }
}

fn list_item(start: Option<u64>, position: usize, item: &ListItem, context: &Context<'_>) -> Element {
    let ink = context.ink();
    let marker = match (item.task, start) {
        (Some(checked), _) => rsx! {
            Checkbox { checked }
        },
        (None, Some(first)) => {
            let number = first.saturating_add(position as u64);
            rsx! {
                Text { text: format!("{number}."), color: ink }
            }
        }
        (None, None) => rsx! {
            Text { text: "\u{2022}", color: ink }
        },
    };
    let children = item
        .blocks
        .iter()
        .map(|block| render_block(block, context));
    rsx! {
        Row {
            fill_max_width: true,
            space_role: SpaceRole::Sm,
            {marker}
            Column { weight: 1.0, space_role: SpaceRole::Xs, {children} }
        }
    }
}

/// A paragraph, with any image the application resolved drawn in place of its words.
fn paragraph(inline: &Inline, context: &Context<'_>) -> Element {
    if let Some(resolver) = &context.view.options.image_resolver {
        let mut images: Vec<_> = inline.images.iter().collect();
        images.sort_by_key(|image| image.start);
        let mut parts = Vec::new();
        let mut at = 0;
        for image in images {
            let start = image.start as usize;
            if start < at {
                continue;
            }
            let Some(asset_id) = resolver.resolve(&image.url) else {
                continue;
            };
            if start > at {
                parts.push(inline_text(
                    &inline.slice(at, start),
                    context,
                    Look::default(),
                ));
            }
            parts.push(rsx! {
                Image { asset_id }
            });
            at = start + image.len as usize;
        }
        if !parts.is_empty() {
            if at < inline.text.len() {
                parts.push(inline_text(
                    &inline.slice(at, inline.text.len()),
                    context,
                    Look::default(),
                ));
            }
            return rsx! {
                Column { fill_max_width: true, space_role: SpaceRole::Xs, {parts.into_iter()} }
            };
        }
    }
    inline_text(inline, context, Look::default())
}

/// The runs of an inline, as the `Text` widget carries them.
///
/// A link run carries its position in the inline's link list, plus one because zero means
/// "not a link", and only when there is a handler to report it to. Without one a link is
/// underlined words and pressing it does nothing.
pub(crate) fn spans_of(inline: &Inline, linked: bool) -> TextSpans {
    TextSpans::new(inline.runs.iter().map(|run| {
        let mut span = TextSpan::new(run.start, run.len);
        if run.style.bold {
            span = span.bold();
        }
        if run.style.italic {
            span = span.italic();
        }
        if run.style.strikethrough {
            span = span.strikethrough();
        }
        if run.style.code {
            span = span
                .with_role(TypeRole::Mono)
                .with_color(Paint::Role(ColorRole::OnSurfaceVariant));
        }
        if let Some(link) = run.link {
            span = span
                .underline()
                .with_color(Paint::Role(ColorRole::Primary));
            if linked {
                span.on_click = Some(u64::from(link) + 1);
            }
        }
        span
    }))
}

fn inline_text(inline: &Inline, context: &Context<'_>, look: Look) -> Element {
    let spans = spans_of(inline, context.view.on_link.is_some());
    let color = context.ink();
    let text = inline.text.clone();
    match context.view.on_link {
        // Every text gets the handler while there is one, links or not, so a paragraph
        // whose first link closes mid-stream changes its runs and keeps its node.
        Some(handler) => {
            let links = inline.links.clone();
            rsx! {
                Text {
                    text,
                    spans,
                    type_role: look.role,
                    text_align: look.align,
                    weight: look.weight,
                    color,
                    streaming: true,
                    on_link: move |value: u64| {
                        let found = value
                            .checked_sub(1)
                            .and_then(|index| links.get(index as usize));
                        if let Some(url) = found {
                            handler.call(url.clone());
                        }
                    },
                }
            }
        }
        None => rsx! {
            Text {
                text,
                spans,
                type_role: look.role,
                text_align: look.align,
                weight: look.weight,
                color,
                streaming: true,
            }
        },
    }
}

/// A code block inside a document: its fold state lives in the document's set, under its
/// place among the document's code blocks.
#[component]
fn CodeView(
    language: Option<String>,
    text: String,
    open: bool,
    ordinal: usize,
    view: ViewRef,
) -> Element {
    let mut folds = view.expanded;
    let expanded = folds.read().contains(&ordinal);
    let options = &view.options;
    rsx! {
        CodeFrame {
            language,
            text,
            streaming: open,
            expanded,
            on_toggle: move |_| {
                let mut set = folds.write();
                if !set.remove(&ordinal) {
                    set.insert(ordinal);
                }
            },
            on_copy: view.on_copy,
            fold_lines: options.fold_lines,
            labels: options.labels.clone(),
            highlight: options.highlight,
        }
    }
}

/// A block of code, or of a program's output, on its own.
///
/// The same box `Markdown` draws for a fenced block, for anything else that is code: a
/// command's output, a log, a diff. It scrolls sideways rather than wrapping, folds past
/// `fold_lines`, and has a copy control that hands the application the text exactly as
/// given.
///
/// With `language` unset the text is treated as terminal output: its colours are kept as
/// colour roles and every other escape sequence is removed. `diff` or `patch` colours a
/// unified diff by line. Any other language is coloured on the highlighter thread and
/// shows as plain monospace until the colours arrive.
///
/// Set `streaming` while the text is still growing: only the new tail is sent to the
/// Renderer, and colouring waits until it is unset.
#[component]
pub fn CodeBlock(
    language: Option<String>,
    text: String,
    on_copy: Option<EventHandler<CopyRequest>>,
    #[props(default = crate::options::DEFAULT_FOLD_LINES)] fold_lines: usize,
    #[props(default)] labels: Labels,
    #[props(default)] streaming: bool,
    /// Where the unfolded state is kept. Give a signal the screen owns when the block
    /// can be scrolled out of a lazy list and back, so it comes back as it was left.
    expanded: Option<Signal<bool>>,
    #[props(default = true)] highlight: bool,
) -> Element {
    let own = use_signal(|| false);
    let mut expanded = expanded.unwrap_or(own);
    let is_expanded = expanded();
    rsx! {
        CodeFrame {
            language,
            text,
            streaming,
            expanded: is_expanded,
            on_toggle: move |_| {
                let next = !*expanded.peek();
                expanded.set(next);
            },
            on_copy,
            fold_lines,
            labels,
            highlight,
        }
    }
}

/// The runs a highlighter result contributes to the shown text.
fn clip(spans: &[CodeSpan], length: usize) -> Vec<CodeSpan> {
    let length = length as u32;
    spans
        .iter()
        .filter(|span| span.start < length)
        .map(|span| CodeSpan {
            len: span.len.min(length - span.start),
            ..*span
        })
        .filter(|span| span.len > 0)
        .collect()
}

fn code_spans(spans: &[CodeSpan]) -> TextSpans {
    TextSpans::new(spans.iter().map(|code| {
        let mut span = TextSpan::new(code.start, code.len);
        if let Some(role) = code.role {
            span = span.with_color(Paint::Role(role));
        }
        if code.bold {
            span = span.bold();
        }
        if code.italic {
            span = span.italic();
        }
        if code.underline {
            span = span.underline();
        }
        span
    }))
}

type Colours = Option<(u64, Arc<Vec<CodeSpan>>)>;

#[component]
fn CodeFrame(
    language: Option<String>,
    text: String,
    streaming: bool,
    expanded: bool,
    on_toggle: EventHandler<()>,
    on_copy: Option<EventHandler<CopyRequest>>,
    fold_lines: usize,
    labels: Labels,
    highlight: bool,
) -> Element {
    let fold_lines = fold_lines.max(1);
    let kind = CodeKind::of(language.as_deref());
    let limit = if expanded { usize::MAX } else { fold_lines };
    let prepared = prepare(&text, kind, limit);
    let hidden = prepared.total_lines.saturating_sub(prepared.shown_lines);
    let foldable = prepared.total_lines > fold_lines;

    // Colours come from the highlighter thread and land in a signal that thread can
    // write. Until they do, and for as long as the block is still growing, the code is
    // plain monospace.
    let colours = use_signal_sync(|| None as Colours);
    let requested = use_hook(|| Rc::new(Cell::new(0_u64)));
    let mut spans = prepared.spans.clone();
    let wants = highlight && !streaming && kind == CodeKind::Language && !text.is_empty();
    if wants {
        let language = language.as_deref().unwrap_or_default();
        let source = without_controls(&text);
        let key = key_of(language, &source);
        if requested.get() != key {
            requested.set(key);
            let mut colours = colours;
            highlight::request(
                key,
                language,
                Arc::<str>::from(&*source),
                Box::new(move |key: u64, result: Arc<Vec<CodeSpan>>| {
                    // The block may be gone by now; then there is nobody to tell.
                    if let Ok(mut slot) = colours.try_write() {
                        *slot = Some((key, result));
                    }
                }),
            );
        }
        let current = colours.read();
        if let Some((done, result)) = &*current {
            if *done == key {
                spans = clip(result, prepared.text.len());
            }
        }
    }
    let text_spans = code_spans(&spans);

    let copy_language = language.clone();
    let copy_text = text.clone();
    let copy = move |_: ()| {
        if let Some(on_copy) = on_copy {
            on_copy.call(CopyRequest {
                language: copy_language.clone(),
                text: copy_text.clone(),
            });
        }
    };
    let fold_label = if expanded {
        labels.show_less.to_string()
    } else {
        labels.show_more(hidden)
    };
    rsx! {
        Column {
            fill_max_width: true,
            background: Paint::Role(ColorRole::SurfaceContainer),
            shape_role: ShapeRole::Small,
            padding_role: SpaceRole::Sm,
            space_role: SpaceRole::Xs,
            Row {
                fill_max_width: true,
                Text {
                    text: language.clone().unwrap_or_default(),
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                    weight: 1.0,
                }
                Button {
                    text: labels.copy.to_string(),
                    variant: ButtonVariant::Text,
                    on_click: copy,
                }
            }
            ScrollRow {
                fill_max_width: true,
                Text {
                    text: prepared.text,
                    spans: text_spans,
                    type_role: TypeRole::Mono,
                    color: Paint::Role(ColorRole::OnSurface),
                    streaming: true,
                }
            }
            if foldable {
                Button {
                    text: fold_label,
                    variant: ButtonVariant::Text,
                    on_click: move |_| on_toggle.call(()),
                }
            }
        }
    }
}

/// What the `Markdown` component draws for its props.
pub(crate) fn markdown(
    source: Option<String>,
    stream: Option<SyncSignal<MarkdownStream>>,
    view: View,
) -> Element {
    let parsed = use_hook(|| Rc::new(RefCell::new(None::<(String, usize, Doc)>)));
    let held = use_hook(|| Rc::new(RefCell::new(None::<ViewRef>)));
    let (doc, generation) = match stream {
        Some(stream) => {
            // Read, not peeked: a stream replaced by another is a different document, and
            // this is what notices.
            let generation = stream.read().generation();
            (Doc::Stream(stream), generation)
        }
        None => {
            let source = source.unwrap_or_default();
            let depth = view.options.max_depth;
            let mut cache = parsed.borrow_mut();
            let doc = match cache.as_ref() {
                Some((seen, seen_depth, doc)) if *seen == source && *seen_depth == depth => {
                    doc.clone()
                }
                _ => {
                    let doc = Doc::Static(Arc::new(crate::parse(&source, depth)));
                    *cache = Some((source, depth, doc.clone()));
                    doc
                }
            };
            (doc, 0)
        }
    };
    let view = {
        let mut held = held.borrow_mut();
        match held.as_ref() {
            Some(kept) if **kept == view => kept.clone(),
            _ => {
                let fresh = ViewRef(Rc::new(view));
                *held = Some(fresh.clone());
                fresh
            }
        }
    };
    document(doc, view, generation)
}
