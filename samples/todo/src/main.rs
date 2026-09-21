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
    let window = use_window_size();
    // A list of one line items read across 1200dp is a list nobody can scan: the eye has
    // to travel from the title to the actions and back for every row. Past an expanded
    // window the screen stops widening and centres, and the page shows either side.
    let measure = if window.is_expanded() {
        Some(WindowSizeClass::EXPANDED_MIN_WIDTH_DP)
    } else {
        None
    };
    let stacked = window.is_compact();
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
    let mut menu_open = use_signal(|| Option::<u64>::None);

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

    let list = rsx! {
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
            fill_max_height: true,
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
                            // A ballot box reads as something you can tick. Ticking
                            // it colours the mark rather than putting a container
                            // behind it: a filled box around one row's first
                            // column would weigh more than the row it belongs to.
                            Button {
                                text: if task.done { "\u{2611}" } else { "\u{2610}" },
                                variant: ButtonVariant::Text,
                                color: if task.done {
                                    Paint::Role(ColorRole::Primary)
                                } else {
                                    Paint::Role(ColorRole::Outline)
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
                                // What a row can have done to it lives behind one control,
                                // not spread across four.
                                //
                                // Edit, move up, move down and delete on the face of every
                                // row is four permanently drawn controls per task, twenty
                                // thousand of them once this list is filled, and a row
                                // whose actions outweigh the task they act on. An overflow
                                // menu is the idiom every design system shares for
                                // this: Material calls it the overflow menu, Cupertino the
                                // ellipsis menu, Fluent a command flyout, and the three
                                // desktop Linux languages a hamburger. The other two
                                // answers are hover reveal and swipe, and neither is
                                // available: pointer enter and exit are not events the
                                // protocol carries, and a swipe is not a gesture the
                                // schema has.
                                //
                                // The entries are declared whether the menu is open or
                                // not. Declaring them only while it is open would be
                                // better, and it recurses until the stack overflows: a
                                // `Menu` whose children are a conditional or a loop,
                                // inside a `LazyColumn` item, does not terminate. Four
                                // entries and an anchor per visible row is still a cost
                                // that tracks the window rather than the list.
                                Menu {
                                    expanded: menu_open() == Some(task.id),
                                    on_dismiss: move |_| menu_open.set(None),
                                    anchor: rsx! {
                                        Button {
                                            text: "\u{22ef}",
                                            variant: ButtonVariant::Text,
                                            color: Paint::Role(ColorRole::OnSurfaceVariant),
                                            on_click: move |_| menu_open.set(Some(task.id)),
                                        }
                                    },
                                    Button {
                                        text: "Edit",
                                        variant: ButtonVariant::Text,
                                        fill_max_width: true,
                                        on_click: move |_| {
                                            menu_open.set(None);
                                            edit_draft.set(String::new());
                                            editing.set(Some(task.id));
                                        },
                                    }
                                    Button {
                                        text: "Move up",
                                        variant: ButtonVariant::Text,
                                        fill_max_width: true,
                                        enabled: previous.is_some(),
                                        on_click: move |_| {
                                            menu_open.set(None);
                                            if let Some(above) = previous {
                                                swap_tasks(index, above);
                                            }
                                        },
                                    }
                                    Button {
                                        text: "Move down",
                                        variant: ButtonVariant::Text,
                                        fill_max_width: true,
                                        enabled: next.is_some(),
                                        on_click: move |_| {
                                            menu_open.set(None);
                                            if let Some(below) = next {
                                                swap_tasks(index, below);
                                            }
                                        },
                                    }
                                    // Deleting is the one action here that throws work
                                    // away without asking first, so it says what it did
                                    // and offers the task back. The message is not a
                                    // widget this code places: it is handed over once and
                                    // the Renderer decides where it goes and how long it
                                    // stays, which is why nothing here holds a timer.
                                    Button {
                                        text: "Delete",
                                        variant: ButtonVariant::Text,
                                        fill_max_width: true,
                                        color: Paint::Role(ColorRole::Error),
                                        on_click: move |_| {
                                            menu_open.set(None);
                                            let removed = tasks.write().remove(index);
                                            store::save(&tasks.read());
                                            let title = removed.title.clone();
                                            Message::new(format!("Deleted \u{201c}{title}\u{201d}"))
                                                .with_action("Undo", move |()| {
                                                    let at = index.min(tasks.read().len());
                                                    tasks.write().insert(at, removed.clone());
                                                    store::save(&tasks.read());
                                                })
                                                .with_duration(MessageDuration::Long)
                                                .show();
                                        },
                                    }
                                }
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
    };

    rsx! {
        // The three filters are three destinations, and one declaration is all of them.
        // This code never asks how wide the window is: the Renderer has measured it and
        // draws a bar along the bottom of a phone, a rail beside a tablet and a drawer
        // standing open on a desktop, from these same three items.
        Navigation {
            fill_max_width: true,
            fill_max_height: true,
            selected_index: filter().index(),
            for choice in Filter::STRIP {
                NavigationItem {
                    key: "{choice.label()}",
                    text: choice.label(),
                    icon: choice.icon(),
                    on_click: move |()| filter.set(choice),
                }
            }
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
                // Clearing throws work away, so it says so in the same colour a row's own
                // Delete uses. It sits beside the count because it is about the count.
                Button {
                    text: if stacked { "Clear" } else { "Clear completed" },
                    variant: ButtonVariant::Text,
                    color: Paint::Role(ColorRole::Error),
                    on_click: move |_| {
                        tasks.write().retain(|task| !task.done);
                        store::save(&tasks.read());
                    },
                }
            }

            dioxus_compose::Box {
                fill_max_width: true,
                fill_max_height: true,
                alignment: Alignment::TopCenter,
            Column {
                fill_max_width: measure.is_none(),
                width: measure,
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

                // List and detail. Narrower than a desktop window the list is the whole
                // width and the editor is the row itself.
                {list}

                // The demo footer. Filling the list with five thousand rows is not
                // something a task list does, it is how this sample makes its claim about
                // windowing checkable, so it says that and sits below the list in the
                // caption ink. It used to share the filter strip with "Clear completed",
                // where a tonal container beside bare red text read as two peers styled by
                // accident rather than as an action and a demo control.
                Row {
                    fill_max_width: true,
                    space_role: SpaceRole::Sm,
                    alignment: Alignment::CenterStart,
                    if !stacked {
                        Text {
                            text: "Sample: the list windows its rows, so only what is on screen exists.",
                            weight: 1.0,
                            type_role: TypeRole::Caption,
                            color: Paint::Role(ColorRole::OnSurfaceVariant),
                            max_lines: 1,
                            overflow: TextOverflow::Ellipsis,
                        }
                    } else {
                        Spacer { weight: 1.0 }
                    }
                    Button {
                        text: if stacked {
                            format!("+{BULK_COUNT}")
                        } else {
                            format!("Add {BULK_COUNT} tasks")
                        },
                        variant: ButtonVariant::Text,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
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
            }
            }
        }
        }
    }
}

// Samples are demonstrations, so they let you see any of the design systems rather than
// only the one this machine happens to select. Unset, the app adapts to the host platform,
// which is what a real application wants.
fn main() {
    store::start_saver();
    dioxus_compose::LaunchBuilder::new()
        .with_theme(dioxus_compose::demo_theme())
        .launch(app);
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
            let path = scratch("todo-window");
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
        /// The click handler each node declared, so a test can press what it found.
        click_handlers: HashMap<u32, u64>,
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
                        (PropertyKind::OnClick, PropertyValue::Integer(value)) => {
                            self.click_handlers.insert(*node_id, *value as u64);
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

        /// How many live nodes are of one kind.
        fn count_of(&self, kind: WidgetKind) -> usize {
            self.widgets.values().filter(|got| **got == kind).count()
        }

        /// Whether a node has an ancestor of the given kind.
        fn is_inside(&self, node_id: u32, kind: WidgetKind) -> bool {
            let mut current = node_id;
            while let Some(parent) = self.parents.get(&current).copied() {
                if self.widgets.get(&parent) == Some(&kind) {
                    return true;
                }
                current = parent;
            }
            false
        }

        /// The nodes carrying one piece of text.
        fn nodes_with_text(&self, wanted: &str) -> Vec<u32> {
            self.texts
                .iter()
                .filter(|(node_id, text)| {
                    self.widgets.contains_key(node_id) && text.as_str() == wanted
                })
                .map(|(node_id, _)| *node_id)
                .collect()
        }
    }

    /// A record copied out of a batch so the batch can be dropped.
    #[derive(Debug)]
    enum OwnedRecord {
        Message {
            handler_id: u64,
            text: String,
            action: String,
        },
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

        /// Presses a node, the way the Renderer reports a press, and returns what the
        /// Host said in reply.
        fn click(&mut self, node_id: u32) -> Vec<OwnedRecord> {
            let handler_id = *self
                .mock
                .click_handlers
                .get(&node_id)
                .unwrap_or_else(|| panic!("node {node_id} has no click handler"));
            self.dispatch(HostEvent {
                node_id,
                handler_id,
                payload: EventPayload::Clicked,
            })
        }

        /// Sends one event and applies the batch that comes back.
        fn dispatch(&mut self, event: HostEvent<'_>) -> Vec<OwnedRecord> {
            encode_event(&event, &mut self.event).expect("the event did not encode");
            let bytes = self.event.clone();
            let (batch, _) = self.host.dispatch_event(&bytes).expect("the event failed");
            let decoded = decode_batch(batch).expect("the batch did not decode");
            self.mock.apply(&decoded);
            decoded
                .iter()
                .filter_map(|mutation| match mutation {
                    Mutation::ShowMessage {
                        handler_id,
                        text,
                        action,
                        ..
                    } => Some(OwnedRecord::Message {
                        handler_id: *handler_id,
                        text: (*text).to_owned(),
                        action: (*action).to_owned(),
                    }),
                    _ => None,
                })
                .collect()
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

    /// Writes one screen as the batches that build it, each behind its byte length.
    ///
    /// A batch is a single envelope and cannot simply be appended to another one, so a
    /// screen that takes more than one of them has to keep the boundaries. Four little
    /// endian bytes of length in front of each is enough, and it is what the Renderer's
    /// screenshot test reads back.
    fn write_frames(path: &std::path::Path, batches: &[&[u8]]) {
        let mut bytes = Vec::new();
        for batch in batches {
            bytes.extend_from_slice(&(batch.len() as u32).to_le_bytes());
            bytes.extend_from_slice(batch);
        }
        std::fs::write(path, bytes)
            .unwrap_or_else(|error| panic!("{} cannot be written: {error}", path.display()));
    }

    /// A filled list under each of the six design systems, in both schemes.
    ///
    /// The calculator shows a keypad and a readout, which leaves the two things this
    /// screen has and that one does not: rows, and the entry above them. Those are where
    /// a spacing ladder and a separator colour stop being numbers in a table and become
    /// something a person either can or cannot read, and until a picture of them exists
    /// under all six nobody has checked.
    ///
    /// A `LazyColumn` holds no rows until the Renderer asks for a window, so this takes
    /// the first frame and then the answer to one range request, which is the pair of
    /// batches a real Renderer would have applied before the first pixel.
    ///
    /// `DXC_FRAME_DIR` writes them out. Unset, which is the normal run, it still checks
    /// that every system fills the window it was asked for.
    #[test]
    fn fr14_a_filled_list_is_produced_under_every_design_system() {
        use dioxus_compose::schema::{ColorScheme, DesignSystem, Theme};

        const WINDOW_SHOWN: usize = 12;

        saved_list();
        let directory = std::env::var("DXC_FRAME_DIR").ok();
        if let Some(directory) = &directory {
            std::fs::create_dir_all(directory).expect("the frame directory can be created");
        }
        for system in [
            DesignSystem::Material3,
            DesignSystem::Cupertino,
            DesignSystem::Fluent,
            DesignSystem::Gnome,
            DesignSystem::Breeze,
            DesignSystem::Deepin,
        ] {
            for scheme in [ColorScheme::Light, ColorScheme::Dark] {
                let theme = Theme::unified(system).with_color_scheme(scheme);
                let mut host = Host::with_theme(app, theme);
                let first = host
                    .rebuild()
                    .expect("the first frame failed to encode")
                    .to_vec();
                let decoded = decode_batch(&first).expect("the first frame did not decode");
                let list = decoded
                    .iter()
                    .find_map(|mutation| match mutation {
                        Mutation::Create {
                            node_id,
                            widget: WidgetKind::LazyColumn,
                        } => Some(*node_id),
                        _ => None,
                    })
                    .expect("the screen has no LazyColumn");
                let handler = decoded
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
                drop(decoded);

                let mut event = Vec::new();
                encode_event(
                    &HostEvent {
                        node_id: list,
                        handler_id: handler,
                        payload: EventPayload::RangeRequested {
                            start: 0,
                            count: WINDOW_SHOWN as u32,
                        },
                    },
                    &mut event,
                )
                .expect("the range request did not encode");
                let (window, _) = host
                    .dispatch_event(&event)
                    .expect("the range request failed");
                let window = window.to_vec();

                let rows = decode_batch(&window)
                    .expect("the window batch did not decode")
                    .iter()
                    .filter(|mutation| {
                        matches!(
                            mutation,
                            Mutation::SetProp {
                                property: PropertyKind::ItemKey,
                                ..
                            }
                        )
                    })
                    .count();
                assert_eq!(
                    rows, WINDOW_SHOWN,
                    "{system:?} {scheme:?} answered a window of {WINDOW_SHOWN} with {rows} rows, \
                     so the list it draws is not the list it was asked for"
                );

                if let Some(directory) = &directory {
                    let path = std::path::Path::new(directory)
                        .join(format!("Todo-{system:?}-{scheme:?}.bin"));
                    write_frames(&path, &[&first, &window]);
                }
            }
        }
    }

    /// Every property this screen sets has to be one the wire can name. A property the
    /// schema does not have fails the whole batch rather than just itself, so a screen that
    /// builds in Rust can still be blank on screen.
    /// A path no other run of these tests can touch.
    ///
    /// Fixed names in the system temp directory meant two checkouts testing at once wrote
    /// and deleted each other's files, and whichever lost the race failed for a reason
    /// that had nothing to do with the code. The process id is what keeps them apart.
    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("sample-scratch-{name}-{}.tmp", std::process::id()))
    }

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
    /// Every string the screen holds after the Renderer reports a window of this width.
    fn texts_at(width_dp: f32) -> Vec<String> {
        saved_list();
        dioxus_compose::window::reset_window_size();
        let mut screen = Screen::new();
        screen.request_range(0, WINDOW);
        let event = HostEvent {
            node_id: 0,
            handler_id: 0,
            payload: EventPayload::WindowSizeChanged {
                width_dp,
                height_dp: 900.0,
                class: dioxus_compose::WindowSizeClass::from_width_dp(width_dp),
            },
        };
        encode_event(&event, &mut screen.event).expect("the resize did not encode");
        let event_bytes = screen.event.clone();
        let (batch, _) = screen
            .host
            .dispatch_event(&event_bytes)
            .expect("the resize failed");
        let decoded = decode_batch(batch).expect("the resize batch did not decode");
        screen.mock.apply(&decoded);
        let texts = screen.mock.texts.values().cloned().collect();
        dioxus_compose::window::reset_window_size();
        texts
    }

    /// The widths the screen asks for after the Renderer reports a window of this width.
    fn widths_at(width_dp: f32) -> Vec<f32> {
        saved_list();
        dioxus_compose::window::reset_window_size();
        let mut host = Host::new(app);
        host.rebuild().expect("the first frame failed to encode");
        let event = HostEvent {
            node_id: 0,
            handler_id: 0,
            payload: EventPayload::WindowSizeChanged {
                width_dp,
                height_dp: 900.0,
                class: dioxus_compose::WindowSizeClass::from_width_dp(width_dp),
            },
        };
        let mut bytes = Vec::new();
        encode_event(&event, &mut bytes).expect("the resize did not encode");
        let (batch, _) = host.dispatch_event(&bytes).expect("the resize failed");
        let widths = decode_batch(batch)
            .expect("the resize batch did not decode")
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::SetModifier {
                    modifier: dioxus_compose::Modifier::Width(width),
                    ..
                } => Some(*width),
                _ => None,
            })
            .collect();
        dioxus_compose::window::reset_window_size();
        widths
    }

    /// A phone shortens the worded actions, because at 400dp the words leave the title no
    /// room, and drops the line that explains the demo control. A desktop window stops the
    /// screen widening and centres it, because a one line item read across 1200dp cannot
    /// be scanned.
    ///
    /// The row's own actions are not in this any more: they are entries in the row's menu,
    /// so they are read at their full length at every width and never crowd the title.
    #[test]
    fn fr20_the_screen_shortens_on_a_phone_and_stops_widening_on_a_desktop() {
        let narrow = texts_at(420.0);
        assert!(narrow.iter().any(|text| text == "Clear"), "{narrow:?}");
        assert!(!narrow.iter().any(|text| text == "Clear completed"));
        assert!(narrow.iter().any(|text| text == "+5000"));

        let wide = texts_at(1200.0);
        assert!(
            wide.iter().any(|text| text == "Clear completed"),
            "{wide:?}"
        );
        assert!(wide.iter().any(|text| text == "Add 5000 tasks"));

        assert!(widths_at(420.0).is_empty());
        assert!(
            widths_at(1200.0).contains(&dioxus_compose::WindowSizeClass::EXPANDED_MIN_WIDTH_DP),
            "the screen did not take a measure"
        );
    }

    /// A row carries one action control, and what it can do sits behind it.
    ///
    /// Edit, an up arrow, a down arrow and a delete on the face of every row is four
    /// permanently drawn controls per task, and twenty thousand of them once this list is
    /// filled. They are entries in the row's menu now, which is one control on the row and
    /// nothing drawn until it is opened.
    #[test]
    fn fr20_a_row_carries_one_action_control_and_hides_the_rest() {
        let mut screen = Screen::new();
        screen.request_range(0, WINDOW);

        assert_eq!(
            screen.mock.count_of(WidgetKind::Menu),
            WINDOW,
            "each visible row should carry exactly one action menu"
        );
        assert_eq!(screen.mock.nodes_with_text("\u{22ef}").len(), WINDOW);
        for action in ["Edit", "Move up", "Move down", "Delete"] {
            let nodes = screen.mock.nodes_with_text(action);
            assert_eq!(nodes.len(), WINDOW, "{action} is not once per visible row");
            for node in nodes {
                assert!(
                    screen.mock.is_inside(node, WidgetKind::Menu),
                    "{action} is on the face of a row rather than inside its menu"
                );
            }
        }
    }

    /// The three filters are three destinations, and this screen never asks how wide the
    /// window is to draw them. Which of the three shapes they take is the Renderer's,
    /// which is what the Renderer's own tests check at each width.
    #[test]
    fn fr21_the_filters_are_a_destination_set_rather_than_a_row_of_buttons() {
        let screen = Screen::new();
        assert_eq!(screen.mock.count_of(WidgetKind::Navigation), 1);
        assert_eq!(screen.mock.count_of(WidgetKind::NavigationItem), 3);
        for label in ["All", "Active", "Done"] {
            assert_eq!(
                screen.mock.nodes_with_text(label).len(),
                1,
                "{label} should be one destination and nothing else"
            );
        }
    }

    /// Deleting throws work away without asking first, so it says what it did and offers
    /// the task back. The message is not a widget this screen placed: no node is created
    /// for it, and nothing here holds a timer for taking it away again.
    #[test]
    fn fr21_deleting_a_task_says_so_and_offers_it_back() {
        let mut screen = Screen::new();
        screen.request_range(0, WINDOW);
        let before = screen.mock.node_count();
        let delete = screen
            .mock
            .nodes_with_text("Delete")
            .into_iter()
            .min()
            .expect("no row offered a Delete");

        let said = screen.click(delete);
        assert_eq!(screen.mock.item_count, Some(SAVED_TASKS as i64 - 1));
        let [
            OwnedRecord::Message {
                handler_id,
                text,
                action,
            },
        ] = said.as_slice()
        else {
            panic!("deleting said {said:?} rather than one message");
        };
        assert!(text.starts_with("Deleted"), "{text}");
        assert_eq!(action, "Undo");
        assert!(
            screen.mock.node_count() <= before,
            "saying something cost widgets: {before} nodes became {}",
            screen.mock.node_count(),
        );

        // The action names no node, because the message owns none.
        screen.dispatch(HostEvent {
            node_id: 0,
            handler_id: *handler_id,
            payload: EventPayload::Clicked,
        });
        assert_eq!(screen.mock.item_count, Some(SAVED_TASKS as i64));
    }

    /// costs widgets in proportion to the twenty. This is the test that fails the day
    /// windowing degrades into building everything.
    #[test]
    fn fr8_node_count_is_proportional_to_the_window() {
        let mut screen = Screen::new();
        screen.request_range(4_000, WINDOW);

        // A row is a handful of widgets: the column holding it, the row itself, the
        // toggle, the title, the action menu with its anchor and its four entries, and
        // the hairline under it. The screen's own chrome is a fixed handful on top of
        // that. What matters is that the total tracks the window and not the list behind
        // it, which is why this is a per-row multiplier rather than a fixed total.
        const PER_ROW: usize = 12;
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
