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

            // The count belongs beside the title, which is where a list says how much of
            // itself is left. The title takes the weight and pushes it to the far end.
            TopAppBar {
                fill_max_width: true,
                Text { text: "Tasks", type_role: TypeRole::Title, weight: 1.0 }
                Text {
                    text: "{remaining} of {total} remaining",
                    type_role: TypeRole::Label,
                    color: Paint::Role(ColorRole::OnSurfaceVariant),
                }
            }

            Column {
                fill_max_width: true,
                fill_max_height: true,
                padding_role: SpaceRole::Lg,
                space_role: SpaceRole::Md,

                // The composer: one grouped strip whose field grows with the window, so
                // the field is the thing you look at and the button is the thing beside it.
                Surface {
                    fill_max_width: true,
                    Row {
                        fill_max_width: true,
                        space_role: SpaceRole::Sm,
                        alignment: Alignment::CenterStart,
                        TextField {
                            weight: 1.0,
                            placeholder: "Add a task, then press Enter",
                            on_value_change: move |value| draft.set(value),
                            on_submit: move |value: String| add(value),
                        }
                        Button {
                            text: "Add",
                            variant: ButtonVariant::Filled,
                            on_click: move |_| add(draft()),
                        }
                    }
                }

                // The filter strip, with the two destructive or bulk actions pushed to the
                // far end so they are not mistaken for part of the filter.
                Row {
                    fill_max_width: true,
                    space_role: SpaceRole::Sm,
                    alignment: Alignment::CenterStart,
                    for choice in Filter::STRIP {
                        Button {
                            key: "{choice.label()}",
                            text: choice.label(),
                            variant: if filter() == choice { ButtonVariant::Filled } else { ButtonVariant::Outlined },
                            on_click: move |_| filter.set(choice),
                        }
                    }
                    Spacer { weight: 1.0 }
                    Button {
                        text: "Clear completed",
                        variant: ButtonVariant::Text,
                        on_click: move |_| {
                            tasks.write().retain(|task| !task.done);
                            store::save(&tasks.read());
                        },
                    }
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
                }

                // An empty list explains itself rather than leaving a blank half window
                // that could just as well be a screen that failed to draw. It replaces the
                // list rather than sitting above it, so it gets the whole of the space the
                // list would have taken.
                if rows.is_empty() {
                    dioxus_compose::Box {
                        fill_max_width: true,
                        weight: 1.0,
                        alignment: Alignment::Center,
                        Text {
                            text: match filter() {
                                Filter::All => "No tasks yet. Add one above.",
                                Filter::Active => "Nothing left to do under this filter.",
                                Filter::Done => "Nothing has been completed yet.",
                            },
                            type_role: TypeRole::Body,
                            color: Paint::Role(ColorRole::OnSurfaceVariant),
                        }
                    }
                } else {
                // The list is one grouped container, not a stack of cards. A task is a row
                // in a list of tasks, and a card each would say that every task is a
                // separate document. What separates one row from the next is a hairline.
                Surface {
                    fill_max_width: true,
                    weight: 1.0,
                LazyColumn {
                    fill_max_width: true,
                    weight: 1.0,
                    item_count: rows.len(),
                    key_of: move |position: usize| keys[position].clone(),
                    item: move |position: usize| {
                        let index = rows[position];
                        let previous = position.checked_sub(1).map(|above| rows[above]);
                        let next = rows.get(position + 1).copied();
                        let task = tasks.read()[index].clone();
                        let editing_this = editing() == Some(task.id);
                        let last = position + 1 == rows.len();
                        rsx! {
                            Column {
                                fill_max_width: true,
                                Row {
                                    fill_max_width: true,
                                    padding_role: SpaceRole::Xs,
                                    space_role: SpaceRole::Xs,
                                    alignment: Alignment::CenterStart,
                                    // A ballot box reads as something you can tick. The
                                    // filled variant is the second half of the same
                                    // statement, so a completed task is legible at a
                                    // glance rather than by reading the glyph.
                                    Button {
                                        text: if task.done { "\u{2611}" } else { "\u{2610}" },
                                        variant: if task.done {
                                            ButtonVariant::Tonal
                                        } else {
                                            ButtonVariant::Text
                                        },
                                        on_click: move |_| {
                                            tasks.write()[index].done = !task.done;
                                            store::save(&tasks.read());
                                        },
                                    }
                                    // The title takes the weight, so the actions sit at the
                                    // far end of every row and line up down the list.
                                    if editing_this {
                                        TextField {
                                            weight: 1.0,
                                            placeholder: task.title.clone(),
                                            on_value_change: move |value| edit_draft.set(value),
                                            on_submit: move |value: String| commit_edit(value),
                                            on_focus_lost: move |_| commit_edit(edit_draft()),
                                        }
                                        Button {
                                            text: "Save",
                                            variant: ButtonVariant::Filled,
                                            on_click: move |_| commit_edit(edit_draft()),
                                        }
                                    } else {
                                        Text {
                                            text: task.title.clone(),
                                            weight: 1.0,
                                            type_role: TypeRole::Body,
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
                                    // Reordering is a minor, repeatable adjustment, so the
                                    // arrows are drawn in the quiet ink rather than in the
                                    // accent. Four actions in the accent would all shout
                                    // equally, and the one that deletes would shout no
                                    // louder than the one that nudges a row up by one.
                                    Button {
                                        text: "\u{2191}",
                                        variant: ButtonVariant::Text,
                                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                                        enabled: previous.is_some(),
                                        on_click: move |_| {
                                            if let Some(above) = previous {
                                                swap_tasks(index, above);
                                            }
                                        },
                                    }
                                    Button {
                                        text: "\u{2193}",
                                        variant: ButtonVariant::Text,
                                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                                        enabled: next.is_some(),
                                        on_click: move |_| {
                                            if let Some(below) = next {
                                                swap_tasks(index, below);
                                            }
                                        },
                                    }
                                    // Deleting a task cannot be undone, and the error role
                                    // is how every one of these design systems says so.
                                    Button {
                                        text: "Delete",
                                        variant: ButtonVariant::Text,
                                        color: Paint::Role(ColorRole::Error),
                                        on_click: move |_| {
                                            tasks.write().remove(index);
                                            store::save(&tasks.read());
                                        },
                                    }
                                }
                                // The hairline belongs between two rows, so the last row
                                // does not draw one against the container's edge.
                                if !last {
                                    Separator {}
                                }
                            }
                        }
                    },
                }
                }
                }
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
    use dioxus_compose::protocol::{
        HostEvent, Mutation, PropertyValue, decode_batch, encode_event,
    };
    use dioxus_compose::schema::{EventPayload, PropertyKind, WidgetKind};
    use std::collections::HashMap;
    use std::sync::OnceLock;

    /// A saved list large enough that a window of twenty is a small fraction of it.
    const SAVED_TASKS: usize = 5_000;
    const WINDOW: usize = 20;

    /// Point the store at a generated file, once for the whole test binary. Tests run
    /// concurrently in one process, so the variable is written before any of them reads it
    /// rather than once per test.
    fn saved_list() {
        static PREPARED: OnceLock<()> = OnceLock::new();
        PREPARED.get_or_init(|| {
            let path = std::env::temp_dir().join("sample-todo-window-test.tsv");
            let saved: String = (0..SAVED_TASKS)
                .map(|id| format!("{id}\t0\tGenerated task {id}\n"))
                .collect();
            std::fs::write(&path, saved).expect("the fixture list could not be written");
            // SAFETY: written once, before this binary's tests read it through
            // `store::load`, and nothing else in the process touches the environment.
            unsafe { std::env::set_var("SAMPLE_TODO_FILE", &path) };
        });
    }

    fn task_title(index: usize) -> String {
        format!("Generated task {index}")
    }

    fn expected_titles(start: usize, count: usize) -> Vec<String> {
        let first = start.min(SAVED_TASKS);
        let last = (first + count).min(SAVED_TASKS);
        let mut titles: Vec<_> = (first..last).map(task_title).collect();
        titles.sort();
        titles
    }

    /// Stand-in for the Compose interpreter: applies a batch to a node table so the test
    /// can ask what is alive rather than what was mentioned. Removing a node removes its
    /// whole subtree, as Compose does.
    #[derive(Default)]
    struct MockRenderer {
        widgets: HashMap<u32, WidgetKind>,
        texts: HashMap<u32, String>,
        item_keys: HashMap<u32, String>,
        parents: HashMap<u32, u32>,
        item_count: Option<i64>,
    }

    impl MockRenderer {
        fn apply(&mut self, batch: &[Mutation<'_>]) {
            for mutation in batch {
                match mutation {
                    Mutation::Create { node_id, widget } => {
                        self.widgets.insert(*node_id, *widget);
                    }
                    Mutation::Insert {
                        parent_id, node_id, ..
                    }
                    | Mutation::Move {
                        parent_id, node_id, ..
                    } => {
                        self.parents.insert(*node_id, *parent_id);
                    }
                    Mutation::Remove { node_id } => self.remove_subtree(*node_id),
                    Mutation::SetProp {
                        node_id,
                        property,
                        value,
                    } => match (property, value) {
                        (PropertyKind::Text, PropertyValue::String(value)) => {
                            self.texts.insert(*node_id, (*value).to_owned());
                        }
                        (PropertyKind::ItemKey, PropertyValue::String(value)) => {
                            self.item_keys.insert(*node_id, (*value).to_owned());
                        }
                        (PropertyKind::ItemCount, PropertyValue::Integer(value)) => {
                            self.item_count = Some(*value);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        fn remove_subtree(&mut self, node_id: u32) {
            let children: Vec<_> = self
                .parents
                .iter()
                .filter(|(_, parent)| **parent == node_id)
                .map(|(child, _)| *child)
                .collect();
            for child in children {
                self.remove_subtree(child);
            }
            self.widgets.remove(&node_id);
            self.texts.remove(&node_id);
            self.item_keys.remove(&node_id);
            self.parents.remove(&node_id);
        }

        /// The titles of the tasks that currently exist as widgets. The screen's own
        /// chrome carries text too, so a task title is recognised by its text.
        fn live_task_titles(&self) -> Vec<String> {
            let mut titles: Vec<_> = self
                .texts
                .iter()
                .filter(|(node_id, _)| self.widgets.contains_key(node_id))
                .map(|(_, text)| text.clone())
                .filter(|text| text.starts_with("Generated task "))
                .collect();
            titles.sort();
            titles
        }

        fn live_item_keys(&self) -> Vec<String> {
            let mut keys: Vec<_> = self
                .item_keys
                .iter()
                .filter(|(node_id, _)| self.widgets.contains_key(node_id))
                .map(|(_, key)| key.clone())
                .collect();
            keys.sort();
            keys
        }

        fn node_count(&self) -> usize {
            self.widgets.len()
        }
    }

    /// The real screen, driven the way the Renderer drives it.
    struct Screen {
        host: Host,
        mock: MockRenderer,
        list: u32,
        handler: u64,
        event: Vec<u8>,
    }

    impl Screen {
        fn new() -> Self {
            saved_list();
            let mut host = Host::new(app);
            let first = decode_batch(host.rebuild().expect("the first frame failed to encode"))
                .expect("the first frame did not decode");
            let mut mock = MockRenderer::default();
            mock.apply(&first);
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
                        value: PropertyValue::Integer(id),
                    } if *node_id == list => Some(*id as u64),
                    _ => None,
                })
                .expect("the LazyColumn declared no range handler");
            drop(first);
            Self {
                host,
                mock,
                list,
                handler,
                event: Vec::new(),
            }
        }

        /// Scrolling: the Renderer asks for a window and the Host answers with a batch.
        /// Returns how many widgets that batch created.
        fn request_range(&mut self, start: usize, count: usize) -> usize {
            encode_event(
                &HostEvent {
                    node_id: self.list,
                    handler_id: self.handler,
                    payload: EventPayload::RangeRequested {
                        start: start as u32,
                        count: count as u32,
                    },
                },
                &mut self.event,
            )
            .expect("the range request did not encode");
            let (batch, _) = self
                .host
                .dispatch_event(&self.event)
                .expect("the range request failed");
            let decoded = decode_batch(batch).expect("the window batch did not decode");
            self.mock.apply(&decoded);
            decoded
                .iter()
                .filter(|mutation| matches!(mutation, Mutation::Create { .. }))
                .count()
        }
    }

    /// Every property this screen sets has to be one the wire can name. A property the
    /// schema does not have fails the whole batch rather than just itself, so a screen that
    /// builds in Rust can still be blank on screen.
    #[test]
    fn the_first_frame_encodes_without_a_protocol_error() {
        saved_list();
        assert!(Host::new(app).rebuild().is_ok());
    }

    /// The Renderer owns the scroll position, so it owns the read-ahead buffer too. The
    /// Host materialises the range it was asked for and no more: a Host that widened the
    /// range would leave the Renderer unable to say where the subtree it received belongs
    /// in the full list.
    #[test]
    fn fr8_only_the_requested_range_is_materialised() {
        let mut screen = Screen::new();
        assert_eq!(screen.mock.item_count, Some(SAVED_TASKS as i64));
        assert!(screen.mock.live_task_titles().len() < SAVED_TASKS);

        screen.request_range(100, WINDOW);

        assert_eq!(screen.mock.live_task_titles(), expected_titles(100, WINDOW));
    }

    /// The claim the "Add 5000" button exists to let a person check by hand, asserted so
    /// it cannot quietly stop being true. A window of twenty out of five thousand tasks
    /// costs widgets in proportion to the twenty. This is the test that fails the day
    /// windowing degrades into building everything.
    #[test]
    fn fr8_node_count_is_proportional_to_the_window() {
        let mut screen = Screen::new();
        screen.request_range(4_000, WINDOW);

        // A row is a handful of widgets: the column holding it, the row itself, the
        // toggle, the title, four buttons and the hairline under it. The screen's own
        // chrome is a fixed handful on top of that. What matters is that the total tracks
        // the window and not the list behind it.
        const PER_ROW: usize = 10;
        const CHROME: usize = 40;
        let nodes = screen.mock.node_count();
        assert_eq!(screen.mock.live_task_titles().len(), WINDOW);
        assert!(
            nodes <= PER_ROW * WINDOW + CHROME,
            "{nodes} nodes materialised for a window of {WINDOW} out of {SAVED_TASKS} tasks"
        );
    }

    /// Scrolling away and back rebuilds the identical subtree. The Host owns the data, so
    /// a row that left the window comes back with the same title under the same key.
    #[test]
    fn fr8_re_requesting_an_earlier_range_restores_it() {
        let mut screen = Screen::new();
        screen.request_range(0, 10);
        let first_titles = screen.mock.live_task_titles();
        let first_keys = screen.mock.live_item_keys();
        assert_eq!(first_titles, expected_titles(0, 10));

        screen.request_range(4_000, 10);
        assert_eq!(screen.mock.live_task_titles(), expected_titles(4_000, 10));

        screen.request_range(0, 10);
        assert_eq!(screen.mock.live_task_titles(), first_titles);
        assert_eq!(screen.mock.live_item_keys(), first_keys);
    }

    /// Scrolling stays flat. A window far down the list costs what the first one cost, or
    /// a long list gets slower the further it is read.
    #[test]
    fn fr8_a_later_window_costs_what_the_first_one_cost() {
        let mut screen = Screen::new();
        let first = screen.request_range(0, WINDOW);
        for start in (WINDOW..SAVED_TASKS).step_by(WINDOW).take(50) {
            let created = screen.request_range(start, WINDOW);
            assert!(
                created <= first,
                "the window at {start} created {created} widgets where the first created {first}"
            );
        }
    }
}
