//! The vertical slice as an Android application.
//!
//! The same Rust source as `desktop_demo`, with one difference: there is no `main`.
//! Kotlin owns the process and the frame loop, so the root component is registered from
//! `JNI_OnLoad` through `android_main!`, and the generated JNI shims carry every call.
//!
//! Built as a cdylib and loaded by the Activity:
//! ```sh
//! dioxus-compose-renderer/android/scripts/build-host.sh
//! ```

use dioxus_compose::prelude::*;
use std::sync::Once;
use std::thread;
use std::time::Duration;

fn app() -> Element {
    let mut messages = use_signal(Vec::<String>::new);
    let mut draft = use_signal(String::new);

    rsx! {
        Column {
            fill_max_width: true,
            Text { text: "dioxus-compose chat" }
            for message in messages() {
                Text { text: message }
            }
            TextField {
                placeholder: "Write a message",
                multiline: true,
                on_value_change: move |value| draft.set(value),
                on_key_down: move |event: KeyEvent| {
                    if event.key() == Key::Enter && !event.shift_key() {
                        let message = draft().trim().to_owned();
                        if !message.is_empty() {
                            messages.write().push(message);
                            draft.set(String::new());
                        }
                        event.consume();
                    }
                }
            }
            Button {
                text: "Send",
                on_click: move |_| {
                    let message = draft().trim().to_owned();
                    if !message.is_empty() {
                        messages.write().push(message);
                        draft.set(String::new());
                    }
                }
            }
        }
    }
}

/// A Host worker thread, doing what every worker does.
///
/// It never calls a boundary function and never touches the UI thread. It asks the Host
/// for a frame, which reaches Compose through the generated upcall, and the JavaVM
/// attachment that upcall needs happens once, on the first request, and is never undone.
fn spawn_frame_requesting_worker() {
    // The root component runs again on every rebuild, so the worker starts once.
    static WORKER: Once = Once::new();
    WORKER.call_once(|| {
        thread::spawn(|| {
            loop {
                thread::sleep(Duration::from_millis(200));
                dioxus_compose::request_frame_from_worker();
            }
        });
    });
}

fn start() -> Element {
    spawn_frame_requesting_worker();
    app()
}

dioxus_compose::android_main!(start);
