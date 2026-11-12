# HTML samples in a window, beside Chromium

What each HTML sample in `samples/` looks like in a dioxus-compose window on macOS, and what
the same HTML and CSS look like in Chromium, which is the reference for an HTML page.

- `macos/<sample>.png`: the sample's window (`cargo run -p sample-html-<sample>`), debug
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
