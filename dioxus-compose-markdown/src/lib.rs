//! Markdown documents for dioxus-compose.
//!
//! A message from a model, a README, a tool's output: CommonMark with GitHub's tables,
//! task lists and strikethrough, drawn with the widgets dioxus-compose already has. The
//! Renderer never hears the word markdown. It receives Columns, Rows, Texts with runs,
//! Dividers, Checkboxes, Buttons, ScrollRows and a SelectionContainer, and every colour in
//! them is a role, so a document looks right in every design system and both schemes.
//!
//! Two ways in, and the same input gives the same tree:
//!
//! ```ignore
//! // A finished text.
//! rsx! { Markdown { source: message.text.clone(), on_link: open_link, on_copy: copy_code } }
//!
//! // A reply arriving a piece at a time.
//! let stream = use_signal_sync(|| MarkdownStream::new(MarkdownOptions::default()));
//! // on the network worker:
//! stream.write().push_delta(delta);
//! stream.write().finish();
//! // on screen:
//! rsx! { Markdown { stream, on_link: open_link, on_copy: copy_code } }
//! ```
//!
//! Untrusted input is the normal case: raw HTML is shown as the characters that were
//! written, images are not fetched, nesting is capped, and no input makes it panic.

pub mod ansi;
pub mod code;
pub mod highlight;
pub mod model;
mod options;
mod parse;
mod stream;
mod view;

pub use code::CodeSpan;
pub use model::BlockRef;
pub use options::{
    CopyRequest, DEFAULT_FOLD_LINES, ImageResolver, Labels, MarkdownOptions,
};
pub use parse::DEFAULT_MAX_DEPTH;
pub use stream::MarkdownStream;
pub use view::CodeBlock;

use dioxus_compose::prelude::*;
use std::collections::BTreeSet;

/// Parses a finished document into its blocks.
///
/// What `Markdown { source }` draws, available on its own for a caller that wants to look
/// at the structure, or to parse on a worker thread and keep the result.
pub fn parse(source: &str, max_depth: usize) -> Vec<BlockRef> {
    model::number_blocks(parse::parse_blocks(source, max_depth), 0)
}

/// Draws a markdown document.
///
/// Give it either `source`, a finished text, or `stream`, a [`MarkdownStream`] that a
/// worker feeds. With both, `stream` wins.
///
/// - `on_link` receives a link's address, exactly as written (a reference link's address
///   as its definition gave it). The crate opens nothing; whether to open a browser, handle
///   the address inside the application or ask first is the application's decision.
///   Without it links are underlined and inert.
/// - `on_copy` receives a code block's language word and its exact text when the copy
///   control is pressed. Putting it on the clipboard is the application's job.
/// - `expanded` holds which code blocks the reader unfolded, by their place among the
///   document's code blocks. Keep it in the message's state when the message lives in a
///   lazy list, so a message scrolled away and back comes back unfolded.
#[component]
pub fn Markdown(
    source: Option<String>,
    stream: Option<SyncSignal<MarkdownStream>>,
    on_link: Option<EventHandler<String>>,
    on_copy: Option<EventHandler<CopyRequest>>,
    #[props(default)] options: MarkdownOptions,
    expanded: Option<Signal<BTreeSet<usize>>>,
) -> Element {
    let own = use_signal(BTreeSet::new);
    let expanded = expanded.unwrap_or(own);
    view::markdown(
        source,
        stream,
        view::View {
            on_link,
            on_copy,
            options,
            expanded,
        },
    )
}
