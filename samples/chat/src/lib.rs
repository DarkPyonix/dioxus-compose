//! A chat window: the conversation scrolls in the space above a composer row that stays at
//! the bottom, the user's messages on the right and the assistant's on the left, and the
//! assistant's reply grows as its text arrives.
//!
//! The conversation lives in a [`Chat`] the app provides as context, so whatever delivers
//! the reply (a network stream, a worker) appends to it with [`Chat::receive`], and the
//! composer reads it from there.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::{use_context, use_context_provider};
use dioxus_signals::{ReadableExt, Signal, WritableExt};

pub const STYLE: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; }
body { font-family: system-ui, sans-serif; font-size: 14px; color: #0f172a; background-color: #ffffff; }

.chat { display: flex; flex-direction: column; height: 100%; }

.messages {
    flex: 1;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
}

.message { max-width: 70%; padding: 8px 12px; border-radius: 12px; }
.message.user { align-self: flex-end; background-color: #2563eb; color: #ffffff; }
.message.assistant { align-self: flex-start; background-color: #f1f5f9; }

.composer { display: flex; gap: 8px; padding: 12px; border-top: 1px solid #e2e8f0; }
.composer input {
    flex: 1;
    min-width: 0;
    height: 36px;
    padding: 0 12px;
    border: 1px solid #cbd5e1;
    border-radius: 8px;
}
.composer button {
    width: 72px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background-color: #2563eb;
    color: #ffffff;
}
"#;

/// Who wrote a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sender {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: usize,
    pub from: Sender,
    pub text: String,
}

/// The conversation and the text being typed.
#[derive(Clone, Copy)]
pub struct Chat {
    pub messages: Signal<Vec<Message>>,
    /// What the composer holds. The field owns its text while the user types; every edit
    /// is copied here.
    pub draft: Signal<String>,
}

impl Chat {
    fn new() -> Self {
        let opening = [
            (Sender::Assistant, "Hello! How can I help?"),
            (Sender::User, "Plan a trip"),
            (Sender::Assistant, "Sure"),
        ];
        let messages = opening
            .into_iter()
            .enumerate()
            .map(|(index, (from, text))| Message {
                id: index + 1,
                from,
                text: text.to_string(),
            })
            .collect();
        Self {
            messages: Signal::new(messages),
            draft: Signal::new(String::new()),
        }
    }

    /// Sends what the composer holds as the user's message and empties the composer. Does
    /// nothing when it holds only white space.
    pub fn send(mut self) {
        let text = self.draft.read().trim().to_string();
        if text.is_empty() {
            return;
        }
        let mut messages = self.messages.write();
        let id = messages.len() + 1;
        messages.push(Message {
            id,
            from: Sender::User,
            text,
        });
        drop(messages);
        self.draft.set(String::new());
    }

    /// Appends a piece of the assistant's reply to the reply being written, the last
    /// message. A piece that arrives after the user has spoken starts a new reply.
    pub fn receive(mut self, chunk: &str) {
        let mut messages = self.messages.write();
        if let Some(last) = messages
            .last_mut()
            .filter(|last| last.from == Sender::Assistant)
        {
            last.text.push_str(chunk);
            return;
        }
        let id = messages.len() + 1;
        messages.push(Message {
            id,
            from: Sender::Assistant,
            text: chunk.to_string(),
        });
    }
}

/// The row at the bottom: a field for the next message and the button that sends it. It
/// is a form, so pressing Enter in the field sends as the button does.
#[allow(non_snake_case)]
fn Composer() -> Element {
    let chat = use_context::<Chat>();
    let mut draft = chat.draft;

    rsx! {
        form {
            id: "composer",
            class: "composer",
            onsubmit: move |event| {
                event.prevent_default();
                chat.send();
            },
            input {
                id: "draft",
                placeholder: "Message",
                value: "{draft}",
                oninput: move |event| draft.set(event.value()),
            }
            button { id: "send", r#type: "submit", "Send" }
        }
    }
}

pub fn app() -> Element {
    let chat = use_context_provider(Chat::new);
    let messages = chat.messages.cloned();

    rsx! {
        div { class: "chat",
            div { id: "messages", class: "messages",
                for message in messages {
                    div {
                        key: "{message.id}",
                        id: "message-{message.id}",
                        class: if message.from == Sender::User { "message user" } else { "message assistant" },
                        "{message.text}"
                    }
                }
            }
            Composer {}
        }
    }
}

/// What the window lays the page out with: the stylesheet, and text measured by the
/// renderer.
pub fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![STYLE.to_string()],
        ..HtmlConfig::default()
    }
}

/// Opens the page in a window. Does not return while it is open.
pub fn launch() {
    launch_builder().launch(app);
}

/// How the page starts, in one place because four entry points need it: the desktop
/// binary, an Android Activity, a browser page and an iOS bundle.
fn launch_builder() -> dioxus_compose::LaunchBuilder {
    dioxus_compose::LaunchBuilder::new()
        .with_window(
            dioxus_compose::schema::Window::new()
                .with_title("Chat")
                .with_size(480, 720),
        )
        .with_html(config)
}

// The platforms where the page is not a program: Android's Activity and the browser's page
// call an entry point that registers the root component, and iOS starts at a C `main`
// (ios/main.c) that hands over to `launch`. Each macro compiles into nothing off its own
// platform.
dioxus_compose::android_main!({ launch_builder() }, app);
dioxus_compose::web_main!({ launch_builder() }, app);
dioxus_compose::ios_main!(launch);
