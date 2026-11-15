# HTML samples in a window, beside Chromium

What each HTML sample in `samples/` looks like in a dioxus-compose window on macOS, and what
the same HTML and CSS look like in Chromium, which is the reference for an HTML page.

- `macos/<sample>.png`: the sample's window after the 2026-11-15 HTML and renderer updates
  (integration of #194-#252 on compose-rust v0.0.10), and `macos-before/<sample>.png` the
  same window before them (#181). Both are (`cargo run -p sample-html-<sample>`), debug
  build, Apple Silicon, scale 1. The top 66 pixels are the window's title band, so the page
  starts at y = 66.
- `chromium/<sample>.html`: the sample's first render serialised as HTML, with its
  stylesheet in `<head>`. `tools/dump.rs` writes it from an `HtmlDom`.
- `chromium/<sample>.png`: that file in headless Chromium (playwright's
  chrome-headless-shell 1248), at the window's width and its height less the title band,
  scale 1, light colour scheme.

The differences between the two are tracked from issue #179.

## Making them again

1. `cargo build` each `sample-html-<name>` binary.
2. `tools/shotall.sh <dir> hello article ...` captures the windows. It photographs only
   the windows of the processes it starts, and stops them by process id.
3. `tools/dump.rs` is the `main.rs` of a throwaway crate that depends on the samples by
   path (with the workspace's blitz-dom patch); run it with an output directory.
4. `chrome-headless-shell --screenshot=<png> --window-size=<w>,<h-66>
   --force-device-scale-factor=1 --hide-scrollbars file://<html>`.

## The other targets

- `linux/<sample>.png`: the whole 1440x900 Xvfb screen of a Docker container (the
  `ember-linux-gui` image, arm64, openbox, Mesa software GL), release build linked
  against the published `linux-arm64` static renderer (captured 2026-11-13 against
  compose-rust v0.0.6). Openbox draws no title bar, so the window is exactly the size the
  sample asks for.
- `windows/<sample>.png`: the UTM window of a Windows 11 arm VM. The sample is an x64
  release build, cross-compiled on the Mac with `cargo xwin` against the published
  `windows-x64` renderer (captured 2026-11-13 against compose-rust v0.0.6), running under
  x64 emulation with WARP (`DXC_D3D12_WARP=1`), started through a scheduled task
  `/IT /RU ember` so it opens on the desktop. Windows taller than the VM's screen run off
  its top.
- `android/<sample>.png`: all 17 samples on a Galaxy Z Flip 4 (SM-F721N), main display
  (1080x2640), captured 2026-11-15 against compose-rust v0.0.10. Each sample's `cdylib`
  (`cargo build --target aarch64-linux-android --release --lib -p sample-html-<sample>`)
  is packaged into the `samples-android` APK as `lib/arm64-v8a/libmain.so`, signed with
  the Android debug keystore, launched with `am start -S -W`, and photographed with
  `screencap -p`. The Android captures on the Flip 4 show the One UI Edge Panel handle at
  `x` 1013..1080, `y` 536..878 (#277); ignore it when comparing.
- `web/<sample>.png`: all 17 samples in headless Chromium (chrome-headless-shell 1248) at
  each sample's window size, captured 2026-11-15 against compose-rust v0.0.10. Each page
  uses the v0.0.10 `samples-web` bundle (`renderer/web/build/dist/wasmJs/productionExecutable`
  from compose-rust tag `v0.0.10`, which streams and compiles `compose_rust_host.wasm` via
  `WebAssembly.instantiateStreaming` without needing special Chromium flags) with its
  `compose_rust_host.wasm` replaced by `sample_html_<sample>.wasm` (built for
  `wasm32-unknown-unknown` with `--import-memory`, `--global-base=4194304`,
  `--initial-memory=8388608`, `--no-stack-first`).

What differs between targets is tracked from #179: on Windows weight 600 at 28px
(compose-rust#551), and on Web `web/settings.png` draws the `<select>` dropdown arrow as a
missing-glyph box (#276).
