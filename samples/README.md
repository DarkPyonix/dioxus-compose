# Sample apps

Each of these is a real application written the way someone using this project would
write one: Rust only, `rsx!` and hooks, no Kotlin.

They exist to be used rather than admired. The widget vocabulary was chosen by reasoning
about what applications need, and reasoning is not evidence. Every place a sample has to
reach for an extension, work around a missing widget, or give up, is a finding about the
vocabulary, and belongs in an issue rather than in a workaround.

Each one is also clone coded from a real application, and the reference is part of the
requirement rather than a mood board: the pictures are in
`docs/references/design-systems/README.md` under "Sample Apps". The design follows the
platform the application is running on, and the layout follows the window size class, so
one declaration is a phone screen, a tablet screen and a desktop window.

| Sample | Clone coded from | What it exercises |
|---|---|---|
| `calculator` | The Windows, macOS and Deepin calculators | Dense button grids, `Row` and `Column` nesting, a memory register, numeric formatting, a tape that stands beside the keys or arrives in a sheet |
| `notepad` | The iOS and Windows memo applications | Multiline text editing, IME, file I/O from a worker thread, several open documents with the list beside the page or in a sheet |
| `todo` | A macOS task list and a mobile task scheduler | Lists with stable keys, checkboxes, filtering as a destination set, `LazyColumn` windowing over thousands of rows, file persistence from a worker thread |
| `chat` | Google Gemini | Conversations as a destination set, scrollback, a composer where Enter sends and Shift+Enter starts a new line, and a reply streamed in from a worker thread |

Each one is an ordinary dx project: a `Dioxus.toml`, an `assets/` directory and
`src/main.rs`, the layout `dx new` produces. That is deliberate, because a sample exists to
be copied, and an arrangement that only works inside this repository teaches nothing.

```
cd samples/calculator
dx serve
```

Building with cargo directly works too:

```
cargo run -p sample-calculator
```

That downloads the released renderer for the crate's version on first use. To run against a
renderer you have just built instead, name it. The path must be absolute: a build script runs
with the package directory as its working directory, not the workspace root.

```
DIOXUS_COMPOSE_RENDERER_DIR=$PWD/dioxus-compose-renderer/build/native-image/dist/lib \
  cargo run -p sample-calculator
```

Pushing a `sample-v*` tag builds all of them for every desktop platform and attaches the
binaries to a GitHub Release.

## Pictures

`scripts/sample-shots.sh` photographs every sample in every design system, both colour
schemes and the three window widths each of them changes shape at. It is two steps, because
the two halves of a screen live in two languages: the samples record the bytes their Host
would have sent, and the Renderer draws those bytes in a window of the size the recording's
name carries. Nothing is restated in Kotlin, so what comes out is what an application would
really have produced.

```
./scripts/sample-shots.sh /tmp/shots            # all of them
./scripts/sample-shots.sh /tmp/shots Todo-      # one sample, everywhere
```
