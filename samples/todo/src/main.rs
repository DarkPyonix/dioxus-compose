//! A task list: add, edit in place, complete, delete, reorder, filter, and survive a
//! restart.
//!
//! The list is drawn with `LazyColumn`, so the number of widgets that exist is
//! proportional to what is on screen rather than to the number of tasks. "Add 5000" is
//! there to make that claim checkable: a list that only ever holds a dozen items never
//! exercises the windowing protocol at all.
//!
//! Every task carries an id that never changes, and that id is the list key. Editing one
//! row therefore rebuilds that row and leaves its neighbours alone.

mod store;

use dioxus_compose::prelude::*;
use store::{Filter, Task};

/// Enough rows that the window is a small fraction of the list.
const BULK_COUNT: usize = 5_000;

fn app() -> Element {
    let mut tasks = use_signal(store::load);
    let mut next_id = use_signal(|| {
        tasks
            .read()
            .iter()
            .map(|task| task.id + 1)
            .max()
            .unwrap_or(1)
    });
    let mut filter = use_signal(|| Filter::All);

    // What the top field currently holds. The field is uncontrolled, so this is a copy the
    // field pushes up, not the field's value being driven from here.
    let mut draft = use_signal(String::new);
    // The task being edited in place, and the text its editor currently holds.
    let mut editing = use_signal(|| Option::<u64>::None);
    let mut edit_draft = use_signal(String::new);

    let visible: Vec<usize> = tasks
        .read()
        .iter()
        .enumerate()
        .filter(|(_, task)| filter().accepts(task))
        .map(|(index, _)| index)
        .collect();
    let remaining = tasks.read().iter().filter(|task| !task.done).count();
    let total = tasks.read().len();

    let mut add = move |title: String| {
        let title = title.trim().to_owned();
        if title.is_empty() {
            return;
        }
        let id = next_id();
        next_id.set(id + 1);
        tasks.write().push(Task {
            id,
            title,
            done: false,
        });
        draft.set(String::new());
        store::save(&tasks.read());
    };

    let mut commit_edit = move |text: String| {
        let Some(id) = editing() else { return };
        let text = text.trim().to_owned();
        if !text.is_empty() {
            let mut list = tasks.write();
            if let Some(task) = list.iter_mut().find(|task| task.id == id) {
                task.title = text;
            }
        }
        editing.set(None);
        edit_draft.set(String::new());
        store::save(&tasks.read());
    };

    // Reordering is expressed in terms of what the user can see. Under a filter the
    // neighbour above a row is the previous visible row, not the previous stored one, so
    // each row works out its two neighbours and this only performs the swap. Keeping the
    // visible list out of the closure is what lets it stay `Copy`, which every handler
    // stored in the tree has to be.
    let mut swap_tasks = move |from: usize, to: usize| {
        tasks.write().swap(from, to);
        store::save(&tasks.read());
    };

    let keys: Vec<String> = visible
        .iter()
        .map(|index| tasks.read()[*index].id.to_string())
        .collect();
    let rows = visible.clone();

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            spacing: 8.0,

            Text { text: "Tasks", type_role: TypeRole::Headline }

            Row {
                fill_max_width: true,
                spacing: 8.0,
                alignment: Alignment::CenterStart,
                TextField {
                    placeholder: "Add a task, then press Enter",
                    on_value_change: move |value| draft.set(value),
                    on_submit: move |value: String| add(value),
                }
                Button {
                    text: "Add",
                    on_click: move |_| add(draft()),
                }
            }

            Row {
                fill_max_width: true,
                spacing: 8.0,
                alignment: Alignment::CenterStart,
                for choice in Filter::STRIP {
                    Button {
                        key: "{choice.label()}",
                        text: choice.label(),
                        variant: if filter() == choice { ButtonVariant::Filled } else { ButtonVariant::Outlined },
                        on_click: move |_| filter.set(choice),
                    }
                }
                Text { text: "{remaining} of {total} remaining", type_role: TypeRole::Label }
            }

            Row {
                fill_max_width: true,
                spacing: 8.0,
                alignment: Alignment::CenterStart,
                Button {
                    text: "Add {BULK_COUNT} tasks",
                    variant: ButtonVariant::Tonal,
                    on_click: move |_| {
                        let start = next_id();
                        {
                            let mut list = tasks.write();
                            list.reserve(BULK_COUNT);
                            for offset in 0..BULK_COUNT as u64 {
                                let id = start + offset;
                                list.push(Task {
                                    id,
                                    title: format!("Generated task {id}"),
                                    done: offset % 3 == 0,
                                });
                            }
                        }
                        next_id.set(start + BULK_COUNT as u64);
                        store::save(&tasks.read());
                    },
                }
                Button {
                    text: "Clear completed",
                    variant: ButtonVariant::Text,
                    on_click: move |_| {
                        tasks.write().retain(|task| !task.done);
                        store::save(&tasks.read());
                    },
                }
            }

            LazyColumn {
                item_count: rows.len(),
                key_of: move |position: usize| keys[position].clone(),
                item: move |position: usize| {
                    let index = rows[position];
                    let previous = position.checked_sub(1).map(|above| rows[above]);
                    let next = rows.get(position + 1).copied();
                    let task = tasks.read()[index].clone();
                    let editing_this = editing() == Some(task.id);
                    rsx! {
                        Row {
                            fill_max_width: true,
                            spacing: 8.0,
                            alignment: Alignment::CenterStart,
                            Button {
                                text: if task.done { "[x]" } else { "[ ]" },
                                variant: ButtonVariant::Text,
                                on_click: move |_| {
                                    tasks.write()[index].done = !task.done;
                                    store::save(&tasks.read());
                                },
                            }
                            if editing_this {
                                TextField {
                                    placeholder: task.title.clone(),
                                    on_value_change: move |value| edit_draft.set(value),
                                    on_submit: move |value: String| commit_edit(value),
                                    on_focus_lost: move |_| commit_edit(edit_draft()),
                                }
                                Button {
                                    text: "Save",
                                    variant: ButtonVariant::Text,
                                    on_click: move |_| commit_edit(edit_draft()),
                                }
                            } else {
                                Text {
                                    text: task.title.clone(),
                                    color: if task.done {
                                        Paint::Role(ColorRole::OutlineVariant)
                                    } else {
                                        Paint::Role(ColorRole::OnSurface)
                                    },
                                    max_lines: 1,
                                    overflow: TextOverflow::Ellipsis,
                                }
                                Button {
                                    text: "Edit",
                                    variant: ButtonVariant::Text,
                                    on_click: move |_| {
                                        edit_draft.set(String::new());
                                        editing.set(Some(task.id));
                                    },
                                }
                            }
                            Button {
                                text: "Up",
                                variant: ButtonVariant::Text,
                                enabled: previous.is_some(),
                                on_click: move |_| {
                                    if let Some(above) = previous {
                                        swap_tasks(index, above);
                                    }
                                },
                            }
                            Button {
                                text: "Down",
                                variant: ButtonVariant::Text,
                                enabled: next.is_some(),
                                on_click: move |_| {
                                    if let Some(below) = next {
                                        swap_tasks(index, below);
                                    }
                                },
                            }
                            Button {
                                text: "Delete",
                                variant: ButtonVariant::Text,
                                on_click: move |_| {
                                    tasks.write().remove(index);
                                    store::save(&tasks.read());
                                },
                            }
                        }
                    }
                },
            }
        }
    }
}

fn main() {
    store::start_saver();
    dioxus_compose::launch(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus_compose::Host;

    use dioxus_compose::protocol::HostEvent;
    use dioxus_compose::protocol::{Mutation, decode_batch, encode_event};
    use dioxus_compose::{EventPayload, PropertyKind, WidgetKind};

    /// Every property this screen sets has to be one the wire can name. A property the
    /// schema does not have fails the whole batch rather than just itself, so a screen that
    /// builds in Rust can still be blank on screen.
    #[test]
    fn the_first_frame_encodes_without_a_protocol_error() {
        assert!(Host::new(app).rebuild().is_ok());
    }

    /// A saved list of several thousand tasks must cost widgets in proportion to the window
    /// the Renderer asked for, not to the list. This is the claim the "Add 5000" button
    /// exists to let a person check by hand, asserted here so it cannot quietly stop being
    /// true.
    #[test]
    fn fr8_a_long_list_materialises_only_the_requested_window() {
        let path = std::env::temp_dir().join("sample-todo-window-test.tsv");
        let saved: String = (0..5_000)
            .map(|id| format!("{id}\t0\tGenerated task {id}\n"))
            .collect();
        std::fs::write(&path, saved).expect("the fixture list could not be written");
        // SAFETY: this test reads the variable through `store::load` on this same thread
        // before anything else in the process looks at it.
        unsafe { std::env::set_var("SAMPLE_TODO_FILE", &path) };

        let mut host = Host::new(app);
        let first = decode_batch(host.rebuild().expect("the first frame failed to encode"))
            .expect("the first frame did not decode");
        let list = first
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::Create {
                    node_id,
                    widget: WidgetKind::LazyColumn,
                } => Some(*node_id),
                _ => None,
            })
            .expect("the screen has no LazyColumn");
        let handler = first
            .iter()
            .find_map(|mutation| match mutation {
                Mutation::SetProp {
                    node_id,
                    property: PropertyKind::OnRangeRequested,
                    value,
                } if *node_id == list => match value {
                    dioxus_compose::protocol::PropertyValue::Integer(id) => Some(*id as u64),
                    _ => None,
                },
                _ => None,
            })
            .expect("the LazyColumn declared no range handler");
        drop(first);

        let mut event = Vec::new();
        encode_event(
            &HostEvent {
                node_id: list,
                handler_id: handler,
                payload: EventPayload::RangeRequested {
                    start: 4_000,
                    count: 20,
                },
            },
            &mut event,
        )
        .expect("the range request did not encode");
        let (batch, _) = host
            .dispatch_event(&event)
            .expect("the range request failed");
        let created = decode_batch(batch)
            .expect("the window batch did not decode")
            .iter()
            .filter(|mutation| matches!(mutation, Mutation::Create { .. }))
            .count();

        // Each row is a handful of widgets. What matters is that the count tracks the
        // window of 20 rather than the 5,000 tasks behind it.
        assert!(
            created < 400,
            "{created} widgets were created for a window of 20 out of 5,000 tasks"
        );
    }
}
