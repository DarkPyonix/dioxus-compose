//! A kanban board: three columns side by side, cards that move one column left or right,
//! and a field that adds a card to the first column.
//!
//! The columns are flex items that share the width equally. Cards are keyed by id, and a
//! card that changes column is removed from one list and created in the other, appended
//! after the cards already there. The board lives in a [`Board`] the app provides as
//! context, and the form that adds a card reads it from there.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::{use_context, use_context_provider};
use dioxus_signals::{ReadableExt, Signal, WritableExt};

pub const STYLE: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; }
body { font-family: system-ui, sans-serif; font-size: 14px; color: #1f2937; background-color: #ffffff; }

.board-app { padding: 16px; }
.board-app h1 { margin: 0 0 12px; font-size: 20px; }

.new-card { display: flex; gap: 8px; margin-bottom: 12px; }
.new-card input {
    flex: 1;
    min-width: 0;
    height: 32px;
    padding: 0 8px;
    border: 1px solid #d1d5db;
    border-radius: 6px;
}
.new-card button {
    width: 64px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background-color: #2563eb;
    color: #ffffff;
}

.board { display: flex; gap: 12px; }
.column {
    flex: 1;
    min-width: 0;
    padding: 8px;
    border-radius: 8px;
    background-color: #f3f4f6;
}
.column h2 { margin: 0 0 8px; font-size: 14px; color: #374151; }
.column .count { margin-left: 6px; color: #6b7280; }
.column.doing { background-color: #fef3c7; }
.column.done { background-color: #dcfce7; }

.cards { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
.card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 76px;
    padding: 8px;
    border: 1px solid #e5e7eb;
    border-radius: 6px;
    background-color: #ffffff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
}
.card .actions { display: flex; gap: 4px; }
.card button {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid #d1d5db;
    border-radius: 4px;
    background-color: #ffffff;
    color: #374151;
}
.card button:disabled { color: #d1d5db; }

.empty { margin: 0; color: #6b7280; }
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Todo,
    Doing,
    Done,
}

impl Stage {
    /// The column to the left, if there is one.
    pub fn previous(self) -> Option<Stage> {
        match self {
            Stage::Todo => None,
            Stage::Doing => Some(Stage::Todo),
            Stage::Done => Some(Stage::Doing),
        }
    }

    /// The column to the right, if there is one.
    pub fn next(self) -> Option<Stage> {
        match self {
            Stage::Todo => Some(Stage::Doing),
            Stage::Doing => Some(Stage::Done),
            Stage::Done => None,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Stage::Todo => "todo",
            Stage::Doing => "doing",
            Stage::Done => "done",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Stage::Todo => "To do",
            Stage::Doing => "Doing",
            Stage::Done => "Done",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: u32,
    pub title: String,
    pub stage: Stage,
}

/// The cards and the title being typed for the next one.
#[derive(Clone, Copy)]
pub struct Board {
    pub cards: Signal<Vec<Card>>,
    /// What the new-card field holds. The field owns its text while the user types; every
    /// edit is copied here.
    pub draft: Signal<String>,
    next_id: Signal<u32>,
}

impl Board {
    fn new() -> Self {
        let cards: Vec<Card> = [
            ("Write docs", Stage::Todo),
            ("Fix layout", Stage::Todo),
            ("Add tests", Stage::Doing),
            ("Design", Stage::Done),
        ]
        .into_iter()
        .zip(1..)
        .map(|((title, stage), id)| Card {
            id,
            title: title.to_string(),
            stage,
        })
        .collect();
        let next_id = cards.len() as u32 + 1;
        Self {
            cards: Signal::new(cards),
            draft: Signal::new(String::new()),
            next_id: Signal::new(next_id),
        }
    }

    /// Adds the typed title as a new card at the end of the first column and empties the
    /// field. Does nothing when the field holds only white space.
    pub fn add(mut self) {
        let title = self.draft.read().trim().to_string();
        if title.is_empty() {
            return;
        }
        let id = self.next_id.cloned();
        self.next_id.set(id + 1);
        self.cards.write().push(Card {
            id,
            title,
            stage: Stage::Todo,
        });
        self.draft.set(String::new());
    }

    /// Moves the card to `stage`, after the cards already there. A card moved to the
    /// column it is in stays where it is.
    pub fn move_to(mut self, id: u32, stage: Stage) {
        let mut cards = self.cards.write();
        let Some(index) = cards.iter().position(|card| card.id == id) else {
            return;
        };
        if cards[index].stage == stage {
            return;
        }
        let mut card = cards.remove(index);
        card.stage = stage;
        cards.push(card);
    }

    pub fn remove(mut self, id: u32) {
        self.cards.write().retain(|card| card.id != id);
    }
}

/// The row that adds a card: a field for its title and a button. It is a form, so
/// pressing Enter in the field adds the card as the button does.
#[allow(non_snake_case)]
fn NewCard() -> Element {
    let board = use_context::<Board>();
    let mut draft = board.draft;

    rsx! {
        form {
            id: "new-card-form",
            class: "new-card",
            onsubmit: move |event| {
                event.prevent_default();
                board.add();
            },
            input {
                id: "new-card",
                placeholder: "New card",
                value: "{draft}",
                oninput: move |event| draft.set(event.value()),
            }
            button { id: "add", r#type: "submit", "Add" }
        }
    }
}

/// One column: its heading with the number of cards, and the cards in `stage`.
fn column(board: Board, stage: Stage, cards: &[Card]) -> Element {
    let in_column: Vec<Card> = cards
        .iter()
        .filter(|card| card.stage == stage)
        .cloned()
        .collect();
    let count = in_column.len();
    let slug = stage.slug();
    let label = stage.label();

    rsx! {
        div { id: "column-{slug}", class: "column {slug}",
            h2 {
                "{label}"
                span { id: "count-{slug}", class: "count", "{count}" }
            }
            if count == 0 {
                p { class: "empty", "No cards" }
            }
            ul { id: "cards-{slug}", class: "cards",
                for card in in_column {
                    li {
                        key: "{card.id}",
                        id: "card-{card.id}",
                        class: "card",
                        span { class: "title", "{card.title}" }
                        div { class: "actions",
                            button {
                                id: "left-{card.id}",
                                disabled: stage.previous().is_none(),
                                onclick: move |_| {
                                    if let Some(to) = stage.previous() {
                                        board.move_to(card.id, to);
                                    }
                                },
                                "<"
                            }
                            button {
                                id: "right-{card.id}",
                                disabled: stage.next().is_none(),
                                onclick: move |_| {
                                    if let Some(to) = stage.next() {
                                        board.move_to(card.id, to);
                                    }
                                },
                                ">"
                            }
                            button {
                                id: "delete-{card.id}",
                                onclick: move |_| board.remove(card.id),
                                "x"
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn app() -> Element {
    let board = use_context_provider(Board::new);
    let cards = board.cards.cloned();

    rsx! {
        div { class: "board-app",
            h1 { "Board" }
            NewCard {}
            div { id: "board", class: "board",
                {column(board, Stage::Todo, &cards)}
                {column(board, Stage::Doing, &cards)}
                {column(board, Stage::Done, &cards)}
            }
        }
    }
}
