//! The same window as `size_plain`, with one Markdown message in it.
//!
//! Built beside `size_plain` so the difference between the two binaries is the crate.

use dioxus_compose::prelude::*;
use dioxus_compose_markdown::Markdown;

fn app() -> Element {
    rsx! {
        Column {
            Markdown { source: Some("# Hello\n\nA **bold** word and a `code` span.\n\n```rust\nfn main() {}\n```\n".to_string()) }
        }
    }
}

fn main() {
    dioxus_compose::launch(app);
}
