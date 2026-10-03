# Samples

Eleven Dioxus apps written the way one is written for the web: HTML elements, CSS, hooks and
signals, with `use dioxus_compose::html::prelude::*` and nothing else from this crate. Each is
a crate of its own, and each exposes the same two things:

```rust
pub fn app() -> Element;     // the root component
pub const STYLE: &str;       // its stylesheet
```

so a window can show it and a test can lay it out without one.

| Sample | What it shows |
|---|---|
| `hello` | A heading, a paragraph with bold and italic words, and a link that counts how often it was clicked |
| `article` | A reading page: headings, wrapping paragraphs, ordered and unordered lists, a quotation and a code block |
| `dashboard` | A top bar, a sidebar of sections and a wrapping row of metric cards, all flexbox |
| `gallery` | A photo grid in CSS grid under a header painted with a gradient. Images are named by URL and never fetched: what draws them is the application's image resolver |
| `settings` | A form: a text field, a text area, a select, a checkbox, a radio group and a save button. The fields are uncontrolled, and `oninput` keeps what was typed until it is saved |
| `table` | Invoices in an HTML table, wider than the page on a narrow screen so it scrolls sideways inside its wrapper. The amount header sorts |
| `chat` | A conversation that scrolls above a composer pinned to the bottom, and an assistant reply that grows as its text arrives, appended through a context any worker can reach |
| `todo` | Add, tick off, delete and reorder items. Rows are keyed by id, so moving an item moves its row rather than rewriting every row's text |
| `overlay` | A dropdown menu positioned against its button, a confirmation dialog over a dimmed `position: fixed` backdrop, and a preview that clips its banner to rounded corners |
| `theme` | Every colour a CSS custom property: a light set, a dark set under `prefers-color-scheme: dark`, and a button that overrides the system's choice |
| `workbench` | A small code editor: an activity bar, a file tree whose folders open and close, tabs that grow to fit their names, and a status bar |

## Where they are drawn

The Host lays them out. `HtmlDom` runs the app, stylo resolves
the CSS, Taffy lays out blocks, flex and grid, and the result is a display list and a plan of
drawing elements: absolute rectangles, colours, borders, text runs, form fields, scroll
containers and images. Each sample's tests check that layout against numbers worked out by
hand, and click, type and submit through the same events a renderer would send.

The HTML bridge writes that plan into the renderer's batch, the whole tree on the first frame
and then only what changed, and the renderer's clicks, typing and choices come back to the
same handlers. `hello` has a binary that opens it in a window through the bridge:

```
cargo run -p sample-html-hello
```

Its library has `config()`, which hands `STYLE` to an `HtmlConfig`, and `launch()`, which calls
`LaunchBuilder::new().with_html(config).launch(app)`. The same builder goes to `android_main!`,
`web_main!` and, through `launch()`, `ios_main!`, so `hello` is a cdylib for an Android Activity
and a browser page and a staticlib for iOS, like the native-widget samples, with the same
`Dioxus.toml` and `ios/main.c`. The other samples expose the same `app()` and `STYLE` and are
launched the same way with `LaunchBuilder::with_html`; they have no binary of their own yet. The bridge is tested on the Host against the records the renderer receives
(`samples/hello/tests/hello.rs`). What a window shows has not been confirmed by running the
renderer yet ([#43](https://github.com/DarkPyonix/dioxus-compose/issues/43)).

## Running the tests

```
cargo test -p sample-html-hello
cargo test -p sample-html-hello -p sample-html-chat -p sample-html-todo
```

or all of them with the rest of the workspace, `cargo test --workspace`.

The tests measure text with a measurer whose answers can be worked out on paper: every
character is half the font size wide and every line 1.25 times the font size tall. It lives
in `support/` (the `sample-html-support` crate), a dev-dependency of the samples that use it,
so the numbers in every sample's tests come from the one measurer. `chat`, `todo`, `overlay`,
`theme` and `workbench` define their own measurers in their test files and do not depend on
it.

## The native-widget samples

The twelve crates that used to live here (`calculator`, `notepad`, `todo`, `chat`,
`minimal`, `store`, `statistics`, `selfcare`, `podcast`, `academic`, `social` and the
`frames` helper their tests shared), written with the Compose widgets (`Column`, `Text`, `Button`), are in
[compose-rust](https://github.com/DarkPyonix/compose-rust) with the tooling that built them
for Android, iOS and the browser and photographed them.
