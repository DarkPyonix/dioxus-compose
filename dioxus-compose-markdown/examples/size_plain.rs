//! The size baseline: a window with one paragraph and no Markdown crate in its code.
//!
//! Built beside `size_markdown` so the difference between the two binaries is the crate.

use dioxus_compose::prelude::*;

fn app() -> Element {
    rsx! {
        Column {
            Text { text: "Hello from a plain window" }
        }
    }
}

fn main() {
    dioxus_compose::launch(app);
}
