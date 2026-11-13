# Changelog

## Unreleased

### Everything compose-rust 0.0.7 added, written in rsx

Two widgets: `FlowRow`, a row that wraps (`horizontal_spacing`, `vertical_spacing`,
`horizontal_alignment`, `vertical_alignment`, `max_items_in_each_row`), and `Popover`, a box
attached to its `anchor` (`open`, `placement`, `align`, `offset` in dp, `scrim` or
`scrim_color`, `on_dismiss`). Every widget takes `semantics` (a `Semantics` role and
`SemanticsState`), `accessible_name` and `corner_curve`, and `AbsoluteBox` takes
`transform_3d`, CSS `matrix3d` with a backface flag. `Switch`, `Checkbox` and `Slider` take
their track, thumb, container and check colours and a `size`; `TextField` and `Dropdown`
take `bare`; `Dialog`, `Menu` and `Sheet` take `scrim`; `Navigation` takes `compact`; a
`window` element takes `report_size`. `CodeEditor` takes `line_numbers`,
`gutter_min_width`, `gutter_padding`, `text_inset`, `colors` (`CodeEditorColors`) and
`on_code_metrics` and `on_code_scrolled`, each reporting physical pixels. `TextField` takes
`on_focus_gained` and `on_key_up`, which brings the same `KeyEvent` as `on_key_down`, now
with its `repeat`, `code` and `text` on both. The scroll containers and the lazy lists take
`on_scroll_changed`, which brings a `ScrollChange` with every field of the report. A
handler left out is never announced to the renderer, and one taken away is withdrawn.

### HTML tables fill their width

A table whose width is greater than its columns, such as `width: 100%` in a wider
container, gives the extra width to its columns. When every column has a fixed width, each
one grows in proportion to it, as in Chromium's auto table layout. Before, the columns kept
their widths and the space past them stayed empty. Tables with `auto` or percentage columns
are unchanged. The change is a blitz-dom patch, `scripts/patches/blitz-dom-0.2.4/0003-table-width-fills-columns.patch`.

### List bullets are drawn as shapes

`list-style-type: disc`, `circle` and `square` are drawn as a filled circle, a 1px ring and
a filled square, as Chromium draws them, instead of the font's bullet character, which was
smaller. A display list entry carries the shape in `marker` (`MarkerShape`, with its
`MarkerSymbol`, rectangle and the item's colour), and the plan draws it as a box keyed
`PlanKey::Marker`. Its size and place come from the font's ascent: 5px across for a 16px
font, its left edge 16px before the item, level with the middle of the lowercase letters.
An inside bullet keeps its character's place in the text, drawn transparent, under the
shape. Numbered and lettered markers are still text.

### Content from the top of the window, and its safe area

`Window::new().chrome(Chrome::FullSize)` (the `Chrome` in the prelude is compose-rust's)
draws the content from the very top of the window, with the window buttons over it.
`use_window_safe_area()` returns `WindowSafeArea { top, leading, trailing }` in dp for the
calling component's window, as the renderer reports it, and redraws only the components
that called it when only the safe area changes. All three are zero on platforms whose
window is not ours. macOS comes first; checking it by eye on a real Mac is manual.

### Several shadows on Column, Row, Box and Card

These four take `shadow: Vec<ShadowLayer>`, a list in CSS order where the first layer is on
top. Each layer is a `Shadow` modifier in its own chain position, up to eight, drawn behind
the node. The shadow follows the node's corner radius once the renderer carries
compose-rust's change for it. The chain positions after the shadows moved up by seven;
nothing outside this crate depends on them.

### HTML screens zoom as a whole

An HTML screen is zoomed the way an editor zooms. The renderer reports `k`, the system's
text size times `1.2^level`, and the page is laid out in a viewport of the window's size
over `k`, drawn at `k`, and measured at `k`. A `width` media query sees that viewport and a
`resolution` query sees `k`. `html::launch` and every `HtmlView` do this, and
`use_zoom()` reads the factor and level and sets the level (`zoom_in`, `zoom_out`, `reset`,
`set_level`). `AbsoluteBox` takes `island_zoom`. The zoom starts at 1 until the renderer
reports one, which is where the web target stays. Checking a real VS Code at the same
zoom, and the system text size on each desktop, is manual. The renderer is pinned at the
compose-rust commit that has the zoom report and the island factor.

### Transforms and renderer-played animations from rsx

`AbsoluteBox` takes a CSS `transform` matrix with its `transform_origin`, or a Compose style
`graphics_layer` (rotation, scale, translation and opacity). `animate_float_as_state`,
`animate_color_as_state` and `animate_offset_as_state` give an `AbsoluteBox` an opacity, a
background or a translation that the renderer animates to whenever the target changes: a
change is one `StartAnimation` record, and the component is not rendered again while it
plays. `use_reduced_motion` reads the platform's reduced motion setting and re-renders when
it changes, and `use_animation_event` hears the animation events a screen asked for. These
need compose-rust's transform and animation records, so compose-rust is pinned at the
commit that has them (compose-rust #83).

### HTML and CSS screens reach the renderer

An HTML app is launched the way a widget app is: `dioxus_compose::html::launch(app)`, or
`LaunchBuilder::new().with_html(config).launch(app)` to pass a configuration.
`cargo run -p sample-html-hello` opens the hello sample in a window.

The bridge writes the drawing plan into compose-rust's batch. The first frame sends the
whole tree; after that only what the plan diff reports is sent, and a frame where nothing
changed sends nothing. Every kind of drawing element has a compose-rust widget to become.
Clicks, typed text, focus leaving a field, Enter in a field, checkbox and radio toggles and
select choices come back to the Dioxus handlers.

The records the renderer receives are tested on the Host. Drawing them on screen has not
been confirmed by running the renderer yet ([#43](https://github.com/DarkPyonix/dioxus-compose/issues/43)).
Text is still measured by Parley, and `font-family` does not reach the screen yet.
`font-style: italic` and `oblique`, and `text-decoration-line: underline` and `line-through`
alone or together, are sent as one span over the whole run of text, because compose-rust's
text style has no slant or lines of its own and its spans do. A box's lines are drawn across
the text inside it, as a browser draws them, so a link is underlined by the default
stylesheet. Plain text sends no span.

Keys and focus reach HTML elements as DOM events. Enter pressed in a field, or in a box
whose element listens for keys, delivers `keydown` and then `keypress`, bubbling from the
focused element; a handler that prevents the default keeps the field from submitting or
starting a line. A field delivers `focus` and `focusin` before the first thing the user does
in it, and `change`, `blur` and `focusout` when it loses the focus. Enter is the one key
compose-rust carries, it has no key release, and it reports a field losing the focus but not
gaining it, so `keyup` is never sent and `focus` arrives with the first keystroke.

`prefers-color-scheme` follows the theme: an application that switches its theme to light or
dark while running has its HTML screen restyled in the same frame, and only the boxes whose
style changed are sent. A theme that follows the system keeps the configured scheme, because
the renderer applies a system change without telling the Host.

The hello sample builds for Android, iOS and the web as well as the desktop, with the entry
points the native-widget samples have. `AbsoluteBox` and the `offset`, `required_size`,
`border_each`, `corner_each`, `shadow`, `clip` and `alpha` attributes can be written in a
widget `rsx!` app, as compose-rust's adapter writes them.

### The renderer is built for the platform where Compose publishes one

macOS no longer carries a Java runtime. Where Compose publishes a target of its own, the
renderer is compiled for the platform directly; where it does not, it stays a native image.
Nothing an application writes changes: the same C ABI, the same protocol bytes, the same
interpreter sources.

| | Before | Now |
| --- | --- | --- |
| A calculator on macOS | 93.1 MB across four files | **28.95 MB in one** |
| What it holds while open | 56 MB | **39 MB** |
| Threads | 22 | 10 |

For comparison, Apple's own Calculator holds 45 MB, and its heap is more than twice ours.

Linux and Windows keep the native image, and are lighter for a second reason: the toolkit
has been taken out of what the image is asked to keep. Nine ways of removing it were
measured; the six that worked are in the build, and the four that were worth nothing are
written down, because their failing is what found the cause.

`docs/platforms.md` has the measurements, what each platform can and cannot do, and why.

### Checked by hand on the macOS native path

The calculator draws, a screen reader is given every button by name, a press reaches the
scene, and the window is the one the application asked for.

## v0.0.0

The first published version. It is numbered 0.0.0 because the approach is proven and the
product is not: you can write a real application in Rust and watch it run, and you will
also run out of widgets partway through.

### What works, and was verified by running it

- **Declarative UI in Rust, drawn by Compose.** `rsx!`, hooks and signals on the Rust
  side; an AOT-compiled Compose Multiplatform renderer draws the pixels. No webview and
  no bundled JVM.
- **Korean IME.** Composition display, jamo-level backspace, arrow keys committing before
  moving, insertion mid-sentence and font fallback, all checked by hand on the macOS
  native image. This was the question the project existed to answer.
- **Accessibility.** VoiceOver reads the tree on the native image.
- **One boundary across two runtimes.** The desktop C smoke host links against the iOS
  static archive unmodified and produces the same handshake, byte for byte.
- **Four sample applications** that run: a calculator, a notepad, a task list over five
  thousand items, and a chat interface whose reply streams in from a worker thread.
- **Memory.** 60MB physical footprint for a running sample on macOS. The number most tools
  show is around 128MB, which counts shared read-only library pages that every other
  application on the machine is also counting.

### What is in the box

Twenty-three of the twenty-nine core widgets, custom drawing, pointer gestures, asset
delivery, six design systems behind one role-based contract, and a modern window that
draws its own title bar and follows the system colour scheme.

### What is not

- Six widgets are missing their renderer half: Checkbox, RadioButton, Switch, Slider,
  ProgressIndicator and Divider.
- Windows and Linux have never been run. The build scripts exist and CI now exercises
  them, which is how their first real bugs were found; neither has drawn a window yet.
- Android and web are designed and unimplemented.
- The design system contract does not yet cover pickers or selection controls, so those
  widgets cannot express what each system does differently.
- The samples work and do not yet look like applications, because until this version an
  application could not write a padding or a background from `rsx!`.

### Installing

The crate builds without a renderer, which is what a plain `cargo add` gets you: enough to
compile against, not enough to draw. Drawing needs the `native-renderer` feature and a
renderer built for your platform, which for now means building it yourself from this
repository. Release artifacts come later.
