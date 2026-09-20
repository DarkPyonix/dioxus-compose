//! A working text editor.
//!
//! Written the way an application using this library would be written: `rsx!` and hooks,
//! no Kotlin and no protocol types. Reading and writing files happens on a worker thread
//! and comes back as a signal update, so the UI thread never waits on a disk.

use std::path::PathBuf;

use dioxus_compose::prelude::*;

mod document;

use document::{Counts, Outcome, counts, display_path};

/// The file the editor starts on, so the sample has something to open and save without a
/// file picker.
fn starting_path() -> PathBuf {
    std::env::temp_dir().join("dioxus-compose-notepad.txt")
}

/// The editor field.
///
/// A plain function rather than a `#[component]`, because `#[component]` expands to a
/// `#[derive(Props)]` and the derive macro is not exported to applications.
///
/// It is the raw `textfield` element rather than the `TextField` component, and the
/// contents arrive through a quoted attribute name, because neither the component nor the
/// declared attribute list offers a way to put text into a field. Opening a file has to.
///
/// `stamp` changes only when the text is replaced from outside the field, and the change of
/// key rebuilds the field so it picks the new contents up. Ordinary typing leaves `stamp`
/// alone, so the caret and the IME composition are never disturbed.
fn editor(stamp: u64, contents: String, on_edit: EventHandler<String>) -> Element {
    rsx! {
        for generation in [stamp] {
            textfield {
                key: "editor-{generation}",
                "text": "{contents}",
                multiline: true,
                placeholder: "Type here, or open a file",
                onvaluechange: move |event: Event<String>| on_edit.call((*event.data()).clone()),
            }
        }
    }
}

/// A single-line field whose contents are set once, for the path.
fn path_field(contents: String, on_edit: EventHandler<String>) -> Element {
    rsx! {
        textfield {
            "text": "{contents}",
            placeholder: "Path to a file",
            onvaluechange: move |event: Event<String>| on_edit.call((*event.data()).clone()),
        }
    }
}

fn app() -> Element {
    let mut text = use_signal(String::new);
    let mut stamp = use_signal(|| 0_u64);
    let mut path = use_signal(|| display_path(&starting_path()));
    let mut status = use_signal(|| "Ready".to_owned());
    let mut busy = use_signal(|| false);

    // The worker's result comes back here, on the UI thread, and the signal writes happen
    // where every other signal write in the app happens.
    let mut settle = move |outcome: Outcome| {
        busy.set(false);
        match outcome {
            Outcome::Opened {
                path: opened,
                text: contents,
            } => {
                status.set(format!(
                    "Opened {} ({} characters)",
                    display_path(&opened),
                    contents.chars().count()
                ));
                text.set(contents);
                path.set(display_path(&opened));
                // The field's contents came from outside it, so the field is rebuilt.
                stamp += 1;
            }
            Outcome::Saved { path: saved, bytes } => {
                status.set(format!("Saved {} ({bytes} bytes)", display_path(&saved)));
            }
            Outcome::Failed(message) => status.set(message),
        }
    };

    // One piece of file work: a real thread does the blocking call, a one-shot channel
    // carries the answer back, and the awaiting task applies it on the UI thread.
    let mut run_on_worker = move |work: Box<dyn FnOnce() -> Outcome + Send + 'static>| {
        busy.set(true);
        let (sender, receiver) = futures_channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = sender.send(work());
        });
        dioxus_core::spawn(async move {
            match receiver.await {
                Ok(outcome) => settle(outcome),
                Err(_) => settle(Outcome::Failed(
                    "The file worker stopped before it answered".to_owned(),
                )),
            }
        });
    };

    let Counts {
        words,
        characters,
        lines,
    } = counts(&text());
    let working = busy();

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            spacing: 6.0,
            TopAppBar {
                Text { text: "Notepad", type_role: TypeRole::Title }
            }
            Row {
                fill_max_width: true,
                spacing: 6.0,
                alignment: Alignment::CenterStart,
                Text { text: "File", type_role: TypeRole::Label }
                {path_field(path(), EventHandler::new(move |value| path.set(value)))}
                Button {
                    text: "Open",
                    variant: ButtonVariant::Tonal,
                    enabled: !working,
                    on_click: move |_| {
                        let target = PathBuf::from(path());
                        run_on_worker(Box::new(move || document::open(target)));
                    },
                }
                Button {
                    text: "Save",
                    variant: ButtonVariant::Filled,
                    enabled: !working,
                    on_click: move |_| {
                        let target = PathBuf::from(path());
                        let contents = text();
                        run_on_worker(Box::new(move || document::save(target, contents)));
                    },
                }
                Button {
                    text: "New",
                    variant: ButtonVariant::Outlined,
                    enabled: !working,
                    on_click: move |_| {
                        text.set(String::new());
                        stamp += 1;
                        status.set("New document".to_owned());
                    },
                }
            }
            Text {
                text: status(),
                type_role: TypeRole::Caption,
                color: Paint::Role(ColorRole::OnSurfaceVariant),
                max_lines: 1,
                overflow: TextOverflow::Ellipsis,
            }
            // The editor scrolls on its own, so a document longer than the window stays
            // reachable without the Host knowing where the scroll is.
            ScrollColumn {
                fill_max_width: true,
                fill_max_height: true,
                {editor(stamp(), text(), EventHandler::new(move |value| text.set(value)))}
            }
            Row {
                fill_max_width: true,
                arrangement: Arrangement::SpaceBetween,
                Text {
                    text: "{words} words",
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
                Text {
                    text: "{characters} characters",
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
                Text {
                    text: "{lines} lines",
                    type_role: TypeRole::Caption,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
            }
        }
    }
}

fn main() {
    dioxus_compose::launch(app);
}
