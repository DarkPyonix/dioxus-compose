//! `Chrome::FullSize`, chosen through the prelude alone, reaching the window record.
//!
//! In a binary of its own: the launch choice lives in a process-wide slot.

use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::{Host, LaunchBuilder, LoopMode};

fn app() -> Element {
    rsx! { Text { text: "full size" } }
}

#[test]
fn fr19_8_full_size_chrome_reaches_set_window() {
    LaunchBuilder::new()
        .with_mode(LoopMode::Platform)
        .with_window(Window::new().chrome(Chrome::FullSize))
        .try_launch(app);

    let mut host = Host::new(app);
    let batch = host.rebuild().expect("the first tree encodes");
    let window = decode_batch(batch)
        .expect("the batch decodes")
        .into_iter()
        .find_map(|mutation| match mutation {
            Mutation::SetWindow(window) => Some(window),
            _ => None,
        })
        .expect("no window record in the first batch");
    assert_eq!(window.chrome, Chrome::FullSize);
}
