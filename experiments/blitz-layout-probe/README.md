# How faithfully blitz-dom lays out the VS Code workbench

The plan behind this (GitHub issue #19): run `blitz-dom` headless inside the Rust Host
(Stylo for CSS, Taffy for layout, Parley for text, no window and no GPU), and hand its
computed boxes and text runs to the Kotlin renderer, so that VS Code's workbench UI can be
drawn through Compose with VS Code's own CSS. Two questions decide whether that plan holds:

1. How faithfully does blitz-dom lay out real workbench UI with the workbench's real CSS?
2. Do Parley's line breaks and widths match VS Code's closely enough that Compose can paint
   text at Parley's positions, or does Compose have to measure text itself?

Numbers are in `results.json` and `captured/compare-blitz.json`. The blitz-dom run was made
on 2026-10-03 by the session that merged this probe, on an Apple Silicon Mac at scale 1.

## The answer

**Question 1: blitz-dom lays out the boxes exactly, and every error it makes comes from text
width.** With the real workbench CSS:

| region | elements | within 0.5px | within 1px | within 10px | worst |
| --- | --- | --- | --- | --- | --- |
| activity bar | 29 | 29 | 29 | 29 | 0px |
| sidebar | 203 | 139 | 140 | 188 | 17.86px |
| editor tabs | 94 | 29 | 29 | 38 | 40.72px |

The activity bar holds no text and comes out pixel for pixel. In the sidebar and the tabs,
every element that is off is either a text run or a box whose position or size follows a
text run: the explorer title is 17.86px narrower, so the title actions start 17.86px early;
each tab label is 3 to 13px narrower, so each later tab starts further left, up to 40.72px
by the last one. No element is missing and none is shown or hidden differently. Stylo
dropped 508 declarations from the workbench stylesheet (`captured/blitz-css-errors.json`);
none of them moved a box in these three regions.

**Question 2: no. Parley's text is far outside FR-34's tolerances, and Compose has to
measure text.** Measured, not estimated:

- text runs in the sidebar: width error median 8.08px, worst 18.85px (`Explorer`, 56.86px
  in VS Code, 38.01px in blitz-dom); every one is narrower
- the 22 text cases: 6 break lines differently (2 of the 8 cases that wrap break the same
  way); single-line width differs by a median of 11.1% and up to 14.3%; the largest
  per-character x difference is 39.89px
- the probe's font report shows SF shaped with no variation coordinates (`coords []`), so
  Parley used the default optical size, as the source predicted below

FR-34 allows no line-break mismatch, 0.5px per glyph and 1px per line. These numbers miss
that by one to two orders of magnitude, so the condition written into D20 holds: text
measurement moves to Compose, and the boundary change that needs comes first.

What follows is why, from measurements that needed no build.

- The workbench font is the system UI font, `.SF NS` (`.SFNS-Regular`,
  `/System/Library/Fonts/SFNS.ttf`), which Chromium itself reported for the tab labels, the
  explorer rows and the sidebar title. It is a variable font with an optical size axis
  (`opsz` 17 to 96, default 28) and a tracking table (`trak`).
- Chromium picks the optical size from the font size. Parley 0.6 has no code that sets
  `opsz` (the word does not occur in its source, nor "optical"), and blitz-dom 0.2.4 passes
  only `font-variation-settings` through to it. So Parley shapes the workbench's 11px and
  13px text with the default instance, cut for 28px.
- That default instance, summed straight from the font's advance table with no kerning, is
  **4.7 to 13.0 percent narrower** than what VS Code drew for the same strings
  (`captured/sf-default-instance.json`):

  | string | size | VS Code | default instance | difference |
  | --- | --- | --- | --- | --- |
  | `README.md` | 13 | 76.03 | 70.47 | -7.3% |
  | `Button.tsx` | 13 | 61.69 | 55.14 | -10.6% |
  | `package.json` | 13 | 80.00 | 72.07 | -9.9% |
  | `Button.test.tsx` | 13 | 88.44 | 78.47 | -11.3% |
  | `EXPLORER` | 11 | 56.86 | 52.48 | -7.7% |
  | `The quick brown fox jumps over the lazy dog` | 11 | 234.11 | 203.59 | -13.0% |
  | the same | 13 | 270.13 | 240.61 | -10.9% |
  | the same | 20 | 388.66 | 370.17 | -4.8% |

  The gap shrinks as the size approaches the default optical size, which is what an
  optical size difference looks like and not what a shaping difference looks like.

This is not Parley's number. Parley adds kerning (which only narrows further) and may yet
do something with the tracking table, and the run below measures what it actually does.
But a 10 percent error in a 13px label is 7 to 10 pixels per tab, which no amount of
agreement elsewhere would rescue, and nothing in Parley's source would close it.

The fourth fact is about line height, and it is also measured. For `line-height: normal`,
Chromium rounds the font's ascent and descent separately and adds them: SF's are 1980 and
432 units in 2048, so a 13px line is 13 + 3 = 16px, an 11px line 11 + 2 = 13px and a 20px
line 19 + 4 = 23px, exactly the box heights in `captured/text.json`. blitz-dom maps
`normal` to 1.2 times the font size (`stylo_to_parley.rs`), so 15.6px, 13.2px and 24px.
The workbench sets explicit line heights on most rows (22px tree rows, 35px tabs), so this
matters less there than in running text, but any `normal` line is off by up to a pixel per
line before Parley has broken a single word.

## Method

### The stylesheet: rebuilt from Code-OSS, checked against the shipped one

Code-OSS (`microsoft/vscode`, MIT) at commit
**`7debcd0e2acdea1c52de81bf9ee1620444407dda`**, which is the commit of the VS Code build
installed on the machine (1.138.0, from its `product.json`), so the CSS and the DOM come
from the same source. It was fetched as a source tarball into `.scratch/`.

The workbench loads one stylesheet, `workbench.desktop.main.css`, which the Code-OSS build
(`build/next/index.ts`) makes by running esbuild over
`src/vs/workbench/workbench.desktop.main.ts` and concatenating every CSS file the module
graph imports, in JS evaluation order. There is no npm, no `node_modules` and no esbuild on
this machine, and installing them was not an option, so `scripts/extract-css.mjs` does the
same walk over the TypeScript sources: depth first, imports in source order, each module
once. The one thing it cannot do exactly is TypeScript's import elision (esbuild drops an
import whose bindings are only used as types); it approximates that from the text.

`scripts/compare-css.mjs` then checks the result against the stylesheet the installed
build ships. That file is only read for the check; nothing from it is fed to blitz-dom.

| | |
| --- | --- |
| CSS files in the rebuilt sheet | 357 |
| selectors in the shipped sheet | 11900 |
| of those, present in the rebuilt sheet | **11900 (all)** |
| selectors only in the rebuilt sheet | 24 (rules the minifier merged or dropped as empty) |
| source files found in the shipped sheet | 351 of 357 |
| of those, in the same relative order | 322 (29 out of order, 14 adjacent inversions) |

So the content is exact and the order is close. Order only matters where two rules of equal
specificity from different files set the same property on the same element. The remaining
inversions are in `list.css` against `contextview.css`, the hover widget, keybinding labels
and editor contributions; the editor tab files, which were out of order in an earlier pass,
are now in order. Whether any remaining inversion touches the three regions has not been
checked rule by rule; the fixture check below says it does not change their layout in
Chromium.

`codicon.ttf` is not in the Code-OSS tree: the build copies it out of the `@vscode/codicons`
npm package. Version `0.0.46-39`, the one Code-OSS's lock file names, was downloaded from
the npm registry and its integrity hash matched the lock file. The file icons use
`theme-seti`'s `seti.woff`, which is in the Code-OSS tree.

### The DOM and the reference geometry: read out of a running VS Code

VS Code was started with `--remote-debugging-port`, a fresh `--user-data-dir` and
`--extensions-dir` under `.scratch/`, and a small workspace (`src/components/button`,
`src/components/input`, `src/utils`, a few files) with five files open in tabs.
`scripts/capture-vscode.mjs` drives it over the Chrome DevTools Protocol: it expands every
folder in the explorer with real mouse clicks, moves the pointer away so nothing is
hovered, and then reads three regions:

| region | root | elements | text nodes |
| --- | --- | --- | --- |
| activity bar | `.part.activitybar` | 46 | 0 |
| sidebar with the explorer tree, four levels deep | `.part.sidebar` | 259 | 17 |
| editor tab strip, five tabs and the breadcrumbs | `.part.editor .title.tabs` | 122 | 7 |

For each it records every element's `getBoundingClientRect` and a set of computed
properties, and every text node's client rects. The region's `outerHTML` goes into
`fixtures/<region>.html`, wrapped in its real ancestor chain (each ancestor with its own
classes, attributes and inline style, without siblings), and every element carries a
`data-probe-id` so the two sides can be matched one to one. Every stylesheet the window
built at run time (theme colours as CSS variables, the icon fonts, list styles computed in
script) is saved as `captured/runtime.css`, so the fixtures carry the theme the window had
(the default dark theme of 1.138, `html lang="en"`).
That file is not committed: it is stylesheet text taken out of the installed Microsoft
build, whose licence is not Code-OSS's. `scripts/capture-vscode.mjs` writes it again on any
machine with VS Code installed, and the probe needs it present to run.

The window was 1440 by 900 CSS pixels at a device pixel ratio of 1. VS Code reported
Chromium 148.0.7778.280, Electron 42.10.0.

For text, `fixtures/text-cases.json` lists 22 strings: the tab and tree labels, the sidebar
titles, a pangram at 11, 13 and 20px and in semibold, digits, a kerning sample, Korean, and
eight paragraphs wrapped at widths from 120 to 300px. The capture lays each one out inside
the workbench element (so it inherits the workbench font) and records every character's
rect, from which line breaks and per-line widths follow.

### The fixtures are checked before blitz-dom sees them

A difference between blitz-dom and VS Code means something only if the static fixture,
laid out by Chromium, gives what the live window gave. `scripts/check-fixtures.mjs` replaces
the VS Code window's document with each fixture in turn and records the same geometry, and
`scripts/compare.mjs . chrome-fixture` compares it with the live capture:

| region | elements compared | equal to the live window (to 0.5px) | text nodes, equal |
| --- | --- | --- | --- |
| activity bar | 29 | 29 | none in the region |
| sidebar | 215 | 203 | 17 of 17 |
| tab strip | 94 | 94 | 7 of 7 |
| text cases | 22 | 22 (same breaks, 0px width difference) | |

The twelve sidebar elements that differ are the explorer header's action buttons (new
file, new folder, refresh, collapse), which the live window showed because the tree had
keyboard focus after the clicks, and which the static fixture hides because nothing is
focused. `compare.mjs` excludes elements the fixture does not reproduce when it compares a
candidate, and reports how many it excluded. Everything else matches to the hundredth of a
pixel: **the fixtures are an exact reproduction**, so the blitz-dom comparison measures
blitz-dom.

### blitz-dom: what the probe does

`src/main.rs` depends on the published crates (`blitz-dom =0.2.4`, `blitz-html =0.2.0`,
`blitz-traits =0.2.0`, and the `parley 0.6.0` and `stylo 0.8.0` blitz-dom resolves). For
each fixture it:

- parses the HTML with `HtmlDocument::from_html` at the same viewport and scale (1440 by 900,
  scale 1, dark colour scheme);
- serves the stylesheets and fonts from disk through a `NetProvider` that answers
  synchronously, maps the window's `vscode-file://` URLs onto the Code-OSS checkout, and
  hands back `codicon.ttf` from the npm package;
- loads every resource and calls `resolve`, timing both;
- writes every element's box from the unrounded layout (Chromium reports fractional
  positions, so the rounded layout would add half a pixel that is not blitz-dom's; the
  rounded one is written alongside). An element with no box of its own inside an inline
  formatting context (an ordinary `<span>`) gets the union of its glyph runs and inline
  boxes, which is what `getBoundingClientRect` gives in Chromium;
- writes every text node's rects, one per line, from the Parley glyph runs shaped in its
  parent's style span, sized to the run's ascent plus descent (Chromium's text rects are
  the font's content area, not the line box);
- for the text cases, writes each character's x and advance from Parley's clusters, the
  line it landed on, and each line's width without trailing whitespace;
- records the family name, size and variation coordinates of every font Parley shaped
  with, read from the font's own `name` table, so the result says which font blitz-dom
  actually used and whether `opsz` was set;
- parses both stylesheets again through stylo with an error reporter attached and counts
  every declaration and rule stylo drops, by property or by rule. blitz-dom parses without
  a reporter, so without this nothing would say which CSS it ignored.

`scripts/compare.mjs . blitz` then compares element by element (position, size, the worst
fifteen with their classes) and text by text, and `scripts/assemble-results.mjs` folds it
into `results.json`.

## What the source already says blitz-dom will get wrong

These come from reading blitz-dom 0.2.4 and Parley 0.6.0, not from running them. They are
here so the run can be read against them, and the run is what settles each one.

- **`text-overflow: ellipsis` is not implemented.** Nothing in blitz-dom reads it. Every
  tab label and tree row in the workbench uses it, so wherever a label is clipped Compose
  would have to elide the text itself.
- **WOFF 1 fonts are skipped.** `fetch_font_face` refuses `Woff` along with SVG and EOT
  fonts, even though the decompressor for it is compiled in. The file icons come from
  `seti.woff`, so blitz-dom has no file icon glyphs. Their box is a fixed 16 by 22px, so
  layout is unaffected; painting would need the icons from somewhere else.
- **`line-height: normal` is 1.2 times the font size** rather than the font's metrics
  (above).
- **No optical sizing** (above), and font features are always empty.
- Stylo in servo mode does not parse `-webkit-` properties the workbench uses for Electron
  (`-webkit-app-region` and similar). They do not affect layout. The real list, with
  counts, is what `captured/blitz-css-errors.json` will hold.

## Running the blitz-dom half

The inputs are under `.scratch/blitz-layout-probe/` of the worktree this was written in:
the Code-OSS checkout (`vscode/`), the codicons package (`codicons/`), and the rebuilt
stylesheet (`out/workbench.css`). The commands to recreate them are under "Reproducing".

From `experiments/blitz-layout-probe/`:

```sh
CARGO_BUILD_JOBS=2 cargo run --release -- \
  ../../.scratch/blitz-layout-probe/vscode \
  ../../.scratch/blitz-layout-probe/codicons/package/dist/codicon.ttf
```

It should print one line per fixture, in this order, and then the stylo and font summary:

```
activitybar: 46 records, parse and load … ms, resolve … ms, 0 resource errors
sidebar: 259 records, parse and load … ms, resolve … ms, 0 resource errors
tabs: 122 records, parse and load … ms, resolve … ms, 0 resource errors
text: 22 records, parse and load … ms, resolve … ms, 0 resource errors
css dropped by stylo: workbench.css <n>, runtime.css <n>
fonts in activitybar: {…}
fonts in sidebar: {"<family> / <postscript name> @ 13px coords […]": <runs>, …}
fonts in tabs: {…}
fonts in text: […]
```

and write `captured/{activitybar,sidebar,tabs,text}.blitz.json`,
`captured/blitz-css-errors.json` and `captured/blitz-meta.json`. Then:

```sh
node scripts/compare.mjs . blitz captured/compare-blitz.json
node scripts/assemble-results.mjs . "$(sysctl -n vm.loadavg)"
```

The first prints, per region, how many elements were compared, how many the fixture check
excluded (12 in the sidebar, 0 elsewhere), the distribution of the largest per-element
error (median, p90, p99, max), how many elements are within 0.5, 1, 2, 5 and 10px, and the
same for text; then for the text cases, how many have the same line breaks as VS Code and
the width difference of the single-line strings in pixels and percent. The second writes
`results.json` with those numbers in the `blitz` section, which says "not run" until then.
The timings are only meaningful beside the load average the second command records: the
machine this was written on was at a load average of about 140.

Things to read in the output before writing the answer to question 1: the `worst` lists
(each names the element and its live and blitz boxes), `hiddenInCandidateOnly` (elements
blitz-dom did not lay out at all), `elementsMissing` (should be 0), and
`captured/blitz-css-errors.json` for which of the dropped declarations belong to rules that
match the regions. For question 2: the `fonts` lines (whether the family is `.SF NS`, and
whether `coords` is empty, which means the default instance), and `text.rows`.

## What failed

- **A git checkout of Code-OSS was refused by the agent's sandbox** (git may only target
  the worktree itself), so the source was fetched as the GitHub tarball of the same commit.
  The content is the same; there is just no `.git` inside it.
- **The first VS Code launches died at once**: the main process puts a Unix socket in the
  user data directory, and the worktree path made the socket path longer than the 104
  bytes macOS allows. The capture used a short symlink to the same directory
  (`<repository>/.scratch/blp`), removed afterwards. Running the binary through a symlink
  also fails (Electron cannot find its helper app), so it has to be started with `open -n -a`.
- `document.write` into the workbench page is blocked by its Trusted Types policy; the
  fixture check bypasses the page's content security policy over the DevTools Protocol and
  reloads once first.
- The first CSS reconstruction had 34 file order inversions, including the editor tab
  files; teaching the import walk to tell a type position from a value position (a `:` in
  `a ? b : c` is not a type annotation, `f(x)` after a `:` is a value) brought it to 14.

## What has not been done

- The probe was written by an agent that was not allowed to build, then compiled and run
  unchanged by the session (it compiled on the first try). The timings in `results.json`
  were taken at a load average of { 6.49 4.26 2.67 }, so they say nothing about what
  blitz-dom costs; only the geometry is meant to be read.
- Which of the 508 dropped declarations matter elsewhere in the workbench is not checked;
  in these three regions none moved a box.
- The import walk's order is not proven exact: 29 of 351 files differ in relative order from
  the shipped sheet. The fixture check shows the order does not matter for these three
  regions in Chromium; it does not prove the same for every other part of the workbench.
- Parley's handling of the font's tracking table has not been read or measured. The default
  instance estimate above leaves tracking and kerning out entirely, which is why it is an
  estimate and not Parley's number.
- Whether Compose's own text measurement matches Chromium here is not measured either. If
  the answer to question 2 is "Compose measures", that is the next number to get, because
  it is the one the renderer would then depend on.
- Only one theme, one window size, one scale (1, on this display), English UI and one
  machine. A device pixel ratio of 2 changes Chromium's rounding and should be run before
  any number here is used for a Retina display.

## Reproducing

Everything downloaded or generated lives under `.scratch/blitz-layout-probe/`, and nothing
is written outside the repository. From the repository root:

```sh
mkdir -p .scratch/blitz-layout-probe/out .scratch/blitz-layout-probe/codicons
curl -sSL -o .scratch/blitz-layout-probe/vscode.tar.gz \
  https://codeload.github.com/microsoft/vscode/tar.gz/7debcd0e2acdea1c52de81bf9ee1620444407dda
tar -xzf .scratch/blitz-layout-probe/vscode.tar.gz -C .scratch/blitz-layout-probe
mv .scratch/blitz-layout-probe/vscode-7debcd0e2acdea1c52de81bf9ee1620444407dda .scratch/blitz-layout-probe/vscode
curl -sSL -o .scratch/blitz-layout-probe/codicons/codicons.tgz \
  https://registry.npmjs.org/@vscode/codicons/-/codicons-0.0.46-39.tgz
tar -xzf .scratch/blitz-layout-probe/codicons/codicons.tgz -C .scratch/blitz-layout-probe/codicons

node experiments/blitz-layout-probe/scripts/extract-css.mjs .scratch/blitz-layout-probe/vscode \
  .scratch/blitz-layout-probe/out/workbench.css .scratch/blitz-layout-probe/out/css-order.json
node experiments/blitz-layout-probe/scripts/compare-css.mjs .scratch/blitz-layout-probe/out/workbench.css \
  "/Applications/Visual Studio Code.app/Contents/Resources/app/out/vs/workbench/workbench.desktop.main.css" \
  experiments/blitz-layout-probe/captured/css-reconstruction.json
```

The capture needs VS Code running with a short user data path. The workspace, the
`User/settings.json` (telemetry, updates, the welcome page, AI features and the secondary
sidebar off, previews off) and the fresh profile are under `.scratch/blitz-layout-probe/`.
`S` below is a short path inside the repository that points at that directory; the
recorded capture used a symlink in the main checkout's `.scratch/`, which is short enough
where a worktree's own path is not.

```sh
S=/Volumes/macMini/darkpyonix/dioxus-compose/.scratch/blp
ln -sfn "$PWD/.scratch/blitz-layout-probe" "$S"
open -n -a "/Applications/Visual Studio Code.app" --args --remote-debugging-port=9339 \
  --user-data-dir="$S/vscode-user" --extensions-dir="$S/vscode-extensions" \
  --new-window "$S/workspace" "$S/workspace/README.md" \
  "$S/workspace/src/index.ts" "$S/workspace/src/utils/format.ts" \
  "$S/workspace/package.json" "$S/workspace/src/components/button/Button.tsx"
cd experiments/blitz-layout-probe
node scripts/capture-vscode.mjs 9339 . ../../../.scratch/blitz-layout-probe/out/workbench.css
node scripts/check-fixtures.mjs 9339 . ../../.scratch/blitz-layout-probe/out/workbench.css
node scripts/compare.mjs . chrome-fixture captured/compare-chrome-fixture.json
python3 scripts/sf-default-instance-widths.py .
node scripts/assemble-results.mjs .
```

`check-fixtures.mjs` leaves the window showing the last fixture; quit VS Code afterwards
(the DevTools `Browser.close` command does it cleanly) and remove the symlink.

## Files

- `scripts/extract-css.mjs`: rebuilds `workbench.desktop.main.css` from Code-OSS.
- `scripts/compare-css.mjs`: checks it against a shipped build.
- `scripts/capture-vscode.mjs`: reads the DOM, the geometry and the run-time stylesheets out
  of a running VS Code, and writes the fixtures.
- `scripts/check-fixtures.mjs`: lays the fixtures out in VS Code's own Chromium.
- `scripts/compare.mjs`: compares any candidate's geometry with the live capture.
- `scripts/sf-default-instance-widths.py`: the default instance width estimate.
- `scripts/assemble-results.mjs`: writes `results.json`.
- `src/main.rs`: the blitz-dom probe.
- `fixtures/`: the three regions and the text cases as static HTML, and the text case list.
- `captured/`: the live geometry (`<region>.json`, `text.json`, `meta.json`), the run-time
  stylesheet, the fixture check, the stylesheet check, the default instance estimate, and,
  after the run, blitz-dom's output.
