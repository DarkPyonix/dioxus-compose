# dioxus-compose

[![Test](https://github.com/DarkPyonix/dioxus-compose/actions/workflows/test.yml/badge.svg)](https://github.com/DarkPyonix/dioxus-compose/actions/workflows/test.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/DarkPyonix/dioxus-compose/blob/main/LICENSE)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

**English** · [한국어](https://github.com/DarkPyonix/dioxus-compose/blob/main/docs/locales/README_ko.md)

**A Dioxus renderer that draws HTML and CSS natively with Compose Multiplatform through compose-rust, with no webview.**

*No webview and no bundled JVM.*

```rust
use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;

pub const STYLE: &str = r#"
.page { padding: 40px 48px; }
h1 { margin: 0 0 16px; font-size: 32px; line-height: 40px; }
"#;

pub fn app() -> Element {
    let mut hellos = use_signal(|| 0u32);

    rsx! {
        main { class: "page",
            h1 { "Hello, Dioxus" }
            a {
                href: "#",
                onclick: move |event| {
                    event.prevent_default();
                    hellos += 1;
                },
                "Say hello"
            }
        }
    }
}
```

This is [`samples/hello`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/hello),
trimmed. **It is laid out today and not drawn on screen yet**
([#43](https://github.com/DarkPyonix/dioxus-compose/issues/43)): the Host computes the layout and a drawing plan in Rust, and the bridge that sends the
plan to the renderer waits for compose-rust's drawing elements. [Why, and what it waits
for](#why-html-screens-are-not-drawn-yet).

You write a Dioxus app the way you would for the web: `rsx!` with `div`, `span` and CSS, hooks
and signals. `blitz-dom` computes styles and layout inside the Host, and
[compose-rust](https://github.com/DarkPyonix/compose-rust)'s AOT-compiled Compose renderer is what
draws the boxes and the text, with Compose's text layout and its platform IME.

The same crate also takes Compose widget names in `rsx!` (`Column`, `Text`, `Button`) as the second
way to write a screen. That path draws through the renderer today.

---

## Contents

- [Why this exists](#why-this-exists)
- [Status](#status)
- [Why HTML screens are not drawn yet](#why-html-screens-are-not-drawn-yet)
- [Two ways to write a screen](#two-ways-to-write-a-screen)
- [Getting started](#getting-started)
- [Samples](#samples)
- [How it works](#how-it-works)
- [Performance](#performance)
- [Repository layout](#repository-layout)
- [Contributing](#contributing)
- [Documentation](#documentation)
- [License](#license)

---

## Why this exists

Two problems meet here.

**Web-stack desktop apps are heavy.** For an application that stays open all day, the memory
footprint and the download size are the problem, not responsiveness. Embedding a browser, or
shipping a JVM beside your app, costs tens to hundreds of megabytes before your own code runs.

**Rust has no toolkit with Compose-grade text.** Rust GUI toolkits render well, but text shaping,
selection, accessibility and above all input methods are not yet where Compose is. For an app where
typing is the interface, an IME that works 90% of the time is an app that does not work. Korean,
Japanese and Chinese composition is not optional.

So dioxus-compose keeps what apps are already written in, Dioxus and CSS, and borrows Compose for
the pixels, the text and the input method. The Compose side is compiled ahead of time into a native
library, so there is no JVM in what you ship.

### What it weighs

The notepad sample (a widget-path app,
compose-rust's [`samples/notepad`](https://github.com/DarkPyonix/compose-rust/tree/develop/samples/notepad)),
built for release and stripped. One executable: no runtime beside it and no virtual machine inside
it.

| Platform | Executable | Physical footprint | How the renderer is built |
|---|---|---|---|
| macOS (arm64) | **28.76 MB** | **35.1 MB** | Kotlin/Native, drawing through Metal |
| Linux (x86-64) | **37.93 MB** | not yet measured | Kotlin/Native, drawing through GLX |
| Windows | not yet measured | not yet measured | GraalVM native image, drawing through Direct3D 12 |

A webview stack carries a browser engine, and a bundled JVM alone is about 80 to 120 MB after
jlink; a pure-Rust toolkit is about 10 to 20 MB. Those are rough orders of magnitude, not
measurements of this project.

### What it will not do

- **No webview.** No WKWebView, WebView2 or WebKitGTK, no Tauri or wry.
- **No bundled JVM.** The desktop renderer is native code.
- **No hand-written boundary glue.** The shims between Rust and Kotlin are generated from one Rust
  schema.
- **UI is written in Rust.** Kotlin is how the renderer is built, not where features go.
- **Compose-grade text and IME, never bypassed.** No code path goes around Compose's platform text
  input.

---

## Status

**0.0.x is an early release stage.** dioxus-compose 0.0.1 on crates.io has both paths and depends
on compose-rust 0.0.1. HTML screens are laid out and planned but not yet drawn (the renderer bridge
is written and waits for compose-rust, see below). The API will change.

| State | Item |
|---|---|
| implemented | HTML and CSS layout in the Host with blitz-dom: block, inline, flexbox, grid, tables, positioning including `fixed`, overflow and scroll containers, borders, radii, shadows, opacity, backgrounds including gradients, images through an app-supplied resolver |
| implemented | HTML events and forms: clicks, `input`, `change` and `submit`, `select`, checkboxes and radio groups, uncontrolled fields, keyed lists, `text-transform` and `white-space` |
| implemented | Box layout within 1px of VS Code for 324 of 326 boxes in three workbench regions, with VS Code's text sizes (measured 2026-10-03, macOS) |
| implemented | Fifteen HTML and CSS examples in [`samples/`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples), each tested on the Host |
| implemented | Compose widgets in `rsx!`, drawn by the renderer: macOS, Android and the web end to end; Windows, Linux and iOS build and start |
| partial | Drawing HTML screens on screen: the plan exists and the bridge to compose-rust's renderer is written in [#67](https://github.com/DarkPyonix/dioxus-compose/pull/67) (tests pass), and it waits for compose-rust's drawing elements in [compose-rust #74](https://github.com/DarkPyonix/compose-rust/pull/74), not merged, then for compose-rust 0.0.2 ([#43](https://github.com/DarkPyonix/dioxus-compose/issues/43); [why](#why-html-screens-are-not-drawn-yet)) |
| partial | Text measured by Compose: the measure call is being implemented in compose-rust; Parley measures text until then ([#43](https://github.com/DarkPyonix/dioxus-compose/issues/43)) |
| planned | CSS transforms beyond `translate`, CSS transitions and animations ([#46](https://github.com/DarkPyonix/dioxus-compose/issues/46), [#47](https://github.com/DarkPyonix/dioxus-compose/issues/47)) |
| planned | Zooming HTML screens the way VS Code does |
| planned | HTML and widgets on one screen |
| planned | The Markdown crate `dioxus-compose-markdown` ([#24](https://github.com/DarkPyonix/dioxus-compose/issues/24); draft [#68](https://github.com/DarkPyonix/dioxus-compose/pull/68)) |

The [status page](http://darkpyonix.dev/dioxus-compose/en/status.html) of the guide has the full
list.

### Why HTML screens are not drawn yet

The Host lays HTML screens out and plans the drawing today, and tests cover both. What is missing is
the last step, sending the plan to compose-rust's renderer. This is where each part stands.

- The bridge is written in [#67](https://github.com/DarkPyonix/dioxus-compose/pull/67), and its tests pass.
- It needs compose-rust's FR-42 drawing elements: `AbsoluteBox` and the per-side border, per-corner
  radius, shadow, clip, alpha, offset and size modifiers. They are in [compose-rust #74](https://github.com/DarkPyonix/compose-rust/pull/74), which is not merged.
- The renderer half of the `motion` sample (rotate, scale and skew, and the transitions and
  animations the renderer plays) needs compose-rust's FR-41. It is in [compose-rust #83](https://github.com/DarkPyonix/compose-rust/pull/83), which is not merged.
- Animating layout properties such as width ([#47](https://github.com/DarkPyonix/dioxus-compose/issues/47)) is advanced by the Host each frame. It needs
  nothing from compose-rust beyond the bridge.

The plan, decided by the owner: compose-rust 0.0.2 is published on crates.io with #74 and #83, then
[#67](https://github.com/DarkPyonix/dioxus-compose/pull/67) moves to depend on compose-rust =0.0.2 and merges when CI is green. From that release, HTML
samples draw in a window. There is no date, because the plan waits on those two merges and that
release.

---

## Two ways to write a screen

| You write | Import | Today |
|---|---|---|
| HTML elements and CSS: `div`, `span`, `input`, a stylesheet | `dioxus_compose::html::prelude::*` | Laid out and planned in the Host, not drawn yet |
| Compose widget names: `Column`, `Text`, `Button` | `dioxus_compose::prelude::*` | Drawn by the renderer |

Both are always in the crate; no feature flag turns either off. An `rsx!` block uses one vocabulary
or the other, because each prelude names its own elements. Mixing them on one screen is planned.

A widget-path app, from
[`dioxus-compose/examples/desktop_demo.rs`](https://github.com/DarkPyonix/dioxus-compose/blob/main/dioxus-compose/examples/desktop_demo.rs),
trimmed:

```rust
use dioxus_compose::prelude::*;

fn app() -> Element {
    let mut messages = use_signal(Vec::<String>::new);
    let mut draft = use_signal(String::new);

    rsx! {
        Column {
            fill_max_width: true,
            Text { text: "dioxus-compose chat" }
            for message in messages() {
                Text { text: message }
            }
            TextField {
                placeholder: "Write a message",
                multiline: true,
                on_value_change: move |value| draft.set(value),
                on_key_down: move |event: KeyEvent| {
                    if event.key() == Key::Enter && !event.shift_key() {
                        let message = draft().trim().to_owned();
                        if !message.is_empty() {
                            messages.write().push(message);
                            draft.set(String::new());
                        }
                        event.consume();   // the renderer's onKeyEvent returns true
                    }
                }
            }
        }
    }
}

fn main() {
    dioxus_compose::launch(app);
}
```

Key events never reach Rust while an IME is composing, so `Enter` during Korean composition commits
the syllable instead of sending the message. `event.consume()` tells the renderer the key was
handled, the way `preventDefault()` does on the web.

---

## Getting started

Add the crate from crates.io:

```bash
cargo add dioxus-compose@0.0.1
cargo add dioxus-hooks@0.7 dioxus-signals@0.7
```

Lay a page out and look at what it would draw:

```rust
use dioxus_compose::html::prelude::*;

fn main() {
    let mut dom = HtmlDom::with_config(app, HtmlConfig {
        stylesheets: vec![STYLE.to_string()],
        ..HtmlConfig::default()
    });
    let list = dom.layout(800.0, 600.0, 1.0);
    for entry in &list.entries {
        println!("{:<6} {:?}", entry.tag, entry.rect);
    }
}
```

`cargo run` prints the boxes. No window opens yet, because nothing draws HTML screens until the
renderer bridge lands. A widget-path app calls `dioxus_compose::launch(app)` instead and opens a
window. The renderer is a prebuilt native library that
[compose-rust](https://github.com/DarkPyonix/compose-rust)'s build script downloads for your target;
building it yourself is covered there.

The examples' tests run with cargo in a checkout:

```bash
cargo test -p sample-html-hello
cargo test --workspace
```

The [getting started page](http://darkpyonix.dev/dioxus-compose/en/getting-started.html) walks
through the same steps with clicks and forms.

---

## Samples

Fifteen Dioxus apps in [`samples/`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples), written the way one is written for the
web: HTML elements, CSS, hooks and signals. Each is one crate, and each is tested on the Host against
hand-worked layout. The guide quotes them chapter by chapter. The last four (`layout`, `landing`,
`typography` and `motion`) are on `develop`, so their links point there.

| Sample | What it shows |
|---|---|
| [`hello`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/hello) | A heading, inline bold and italic, a link that counts its clicks |
| [`article`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/article) | Headings, wrapping paragraphs, lists, a quotation, a code block |
| [`dashboard`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/dashboard) | A top bar, a sidebar and wrapping metric cards, all flexbox |
| [`gallery`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/gallery) | A CSS grid of photos under a gradient header |
| [`settings`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/settings) | A form: text field, text area, select, checkbox, radio group |
| [`table`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/table) | An HTML table that scrolls sideways; the amount header sorts |
| [`chat`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/chat) | A scrolling conversation above a composer pinned to the bottom |
| [`todo`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/todo) | Add, tick, delete and reorder keyed rows |
| [`overlay`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/overlay) | A dropdown menu, a dialog over a fixed backdrop, a clipped preview |
| [`theme`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/theme) | Colours as CSS custom properties, a dark set, an override button |
| [`workbench`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/workbench) | A small code editor: activity bar, file tree, tabs, status bar |
| [`layout`](https://github.com/DarkPyonix/dioxus-compose/tree/develop/samples/layout) | Flexbox (wrap, gap, grow, shrink, basis, alignment, order), grid (areas, `auto-fill`, spans, line placement), `sticky`, `z-index`, absolute in relative |
| [`landing`](https://github.com/DarkPyonix/dioxus-compose/tree/develop/samples/landing) | A responsive page: `@media` at phone, tablet and desktop widths, a centred container, a card grid, pricing, a footer, a gradient background, shadows and radii |
| [`typography`](https://github.com/DarkPyonix/dioxus-compose/tree/develop/samples/typography) | Font size, weight and style, line height, letter spacing, alignment, case, white space, word breaking, ellipsis, generated content, list markers, decoration |
| [`motion`](https://github.com/DarkPyonix/dioxus-compose/tree/develop/samples/motion) | Transforms with `transform-origin`, transitions, `@keyframes`, a width transition |

Samples are laid out, not drawn on screen yet. See [why](#why-html-screens-are-not-drawn-yet).

---

## How it works

```
your component (rsx! with div, span, CSS)
  -> Dioxus VirtualDom                 runs it
  -> blitz-dom document                Stylo resolves CSS, Taffy lays out
  -> DisplayList                       boxes, colours, text runs, fields, images
  -> Plan, and its diff                drawing elements; only what changed is sent
  -> Compose renderer (compose-rust)   draws (the bridge is written in #67, not merged)
```

**The renderer never sees CSS.** It gets rectangles, colours and runs of text. That keeps the
renderer small and the same for every app, and a CSS feature is added in Rust in one place.

**Text is measured by the engine that draws it.** The Host asks a `TextMeasurer` how big each run
is. Parley measures today; Compose will, through a synchronous call during layout, because Parley's
sizes differ from Chromium's enough to move line breaks.

**Fields are uncontrolled.** The renderer owns a field's text while the user types, and only
committed text reaches Rust, so input method composition never makes a round trip.

**The boundary is synchronous and on one thread**, the way JSI replaced React Native's bridge. Only
primitives, pointers and lengths cross it, in fixed-layout records generated from one Rust schema,
and nothing is queued or copied. Blocking work runs on worker threads that update signals and ask
for a frame.

**JavaScript never runs and nothing is fetched.** A `<script>` element is inert, and an image URL
draws whatever the app's `ImageResolver` says it should.

---

## Performance

The goal for the widget path is to be indistinguishable from the same screen written by hand in
Kotlin and Compose. Host-side numbers, measured on 2026-09-20 on a Mac mini (M1, 16 GiB, macOS
26.5.1), release build, recorded in
[`dioxus-compose/benches/baseline.json`](https://github.com/DarkPyonix/dioxus-compose/blob/main/dioxus-compose/benches/baseline.json):

| Measurement | Result |
|---|---|
| Click to dispatch, diff and encode | **13.3 µs** p99 |
| Encode 100 mutations | **2.9 µs** p99, no steady-state allocation |
| 100 appends into a 10,000-message conversation | **220 µs** p99 |

An HTML frame costs more, because layout runs in the Host. Laying out the VS Code sidebar fixture
again with nothing changed took **3.1 ms** p99 on 2026-10-03, measured on a busy machine and so an
upper bound. That is above the 0.5 ms the Host is allowed per interaction, and it is the next thing
to bring down on the HTML path.

---

## Repository layout

```
dioxus-compose/
├─ dioxus-compose/        # the crate: the Dioxus adapter and the HTML path
│  ├─ src/html.rs         #   dioxus_compose::html: HtmlDom, HtmlConfig, TextMeasurer, ImageResolver
│  ├─ src/dom, layout, paint  # blitz-dom document and events, layout pass, display list and plan
│  ├─ examples/           #   desktop_demo, the widget-path demo
│  └─ tests/              #   the crate's tests
├─ samples/               # fifteen apps written with HTML and CSS, one crate each
├─ experiments/           # probes worth keeping (on develop only, not on main)
├─ scripts/               # the quality gate and repository tooling
└─ docs/
   ├─ guide/              # the user guide site (English and Korean)
   └─ locales/            # this README in Korean
```

The renderer, its builds and the design systems live in
[compose-rust](https://github.com/DarkPyonix/compose-rust).

---

## Contributing

Issues and pull requests are welcome. Work happens on `develop`.

- A change comes with the test that would have caught its absence, and a bug fix starts with a test
  that reproduces the bug.
- `cargo test --workspace` runs everything; `./scripts/check.sh` adds formatting, Clippy with
  warnings denied and the benchmarks.
- One logical change per commit, with a subject such as `Feat: Add select to the HTML path`
  (`Feat`, `Fix`, `Refactor`, `Docs`, `Test`, `Chore`).

---

## Documentation

**Guide: <http://darkpyonix.dev/dioxus-compose/>**, in English and Korean. It covers getting
started, layout and CSS, text, forms and events, images, scrolling and overlays, theming, the
widget path, and the status of every part.

---

## License

[Apache License 2.0](https://github.com/DarkPyonix/dioxus-compose/blob/main/LICENSE).
