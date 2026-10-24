//! A help centre: a search field, and a list of topics that open to show their answer.
//!
//! A topic is a row whose heading toggles it. The heading is a flex row, so its name and
//! its chevron are blocks with entries of their own: a long name is cut with an ellipsis
//! and the chevron turns a quarter turn, by a transition, when the topic opens. An answer
//! is a paragraph that mixes plain words with `strong`, `em`, `code`, a coloured `span` and
//! a link. Those are inline, so the paragraph is one measured text and the styles are spans
//! over it. A link inside text is drawn as a coloured, underlined span; it is not a
//! separate target. The last answer is a numbered list, whose markers are runs of their own.
//!
//! The search field is uncontrolled, as in every form here: the renderer owns what is
//! typed and `oninput` copies it into a signal that filters the topics by name.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::{ReadableExt, WritableExt};

pub const STYLE: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; }
body {
    font-family: system-ui, sans-serif;
    font-size: 14px;
    line-height: 20px;
    color: #1f2937;
    background-color: #ffffff;
}

.help { padding: 16px; }
/* The heading is plain text only, so it is one block with one line. */
.help h1 { margin: 0 0 12px; font-size: 20px; line-height: 24px; }

.search {
    display: block;
    width: 100%;
    height: 32px;
    margin: 0 0 8px;
    padding: 0 8px;
    border: 1px solid #d1d5db;
    border-radius: 6px;
}
.summary, .empty { margin: 0 0 12px; color: #6b7280; }

.topics { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
.topic { border: 1px solid #e5e7eb; border-radius: 8px; background-color: #ffffff; }
.topic.open { border-color: #93c5fd; }

/* The name and the chevron are flex items, so each is a block with its own entry and the
   name can be cut to the room the chevron leaves it. */
.head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    padding: 0 12px;
    cursor: pointer;
}
.name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-weight: 600;
}
.chevron {
    flex: none;
    color: #6b7280;
    transition: transform 0.2s ease;
}
.open .chevron { transform: rotate(90deg); }

.body { padding: 0 12px 12px; }
.body p { margin: 0; }
.body ol { margin: 0; padding-left: 20px; list-style-type: decimal; }
.body li { margin: 0; }
/* Inline code keeps the paragraph's size: a span only carries a family, a weight, a style,
   a colour and a decoration, so a size set here would not change the text around it. */
.body code { font-family: ui-monospace, Menlo, monospace; color: #7c3aed; }
.warn { color: #b91c1c; }
.link { color: #0969da; text-decoration: underline; }
"#;

#[derive(Clone, Copy, PartialEq)]
pub struct Topic {
    pub id: &'static str,
    pub name: &'static str,
}

pub const TOPICS: [Topic; 4] = [
    Topic {
        id: "reset",
        name: "Reset your password",
    },
    Topic {
        id: "export",
        name: "Export everything you have stored in your account as a CSV or JSON file, including archived items and old drafts",
    },
    Topic {
        id: "billing",
        name: "Billing and invoices",
    },
    Topic {
        id: "shortcuts",
        name: "Keyboard shortcuts",
    },
];

/// The answer to the topic `id`.
fn answer(id: &str) -> Element {
    match id {
        "reset" => rsx! {
            p { id: "answer-reset",
                "Open "
                strong { "Settings" }
                ", choose "
                em { "Security" }
                " and press Reset. The link stays valid for ten minutes."
            }
        },
        "export" => rsx! {
            p { id: "answer-export",
                "Choose "
                strong { "Export" }
                " in the menu, then open "
                code { "export.csv" }
                " in any spreadsheet."
            }
        },
        "billing" => rsx! {
            p { id: "answer-billing",
                "Invoices go out on the first of each month. "
                span { class: "warn", "Cards that fail twice are paused" }
                " until you update them on the "
                a { class: "link", href: "#billing", "billing page" }
                ". Questions? We reply within one business day."
            }
        },
        _ => rsx! {
            ol { id: "answer-shortcuts",
                li { id: "shortcut-1", "Press / to focus the search" }
                li { id: "shortcut-2", "Press Esc to clear it" }
                li { id: "shortcut-3", "Press Enter to open a topic" }
            }
        },
    }
}

pub fn app() -> Element {
    let mut open = use_signal(Vec::<&'static str>::new);
    let mut query = use_signal(String::new);

    let needle = query.read().trim().to_lowercase();
    let open_ids = open.cloned();
    let shown: Vec<Topic> = TOPICS
        .into_iter()
        .filter(|topic| topic.name.to_lowercase().contains(&needle))
        .collect();
    let summary = format!("{} of {} topics", shown.len(), TOPICS.len());

    rsx! {
        div { class: "help",
            h1 { "Help centre" }
            input {
                id: "search",
                class: "search",
                placeholder: "Search topics",
                value: "{query}",
                oninput: move |event| query.set(event.value()),
            }
            p { id: "summary", class: "summary", "{summary}" }
            if shown.is_empty() {
                p { id: "empty", class: "empty", "No topic matches" }
            }
            ul { id: "topics", class: "topics",
                for topic in shown {
                    li {
                        key: "{topic.id}",
                        id: "topic-{topic.id}",
                        class: if open_ids.contains(&topic.id) { "topic open" } else { "topic" },
                        div {
                            id: "head-{topic.id}",
                            class: "head",
                            onclick: move |_| {
                                let mut ids = open.write();
                                match ids.iter().position(|id| *id == topic.id) {
                                    Some(index) => {
                                        ids.remove(index);
                                    }
                                    None => ids.push(topic.id),
                                }
                            },
                            span { id: "name-{topic.id}", class: "name", "{topic.name}" }
                            span { id: "chevron-{topic.id}", class: "chevron", ">" }
                        }
                        if open_ids.contains(&topic.id) {
                            div { id: "body-{topic.id}", class: "body", {answer(topic.id)} }
                        }
                    }
                }
            }
        }
    }
}
