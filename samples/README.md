# Sample apps

Each of these is a real application written the way someone using this project would
write one: Rust only, `rsx!` and hooks, no Kotlin.

They exist to be used rather than admired. The widget vocabulary was chosen by reasoning
about what applications need, and reasoning is not evidence. Every place a sample has to
reach for an extension, work around a missing widget, or give up, is a finding about the
vocabulary, and belongs in an issue rather than in a workaround.

| Sample | What it exercises |
|---|---|
| `calculator` | Dense button grids, `Row` and `Column` nesting, keyboard input, numeric formatting |
| `notepad` | Multiline text editing, IME, scrolling, file I/O from a worker thread |
| `todo` | Lists with stable keys, checkboxes, filtering, per-item state, `LazyColumn` windowing over thousands of rows, file persistence from a worker thread |
| `chat` | An LLM chat interface: scrollback, a multiline composer where Enter sends and Shift+Enter starts a new line, and a reply streamed in from a worker thread |

The four above are **adaptive**: they take whichever design system the platform picks, so
the same declaration is a Material 3 screen on Android and a Cupertino one on a Mac. The
ones below are **unified**. Each names one design system and draws the same way
everywhere, which is what an application does when the design is the product rather than
the platform's convention. All of them are rebuilds of published iOS designs, so all of
them name Cupertino.

| Sample | What it exercises |
|---|---|
| `minimal` | A playground: every control the schema has, all nine fills with the ink each carries, the type and corner ladders, and a `Canvas` drawing |
| `store` | A clothing shop: a `LazyRow` carousel, a category strip, a two-up grid, a product page with sizes and a stepper, and a bag that adds up |
| `statistics` | Charts: a dial and a week of costs drawn with `Canvas`, both painted entirely in roles, on a tinted page |
| `selfcare` | A mood picker whose answers are colours, faces drawn with `Canvas`, a chip grid, a windowing row of session cards and a week as a line |
| `podcast` | A player whose waveform is a `Canvas`, cover art built from the accent families, and a full-screen page that covers the navigation bar |
| `academic` | A grid of subject tiles, a stage strip, open and locked lessons, and the one place the role vocabulary ran out |

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
