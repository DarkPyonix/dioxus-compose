# dioxus-compose-markdown

Markdown documents for dioxus-compose: a model's reply, a README, a tool's output.
CommonMark plus GitHub's tables, task lists and strikethrough, drawn with the widgets
dioxus-compose already has.

```rust
use dioxus_compose::prelude::*;
use dioxus_compose_markdown::{Markdown, MarkdownOptions, MarkdownStream};

// A finished text.
rsx! { Markdown { source: message.text.clone(), on_link: open_link, on_copy: copy_code } }

// A reply arriving a piece at a time. The stream is `Send`: feed it on the thread the
// network delivers on.
let stream = use_signal_sync(|| MarkdownStream::new(MarkdownOptions::default()));
stream.write().push_delta(delta);   // as each piece arrives
stream.write().finish();            // when the reply is complete
rsx! { Markdown { stream, on_link: open_link, on_copy: copy_code } }
```

`CodeBlock` is public on its own, for code that is not inside a document: a command's
output, a log, a diff. With no `language` it treats its text as terminal output, keeping
red, green and the other colours as colour roles and removing every other escape sequence.

## What reaches the Renderer

Nothing new. A document is `Column`, `Row`, `Text` with runs, `Divider`, `Checkbox`,
`Button`, `ScrollRow`, `SelectionContainer`, and `Image` when the application resolves an
image itself. Every colour is a `ColorRole`; there is no path that sends a literal colour,
so a document is right in every design system and in both schemes, and a theme change
costs this crate nothing.

## Streaming

A block is frozen once the next block has begun on a complete line. Only the blocks after
the last frozen one are parsed again when a delta arrives, so a delta costs the size of the
block it lands in, not the size of the message. Frozen blocks are shared values, compared
by identity, and their widgets are never drawn again.

The drawing side is arranged for the same property. Blocks are drawn through groups of
chunks of blocks, every level a fragment, so all blocks are siblings in one `Column`. The
stream tells exactly the chunk that changed to draw again; the component above it does not
run. A growing paragraph is sent as `AppendText` with only its new tail.

After `finish()` the tree is the tree the whole text draws as `source`. The one place a
frozen block is built again is `finish()` itself, for a reference link whose definition
came later in the text: CommonMark resolves it, a stream cannot until the definition has
arrived, so during streaming it is the words that were written and at `finish()` it becomes
the link.

## Safety

Agent output is untrusted input. Raw HTML, block and inline, is shown as the characters
that were written. Images are not fetched: an image is its description and its address,
and the address is a link. An application that wants pictures supplies an
`ImageResolver`, which turns an address into an asset id it registered, or declines.
Quotes and lists nest at most 16 levels (configurable) before deeper content is shown flat;
the parser runs in one pass with an explicit stack, so ten thousand nested quotes cost ten
thousand steps.

## Colouring code

Highlighting is regular-expression work proportional to the number of lines, so it never
runs on the UI thread. A code block asks the highlighter thread for its colours and shows
plain monospace until they arrive, as one change to that `Text`'s runs. Diffs (`diff`,
`patch`) are coloured by the first character of each line, and terminal output by its
escape codes; both are linear scans over the shown lines and are done in place.

A browser build has no worker thread, so it has no highlighter: code there is plain
monospace. Diffs and terminal colours still work.

The grammar scopes are mapped to the code colour roles (`SyntaxKeyword`, `SyntaxString`,
`SyntaxComment` and the rest) by one table in `src/highlight.rs`. The highlighter's own
themes are not used.

## Choices

### Parser: pulldown-cmark

- Complies with CommonMark and has GitHub tables, task lists and strikethrough.
- Gives a byte range for every event (`into_offset_iter`), which is what finding the
  boundary between a frozen block and the open one needs.
- A broken-link callback lets a parse that starts part way into a document resolve
  references defined earlier, which is how a reparse of the open block alone agrees with a
  parse of the whole.
- Pure Rust, used by rustdoc, MIT. Built with default features off, which drops the HTML
  writer and the command line binary.

comrak and markdown-rs were the alternatives. comrak builds an AST that would be walked a
second time; markdown-rs's positions are harder to use for restarting a parse.

### Highlighter: syntect with two-face, on fancy-regex

- syntect alone lacks Kotlin, Swift, TypeScript (and TSX), TOML and Dockerfile; two-face
  carries bat's grammar collection, which has all twenty-two languages this crate promises.
- The `syntect-fancy` feature selects the pure-Rust regex engine and the grammar dump built
  for it. There is no Oniguruma C library to cross-compile for iOS and Android.
- syntect is used through two-face's re-export so the two never disagree about the version.

tree-sitter was the alternative: better highlighting and incremental parsing, but every
language is C code to compile per platform, which makes the web build hard, and twenty
languages of it is a large binary.

### Questions the specification left open, and what was done

1. **A reference definition after its use.** The proposal was taken: the link is plain
   text while streaming and the block is built again once, at `finish()`.
2. **What counts as the open block.** A top-level block. A long list or table is one
   block until something follows it.
3. **Who owns the fold state.** The Host, in a `Signal<BTreeSet<usize>>` of code block
   positions that the application may own (`Markdown { expanded }`) so it survives a lazy
   list's window. Without one the document keeps its own.
4. **The web.** No highlighting there, keeping the rule that highlighting never runs on
   the UI thread without an exception.
5. **Table columns.** Each cell takes an equal share of the row (`weight`), as proposed.
6. **Headings for assistive technology.** Not addressed here: there is no semantics
   property in the schema to carry a heading level, and adding one is a schema change.
7. **Bare addresses.** pulldown-cmark does not link `https://...` written without angle
   brackets, so this crate does, the way GitHub does: `http://` and `https://` only, never
   inside code or an existing link, never right after `](` (which is a link still being
   written), and with trailing sentence punctuation left out.

## Licences

`licenses.txt` lists every crate an application links because of this one, with its licence;
`tests/licenses.rs` walks `Cargo.lock` and fails for any crate missing from it or under a
licence that is not permissive.

The grammars two-face bundles have licences of their own, and two-face records them; the
same test checks every one. For the twenty-two languages this crate promises:

| Grammar | Licence |
|---|---|
| Rust | MIT |
| Kotlin | Apache-2.0 |
| Swift | MIT |
| TypeScript, TSX | Apache-2.0 |
| TOML | MIT |
| Dockerfile | MIT |
| Java, C, C++, C#, Go, Python, JavaScript, JSON, YAML, HTML, CSS, Bash, SQL, Markdown, Diff, Ruby | Sublime Text default packages: "Permission to copy, use, modify, sell and distribute this software is granted", no conditions |

The rest of the collection is MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause and Unlicense,
except two files under WTFPL, a grant with no conditions at all: the GraphQL grammar and the
GitHub colour theme. Neither is used by this crate (it uses no themes, and GraphQL is not
one of its languages), but both are inside the bundled dump. The full texts are in two-face's
`generated/acknowledgements_full.md` and are available at run time through
`two_face::acknowledgement::listing()`.
