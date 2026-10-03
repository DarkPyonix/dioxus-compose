//! A to-do list: add an item, tick it off, delete it, and move it up or down the list.
//!
//! Rows are keyed by the item's id, so moving an item moves its row rather than rewriting
//! every row's text. The list lives in a [`Todos`] the app provides as context, and the
//! row that adds an item reads it from there.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::{use_context, use_context_provider};
use dioxus_signals::{ReadableExt, Signal, WritableExt};

pub const STYLE: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; }
body { font-family: system-ui, sans-serif; font-size: 14px; color: #1f2937; background-color: #ffffff; }

.todo-app { padding: 16px; }
.todo-app h1 { margin: 0 0 12px; font-size: 20px; }

.new-todo { display: flex; gap: 8px; margin-bottom: 12px; }
.new-todo input {
    flex: 1;
    min-width: 0;
    height: 32px;
    padding: 0 8px;
    border: 1px solid #d1d5db;
    border-radius: 6px;
}
.new-todo button {
    width: 64px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background-color: #2563eb;
    color: #ffffff;
}

.todos { list-style: none; margin: 0; padding: 0; }
.todo {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    border-bottom: 1px solid #e5e7eb;
}
.todo input { width: 16px; height: 16px; margin: 0; }
.todo .title { flex: 1; }
.todo button {
    width: 64px;
    height: 28px;
    padding: 0;
    border: 1px solid #d1d5db;
    border-radius: 4px;
    background-color: #ffffff;
    color: #374151;
}
.todo button:disabled { color: #d1d5db; }
.todo.done { background-color: #f9fafb; }
.todo.done .title { color: #9ca3af; text-decoration: line-through; }

.empty { margin: 0; color: #6b7280; }
"#;

#[derive(Clone, Debug, PartialEq)]
pub struct Todo {
    pub id: u32,
    pub title: String,
    pub done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

/// The items and the title being typed for the next one.
#[derive(Clone, Copy)]
pub struct Todos {
    pub items: Signal<Vec<Todo>>,
    /// What the new-item field holds. The field owns its text while the user types; every
    /// edit is copied here.
    pub draft: Signal<String>,
    next_id: Signal<u32>,
}

impl Todos {
    fn new() -> Self {
        let items: Vec<Todo> = ["Buy milk", "Write tests", "Call mom"]
            .into_iter()
            .zip(1..)
            .map(|(title, id)| Todo {
                id,
                title: title.to_string(),
                done: false,
            })
            .collect();
        let next_id = items.len() as u32 + 1;
        Self {
            items: Signal::new(items),
            draft: Signal::new(String::new()),
            next_id: Signal::new(next_id),
        }
    }

    /// Adds the typed title as a new item at the end and empties the field. Does nothing
    /// when the field holds only white space.
    pub fn add(mut self) {
        let title = self.draft.read().trim().to_string();
        if title.is_empty() {
            return;
        }
        let id = self.next_id.cloned();
        self.next_id.set(id + 1);
        self.items.write().push(Todo {
            id,
            title,
            done: false,
        });
        self.draft.set(String::new());
    }

    pub fn toggle(mut self, id: u32) {
        let mut items = self.items.write();
        if let Some(todo) = items.iter_mut().find(|todo| todo.id == id) {
            todo.done = !todo.done;
        }
    }

    pub fn remove(mut self, id: u32) {
        self.items.write().retain(|todo| todo.id != id);
    }

    /// Swaps the item with its neighbour above or below. The first item cannot go up and
    /// the last cannot go down.
    pub fn shift(mut self, id: u32, direction: Direction) {
        let mut items = self.items.write();
        let Some(index) = items.iter().position(|todo| todo.id == id) else {
            return;
        };
        let other = match direction {
            Direction::Up => index.checked_sub(1),
            Direction::Down => Some(index + 1).filter(|&next| next < items.len()),
        };
        if let Some(other) = other {
            items.swap(index, other);
        }
    }
}

/// The row that adds an item: a field for its title and a button. It is a form, so
/// pressing Enter in the field adds the item as the button does.
#[allow(non_snake_case)]
fn NewTodo() -> Element {
    let todos = use_context::<Todos>();
    let mut draft = todos.draft;

    rsx! {
        form {
            id: "new-todo-form",
            class: "new-todo",
            onsubmit: move |event| {
                event.prevent_default();
                todos.add();
            },
            input {
                id: "new-todo",
                placeholder: "What needs doing?",
                value: "{draft}",
                oninput: move |event| draft.set(event.value()),
            }
            button { id: "add", r#type: "submit", "Add" }
        }
    }
}

pub fn app() -> Element {
    let todos = use_context_provider(Todos::new);
    let items = todos.items.cloned();
    let empty = items.is_empty();
    let last = items.len().saturating_sub(1);

    rsx! {
        div { class: "todo-app",
            h1 { "Todos" }
            NewTodo {}
            if empty {
                p { class: "empty", "Nothing to do." }
            }
            ul { id: "todos", class: "todos",
                for (index, todo) in items.into_iter().enumerate() {
                    li {
                        key: "{todo.id}",
                        id: "todo-{todo.id}",
                        class: if todo.done { "todo done" } else { "todo" },
                        input {
                            id: "toggle-{todo.id}",
                            r#type: "checkbox",
                            checked: todo.done,
                            onclick: move |_| todos.toggle(todo.id),
                        }
                        span { class: "title", "{todo.title}" }
                        button {
                            id: "up-{todo.id}",
                            disabled: index == 0,
                            onclick: move |_| todos.shift(todo.id, Direction::Up),
                            "Up"
                        }
                        button {
                            id: "down-{todo.id}",
                            disabled: index == last,
                            onclick: move |_| todos.shift(todo.id, Direction::Down),
                            "Down"
                        }
                        button {
                            id: "delete-{todo.id}",
                            onclick: move |_| todos.remove(todo.id),
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}
