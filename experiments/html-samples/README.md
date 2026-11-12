# HTML samples in a window, beside Chromium

What each HTML sample in `samples/` looks like in a dioxus-compose window on macOS, and what
the same HTML and CSS look like in Chromium, which is the reference for an HTML page.

- `macos/<sample>.png`: the sample's window after the 2026-11-13 HTML fixes (integration of #194-#212), and `macos-before/<sample>.png` the same window before them (#181). Both are (`cargo run -p sample-html-<sample>`), debug
  build, Apple Silicon, scale 1, compose-rust v0.0.6. The top 66 pixels are the window's
  title band, so the page starts at y = 66.
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

Captured 2026-11-13 against compose-rust v0.0.6, from the samples as they are on develop
after #181, before any of the fixes tracked from #179.

- `linux/<sample>.png`: the whole 1440x900 Xvfb screen of a Docker container (the
  `ember-linux-gui` image, arm64, openbox, Mesa software GL), release build linked
  against the published `linux-arm64` static renderer. Openbox draws no title bar, so the
  window is exactly the size the sample asks for.
- `windows/<sample>.png`: the UTM window of a Windows 11 arm VM. The sample is an x64
  release build, cross-compiled on the Mac with `cargo xwin` against the published
  `windows-x64` renderer, running under x64 emulation with WARP (`DXC_D3D12_WARP=1`),
  started through a scheduled task `/IT /RU ember` so it opens on the desktop. Windows
  taller than the VM's screen run off its top.
- `android/<sample>.png`: a Galaxy Z Flip 4 (SM-F721N), main display, `dx build
  --platform android --release` with NDK 27.1. Only hello, dashboard and settings.
- `web/hello.png`: headless Chromium (chrome-headless-shell 1248) at 800x600. The page is
  the v0.0.6 `samples-web` bundle from compose-rust's Samples test run 43264492969, with
  its `compose_rust_host.wasm` replaced by `sample_html_hello.wasm` (built for
  `wasm32-unknown-unknown` with `--import-memory`, `--global-base=4194304`,
  `--initial-memory=8388608`, `--no-stack-first`). It only starts with
  `--enable-features=WebAssemblyUnlimitedSyncCompilation`, because the Host module is
  13.5MB and the loader instantiates it synchronously (compose-rust#565).

What differs between targets is tracked from #179: on Windows weight 600 at 28px
(compose-rust#551), on the web bold drawn regular (compose-rust#566), and the form fields
drawn in each platform's design system (#197, compose-rust#519).
