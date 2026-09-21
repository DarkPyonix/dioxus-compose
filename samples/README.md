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

Each one is an ordinary dx project: a `Dioxus.toml`, an `assets/` directory and
`src/main.rs`, the layout `dx new` produces. That is deliberate, because a sample exists to
be copied, and an arrangement that only works inside this repository teaches nothing.

```
cd samples/calculator
dx serve
```

Building with cargo directly works too, and is what CI does, but the renderer has to be
found. The path must be absolute: a build script runs with the package directory as its
working directory, not the workspace root.

```
DIOXUS_COMPOSE_RENDERER_DIR=$PWD/dioxus-compose-renderer/build/native-image/dist/lib \
  cargo run -p sample-calculator --features dioxus-compose/native-renderer
```

## Seeing a system you are not running

Every sample takes its theme from `demo_theme()`, which reads two variables. An
application picks its own theme and never needs either of these; a sample does, because on
any one machine following the host would only ever show you one of the six systems and one
of its two colour schemes.

| Variable | Values | Default |
|---|---|---|
| `DXC_DESIGN` | `material3`, `cupertino` (also `apple`, `liquid-glass`, `liquidglass`), `fluent` | follows the host platform |
| `DXC_SCHEME` | `light`, `dark` | follows the system appearance |

```
DXC_DESIGN=apple DXC_SCHEME=dark cargo run -p sample-chat --features dioxus-compose/native-renderer
```

Pushing a `sample-v*` tag builds all of them for every desktop platform and attaches the
binaries to a GitHub Release.
