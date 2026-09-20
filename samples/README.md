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

Run one with the renderer present:

```
DIOXUS_COMPOSE_RENDERER_DIR=dioxus-compose-renderer/build/native-image/dist/lib \
  cargo run -p sample-calculator --features dioxus-compose/native-renderer
```

Pushing a `sample-v*` tag builds all of them for every desktop platform and attaches the
binaries to a GitHub Release.
