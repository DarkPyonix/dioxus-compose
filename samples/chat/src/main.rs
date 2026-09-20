//! An LLM chat interface with no network.
//!
//! A scrollback of messages, a multiline composer where Enter sends and Shift+Enter starts
//! a new line, and an assistant whose reply arrives a few characters at a time from a
//! worker thread while the user keeps scrolling and typing.
//!
//! The scrollback is a `LazyColumn` so a long conversation costs widgets in proportion to
//! what is on screen. Each message carries an id that never changes and that id is its key,
//! so the message growing at the bottom does not disturb the ones above it.

mod assistant;

use assistant::Turns;
use dioxus_compose::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: u64,
    /// The turn this message belongs to. A stale worker compares it before appending.
    pub token: u64,
    pub from_user: bool,
    pub text: String,
    pub streaming: bool,
}

fn opening_messages() -> Vec<Message> {
    vec![Message {
        id: 1,
        token: 0,
        from_user: false,
        text: "Ask me something. I will answer slowly and at length, on a thread that is \
not this one. Enter sends; Shift+Enter starts a new line."
            .to_owned(),
        streaming: false,
    }]
}

fn app() -> Element {
    // Shared with the assistant thread, so it is a sync signal rather than the usual one.
    // Writing it from the worker marks this scope dirty through a channel the scheduler
    // owns, and the Host asks for the frame.
    let mut messages = use_signal_sync(opening_messages);
    let mut next_id = use_signal(|| 2_u64);
    let mut draft = use_signal(String::new);
    let turns = use_signal(Turns::default);

    let mut send = move |text: String| {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        let token = turns.peek().begin();
        let id = next_id();
        next_id.set(id + 2);
        {
            let mut list = messages.write();
            list.push(Message {
                id,
                token,
                from_user: true,
                text: text.clone(),
                streaming: false,
            });
            // The empty reply goes in now, on the UI thread, so the worker only ever has to
            // append to a message that already exists.
            list.push(Message {
                id: id + 1,
                token,
                from_user: false,
                text: String::new(),
                streaming: true,
            });
        }
        draft.set(String::new());
        assistant::stream_reply(text, token, turns.peek().clone(), messages);
    };

    let count = messages.read().len();
    let busy = messages
        .read()
        .last()
        .is_some_and(|message| message.streaming);
    let keys: Vec<String> = messages
        .read()
        .iter()
        .map(|message| message.id.to_string())
        .collect();

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            spacing: 8.0,

            Row {
                fill_max_width: true,
                spacing: 8.0,
                alignment: Alignment::CenterStart,
                Text { text: "Chat", type_role: TypeRole::Headline }
                Text {
                    text: if busy { "assistant is replying" } else { "ready" },
                    type_role: TypeRole::Label,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
                Button {
                    text: "New conversation",
                    variant: ButtonVariant::Text,
                    on_click: move |_| {
                        turns.peek().begin();
                        messages.set(opening_messages());
                        next_id.set(2);
                    },
                }
            }

            LazyColumn {
                item_count: count,
                key_of: move |index: usize| keys[index].clone(),
                item: move |index: usize| {
                    let message = messages.read()[index].clone();
                    rsx! {
                        // A `Spacer` would be the obvious way to separate one message from
                        // the next, but a Spacer can only be sized with a modifier and no
                        // modifier can be written from rsx, so the gap is the column's
                        // spacing instead.
                        Column {
                            fill_max_width: true,
                            spacing: 2.0,
                            Text {
                                text: if message.from_user { "You" } else { "Assistant" },
                                type_role: TypeRole::Label,
                                color: Paint::Role(if message.from_user {
                                    ColorRole::Primary
                                } else {
                                    ColorRole::Secondary
                                }),
                            }
                            Text {
                                // A message still arriving shows a caret so an empty reply
                                // does not look like a dead one.
                                text: if message.streaming {
                                    format!("{}\u{2589}", message.text)
                                } else {
                                    message.text.clone()
                                },
                                type_role: TypeRole::Body,
                            }
                        }
                    }
                },
            }

            Row {
                fill_max_width: true,
                spacing: 8.0,
                alignment: Alignment::CenterStart,
                // No `on_key_down` here on purpose. The Renderer already treats Enter in a
                // multiline field that has a submit handler as "send" and Shift+Enter as
                // "new line", and `on_submit` carries the text the field holds at that
                // instant. A key handler would have to read the separately reported value,
                // which lags typing by the field's change debounce, so the last characters
                // typed before Enter would be dropped.
                TextField {
                    multiline: true,
                    placeholder: "Message. Enter sends, Shift+Enter starts a new line",
                    on_value_change: move |value| draft.set(value),
                    on_submit: move |value: String| send(value),
                }
                Button {
                    text: "Send",
                    on_click: move |_| send(draft()),
                }
            }
        }
    }
}

fn main() {
    dioxus_compose::launch(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus_compose::Host;

    /// Every property this screen sets has to be one the wire can name. A property the
    /// schema does not have fails the whole batch rather than just itself, so a screen that
    /// builds in Rust can still be blank on screen.
    #[test]
    fn the_first_frame_encodes_without_a_protocol_error() {
        assert!(Host::new(app).rebuild().is_ok());
    }

    /// The reply has to arrive from the worker without anyone on the UI thread asking for
    /// it. Sending marks the scope that reads the message list dirty from another thread,
    /// and the next frame carries whatever has landed so far.
    ///
    /// This also pins down what the wire actually carries while a reply streams: today the
    /// growing message is resent whole on every frame, because the append command has no
    /// path out of `rsx!`.
    #[test]
    fn pr3_a_reply_arrives_from_the_worker_thread() {
        use dioxus_compose::protocol::{
            HostEvent, Mutation, PropertyValue, decode_batch, encode_event,
        };
        use dioxus_compose::{EventPayload, PropertyKind, WidgetKind};

        let mut host = Host::new(app);
        let first = decode_batch(host.rebuild().expect("the first frame failed to encode"))
            .expect("the first frame did not decode");
        let scrollback = first
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::Create {
                    node_id,
                    widget: WidgetKind::LazyColumn,
                } => Some(*node_id),
                _ => None,
            })
            .expect("the screen has no scrollback");
        let range_handler = first
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::SetProp {
                    node_id,
                    property: PropertyKind::OnRangeRequested,
                    value: PropertyValue::Integer(id),
                } if *node_id == scrollback => Some(*id as u64),
                _ => None,
            })
            .expect("the scrollback declared no range handler");
        let composer = first
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::Create {
                    node_id,
                    widget: WidgetKind::TextField,
                } => Some(*node_id),
                _ => None,
            })
            .expect("the screen has no composer");
        let handler = first
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::SetProp {
                    node_id,
                    property: PropertyKind::OnSubmit,
                    value: PropertyValue::Integer(id),
                } if *node_id == composer => Some(*id as u64),
                _ => None,
            })
            .expect("the composer declared no submit handler");
        drop(first);

        let mut event = Vec::new();
        // Until the Renderer asks for a range the scrollback holds no items at all, so the
        // window has to be opened before there is any message widget to grow.
        encode_event(
            &HostEvent {
                node_id: scrollback,
                handler_id: range_handler,
                payload: EventPayload::RangeRequested {
                    start: 0,
                    count: 20,
                },
            },
            &mut event,
        )
        .expect("the range request did not encode");
        host.dispatch_event(&event)
            .expect("the range request failed");

        encode_event(
            &HostEvent {
                node_id: composer,
                handler_id: handler,
                payload: EventPayload::TextSubmitted("tell me about threads"),
            },
            &mut event,
        )
        .expect("the submit did not encode");
        host.dispatch_event(&event).expect("the submit failed");

        // Poll frames the way the Renderer would after the Host asked for one. The reply
        // starts after a short pause, so a second of frames is far more than it needs.
        // A reply that is still arriving is sent as the new tail alone, so the length on
        // screen is what the node was set to plus everything appended to it since.
        let mut lengths: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        let mut longest = 0usize;
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let batch = host.render_frame(0).expect("a streaming frame failed");
            for mutation in decode_batch(batch).expect("a streaming frame did not decode") {
                let grown = match mutation {
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::Text,
                        value: PropertyValue::String(text),
                    } => Some(*lengths.entry(node_id).insert_entry(text.len()).get()),
                    Mutation::AppendText { node_id, text } => {
                        let total = lengths.entry(node_id).or_default();
                        *total += text.len();
                        Some(*total)
                    }
                    _ => None,
                };
                if let Some(length) = grown {
                    longest = longest.max(length);
                }
            }
            if longest > 200 {
                break;
            }
        }
        assert!(
            longest > 200,
            "the worker's reply never reached a frame; the longest text sent was {longest} bytes"
        );
    }
}
