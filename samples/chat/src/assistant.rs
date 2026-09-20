//! The fake assistant.
//!
//! There is no network here. What matters is the shape of the work rather than where the
//! tokens come from: a reply arrives a few characters at a time, from a thread that is not
//! the UI thread, over a stretch of time long enough that the user is still scrolling and
//! typing while it lands.
//!
//! The worker appends to the shared message list and nothing else. It never calls a
//! boundary function: writing the signal marks the reading scope dirty, and the Host asks
//! the Renderer for the next frame on its own.

use dioxus_compose::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::Message;

/// How much text lands at once, and how long the gap is. Roughly 180 characters a second,
/// which is close to what a hosted model feels like and fast enough that a stall shows.
const CHUNK: usize = 3;
const GAP: Duration = Duration::from_millis(16);

/// Cancellation, shared with the UI thread.
///
/// Every reply is started with a token. Sending again bumps the counter, and a worker whose
/// token is no longer the current one stops at its next chunk instead of appending to a
/// message that is no longer the last one.
#[derive(Clone, Default)]
pub struct Turns(Arc<AtomicU64>);

impl Turns {
    pub fn begin(&self) -> u64 {
        self.0.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn is_current(&self, token: u64) -> bool {
        self.0.load(Ordering::Acquire) == token
    }
}

/// Reply text for a prompt. Canned, but picked from the prompt so the answers are not all
/// the same wall of words.
fn reply_to(prompt: &str) -> String {
    let lowered = prompt.to_lowercase();
    if lowered.contains("hello") || lowered.contains("hi ") || lowered.trim() == "hi" {
        return "Hello. I am a stand-in for a language model: there is no network behind \
me, only a worker thread handing you a few characters at a time so the streaming path \
gets exercised for real. Ask me anything and I will answer at length, because a short \
answer would not tell you whether scrolling stays smooth."
            .to_owned();
    }
    if lowered.contains("thread") || lowered.contains("stream") {
        return "Streaming works like this. The UI thread pushes your message and an empty \
reply, then hands the prompt to a worker. The worker sleeps, appends to the shared list, \
sleeps again. Writing the list marks the scope that reads it dirty, which wakes the \
scheduler, which asks for a frame. Nothing in the loop touches the boundary directly, and \
nothing in the loop runs on the UI thread except the diff of the one message that grew."
            .to_owned();
    }
    if lowered.contains("code") || lowered.contains("rust") {
        return "Here is what a handler looks like:\n\n    Button {\n        text: \"Send\",\n\
        on_click: move |_| send(draft()),\n    }\n\nThat block is plain text in a Text \
widget. There is no code block widget and no monospace span inside a paragraph, so the \
indentation is doing all the work that a background and a border would normally do."
            .to_owned();
    }
    format!(
        "You said: \"{prompt}\". I do not know what that means, because I am four lines of \
string matching wearing a trench coat. What I can do is keep talking long enough for you \
to try scrolling the history, selecting some of this text, and typing your next message \
while the rest of this sentence is still arriving, which is the only part of this sample \
that was ever in doubt."
    )
}

/// Start a reply. Returns immediately; the text arrives on `messages` over the next second
/// or so.
pub fn stream_reply(
    prompt: String,
    token: u64,
    turns: Turns,
    mut messages: SyncSignal<Vec<Message>>,
) {
    std::thread::Builder::new()
        .name("sample-chat-assistant".to_owned())
        .spawn(move || {
            let reply = reply_to(prompt.trim());
            // Chunk on character boundaries, never on bytes: a reply cut through the middle
            // of a multi-byte character would not be valid UTF-8 to push.
            let characters: Vec<char> = reply.chars().collect();
            // A beat before the first characters, the way a real model takes a moment.
            std::thread::sleep(Duration::from_millis(220));
            for chunk in characters.chunks(CHUNK) {
                if !turns.is_current(token) {
                    return;
                }
                let tail: String = chunk.iter().collect();
                {
                    let mut list = messages.write();
                    match list.last_mut() {
                        // The append is the point. Replacing the message instead would make
                        // the whole reply new text on every chunk.
                        Some(message) if message.token == token => message.text.push_str(&tail),
                        _ => return,
                    }
                }
                std::thread::sleep(GAP);
            }
            if turns.is_current(token) {
                let mut list = messages.write();
                if let Some(message) = list.last_mut() {
                    message.streaming = false;
                }
            }
        })
        .expect("the assistant thread could not be started");
}
