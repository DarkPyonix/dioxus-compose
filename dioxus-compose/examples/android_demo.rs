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
use std::thread;
use std::time::Duration;

/// A hundred appends a second, which is the streaming rate the frame budget is stated at.
const APPEND_INTERVAL: Duration = Duration::from_millis(10);

/// How long the streamed line grows before it starts over. Long enough to watch, short
/// enough that the screen does not fill with it.
const STREAM_LENGTH: usize = 60;

fn app() -> Element {
    let mut messages = use_signal(Vec::<String>::new);
    let mut draft = use_signal(String::new);
    let streaming = use_streaming_line();

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
            Text { text: streaming() }
        }
    }
}

/// A line a worker thread grows a hundred times a second.
///
/// The worker is what every Host worker is: it does its work off the UI thread, writes a
/// signal, and never calls a boundary function. Writing the signal marks the reading scope
/// dirty and the Host asks for a frame on its own, which on Android means the generated
/// upcall, the JavaVM attachment it needs, and Compose's frame clock. Typing and pressing
/// Send have to stay responsive while it runs.
fn use_streaming_line() -> Signal<String, SyncStorage> {
    let mut line = use_signal_sync(|| String::from("streaming "));
    use_hook(move || {
        thread::Builder::new()
            .name("android-demo-stream".to_owned())
            .spawn(move || {
                loop {
                    thread::sleep(APPEND_INTERVAL);
                    let mut text = line.write();
                    if text.len() >= STREAM_LENGTH {
                        text.clear();
                        text.push_str("streaming ");
                    } else {
                        text.push('.');
                    }
                }
            })
            .expect("the streaming thread could not be started");
    });
    line
}

dioxus_compose::android_main!(app);
