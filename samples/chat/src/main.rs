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

use assistant::{Length, Settings, Turns};
use dioxus_compose::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: u64,
    /// The turn this message belongs to. A stale worker compares it before appending.
    pub token: u64,
    /// Which conversation said it. One list holds every conversation's messages so the
    /// worker can keep appending to the last one it was given without knowing that the
    /// person reading has moved to a different conversation meanwhile.
    pub conversation: u64,
    pub from_user: bool,
    pub text: String,
    pub streaming: bool,
}

/// One conversation in the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Conversation {
    /// Never reused, so it is the destination's key and what a message belongs to.
    id: u64,
    /// What the sidebar calls it. Empty until the first thing is said in it, because
    /// the reference names a conversation after its opening line.
    title: String,
}

impl Conversation {
    /// What a conversation with nothing in it is called.
    const UNTITLED: &'static str = "New chat";

    fn label(&self) -> String {
        if self.title.is_empty() {
            Self::UNTITLED.to_owned()
        } else {
            self.title.clone()
        }
    }
}

/// How many characters of the opening line become the conversation's name.
///
/// Long enough to tell two conversations apart and short enough to sit in a rail. The
/// reference does the same thing with about the same amount of room.
const TITLE_CHARS: usize = 24;

fn title_from(text: &str) -> String {
    let mut title: String = text.chars().take(TITLE_CHARS).collect();
    if text.chars().nth(TITLE_CHARS).is_some() {
        title.push('\u{2026}');
    }
    title
}

/// How many conversations stand in the destination set.
///
/// The reference's sidebar shows recent chats and keeps the rest behind a "show more",
/// and there is a harder reason than taste: the same declaration is a bar along the
/// bottom of a phone, and a bar that holds every conversation ever started is a bar with
/// nothing legible in it. Five is the published ceiling for a bottom bar in the design
/// system this project takes its size classes from.
const RECENT_CONVERSATIONS: usize = 5;

/// A new conversation has nothing in it.
///
/// It used to open with one message from the assistant explaining what the screen was,
/// which put an introduction in the transcript: something the assistant never said, under
/// its name, that you could scroll back to a week later and read as part of the
/// conversation. The reference does not do that. An empty conversation is empty, and what
/// the screen is gets said by the screen, in the middle, until there is something to read.
fn opening_messages() -> Vec<Message> {
    Vec::new()
}

/// What stands in the middle of a conversation that has not started.
///
/// Not a widget in the scrollback: the list is genuinely empty, and this sits over it. It
/// leaves as soon as there is a first message, which is why it can say the thing that is
/// only true before then.
fn opening_greeting() -> Element {
    rsx! {
        Column {
            padding_role: SpaceRole::Lg,
            space_role: SpaceRole::Sm,
            alignment: Alignment::Center,
            Text {
                text: "Ask me something",
                type_role: TypeRole::Headline,
                text_align: TextAlign::Center,
            }
            Text {
                text: "I answer slowly and at length, on a thread that is not this one. \
                       Enter sends, Shift+Enter starts a new line.",
                type_role: TypeRole::Body,
                text_align: TextAlign::Center,
                color: Paint::Role(ColorRole::OnSurfaceVariant),
            }
        }
    }
}

/// The widest a thread is allowed to be, per class.
///
/// A line of text stops being readable somewhere around sixty to eighty characters, and a
/// window twice that wide does not make it more readable, it makes it worse. So the thread
/// stops growing and centres itself instead. The two bounds are the class boundaries
/// themselves: a medium window reads at the width a medium window starts at, and an
/// expanded one at the width an expanded one starts at.
fn thread_width(window: &WindowSize) -> Option<f32> {
    match window.class {
        WindowSizeClass::Compact => None,
        WindowSizeClass::Medium => Some(WindowSizeClass::MEDIUM_MIN_WIDTH_DP),
        WindowSizeClass::Expanded => Some(WindowSizeClass::EXPANDED_MIN_WIDTH_DP),
    }
}

/// The bar across the top of the window, with its contents held to the thread's measure.
///
/// A bar spans the window because it belongs to the window. Its contents belong to the
/// conversation, and a title that starts at the window's edge while the thread it names
/// starts two hundred dp further in is a window whose two halves disagree about where the
/// left side is. So the bar fills, and the row inside it is the same width as the thread
/// and carries the same inset.
///
/// `measure` is `None` on a window with nothing to spare, where the row fills the bar and
/// the bar's own inset is already the thread's.
fn thread_bar(measure: Option<f32>, busy: bool, children: Element) -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            TopAppBar {
                fill_max_width: true,
                dioxus_compose::Box {
                    weight: 1.0,
                    alignment: Alignment::Center,
                    Row {
                        width: measure,
                        fill_max_width: measure.is_none(),
                        padding_role: measure.map(|_| SpaceRole::Md),
                        space_role: SpaceRole::Sm,
                        alignment: Alignment::CenterStart,
                        {children}
                    }
                }
            }
            // A reply arriving is work in progress, and a line under the bar is what every
            // one of these systems uses to say so. It replaced a word in the corner that
            // said "assistant is replying": the word was correct and nobody looks at the
            // corner while they are reading the middle. Indeterminate, because the
            // assistant does not know how long its answer is going to be either.
            if busy {
                ProgressIndicator { determinate: false }
            }
        }
    }
}

/// The assistant's settings, as a panel that can stand on its own.
///
/// Everything here changes what the worker does with the next reply, which is the
/// difference between a settings screen and a picture of one.
fn settings_panel(
    settings: Settings,
    change: EventHandler<Settings>,
    close: EventHandler<()>,
) -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            space_role: SpaceRole::Md,
            Row {
                fill_max_width: true,
                alignment: Alignment::CenterStart,
                Text { text: "Assistant", type_role: TypeRole::Subtitle, weight: 1.0 }
                Button {
                    text: "Done",
                    variant: ButtonVariant::Filled,
                    on_click: move |_| close.call(()),
                }
            }
            Separator {}

            // One of three, which is what a radio group is for. A dropdown would hide two
            // of them behind a tap for no gain at this size.
            Text {
                text: "Reply length",
                type_role: TypeRole::Label,
                color: Paint::Role(ColorRole::OnSurfaceVariant),
            }
            for length in Length::ALL {
                Row {
                    key: "{length.label()}",
                    fill_max_width: true,
                    space_role: SpaceRole::Sm,
                    alignment: Alignment::CenterStart,
                    RadioButton {
                        selected: settings.length == length,
                        on_change: move |_| change.call(Settings { length, ..settings }),
                    }
                    Text { text: length.label(), type_role: TypeRole::Body }
                }
            }

            Separator {}
            Row {
                fill_max_width: true,
                space_role: SpaceRole::Sm,
                alignment: Alignment::CenterStart,
                Column {
                    weight: 1.0,
                    Text { text: "Type the reply out", type_role: TypeRole::Body }
                    Text {
                        text: "Off, and the whole answer lands at once.",
                        type_role: TypeRole::Caption,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                }
                Switch {
                    checked: settings.streaming,
                    on_change: move |streaming| {
                        change.call(Settings { streaming, ..settings })
                    },
                }
            }

            // The speed only means anything while the reply is being typed out, so it is
            // disabled rather than hidden: a control that vanishes takes the explanation
            // of what the switch above it does with it.
            Text {
                text: "{settings.speed as u32} characters a second",
                type_role: TypeRole::Label,
                color: Paint::Role(ColorRole::OnSurfaceVariant),
            }
            Slider {
                value: settings.speed,
                min: assistant::SLOWEST,
                max: assistant::FASTEST,
                enabled: settings.streaming,
                on_change: move |speed| change.call(Settings { speed, ..settings }),
            }
        }
    }
}

fn app() -> Element {
    let window = use_window_size();
    let measure = thread_width(&window);
    let crowded = window.is_compact();
    // Shared with the assistant thread, so it is a sync signal rather than the usual one.
    // Writing it from the worker marks this scope dirty through a channel the scheduler
    // owns, and the Host asks for the frame.
    let mut messages = use_signal_sync(opening_messages);
    let mut next_id = use_signal(|| 2_u64);
    let mut draft = use_signal(String::new);
    let turns = use_signal(Turns::default);
    let mut settings = use_signal(Settings::default);
    let mut settings_open = use_signal(|| false);
    // The conversations, newest last, and the one being read. A chat application with one
    // conversation is a chat application with the part people use missing, and the
    // reference's sidebar is mostly this list.
    let mut conversations = use_signal(|| {
        vec![Conversation {
            id: 1,
            title: String::new(),
        }]
    });
    let mut current = use_signal(|| 1_u64);
    let mut next_conversation = use_signal(|| 2_u64);

    let mut send = move |text: String| {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        let token = turns.peek().begin();
        let id = next_id();
        next_id.set(id + 2);
        let conversation = current();
        {
            let mut list = messages.write();
            list.push(Message {
                id,
                token,
                conversation,
                from_user: true,
                text: text.clone(),
                streaming: false,
            });
            // The empty reply goes in now, on the UI thread, so the worker only ever has to
            // append to a message that already exists.
            list.push(Message {
                id: id + 1,
                token,
                conversation,
                from_user: false,
                text: String::new(),
                streaming: true,
            });
        }
        // A conversation is named after the first thing said in it, which is how the
        // sidebar tells one from another without asking anybody to name anything.
        {
            let mut list = conversations.write();
            if let Some(entry) = list.iter_mut().find(|entry| entry.id == conversation) {
                if entry.title.is_empty() {
                    entry.title = title_from(&text);
                }
            }
        }
        draft.set(String::new());
        assistant::stream_reply(text, token, turns.peek().clone(), settings(), messages);
    };

    // What is on screen is one conversation's messages. The list holds every
    // conversation's, so this is the window into it, the same shape the task list uses
    // for its filters.
    let thread: Vec<usize> = messages
        .read()
        .iter()
        .enumerate()
        .filter(|(_, message)| message.conversation == current())
        .map(|(index, _)| index)
        .collect();
    let count = thread.len();
    let busy = messages
        .read()
        .last()
        .is_some_and(|message| message.streaming && message.conversation == current());
    let keys: Vec<String> = thread
        .iter()
        .map(|index| messages.read()[*index].id.to_string())
        .collect();
    let rows = thread.clone();
    // Newest first in the sidebar, and only as many as a set of destinations can hold.
    let recent: Vec<Conversation> = conversations()
        .iter()
        .rev()
        .take(RECENT_CONVERSATIONS)
        .cloned()
        .collect();
    let selected = recent
        .iter()
        .position(|entry| entry.id == current())
        .unwrap_or(0);

    rsx! {
        // The conversations are the destination set, which is the reference's sidebar.
        // This code never asks how wide the window is for it: the Renderer has measured
        // the window and draws a bar along the bottom of a phone, a rail beside a tablet
        // and a sidebar standing open on a desktop, from these same items.
        Navigation {
            fill_max_width: true,
            fill_max_height: true,
            selected_index: selected,
            for conversation in recent.iter().cloned() {
                NavigationItem {
                    key: "{conversation.id}",
                    text: conversation.label(),
                    // A rail is allowed to drop the labels, so a destination that is
                    // nothing but a title would be a blank strip in one of the three
                    // presentations.
                    icon: IconRole::Inbox,
                    on_click: {
                        let id = conversation.id;
                        move |()| current.set(id)
                    },
                }
            }
        Column {
            fill_max_width: true,
            fill_max_height: true,

            {thread_bar(measure, busy, rsx! {
                Text { text: "Chat", type_role: TypeRole::Title, weight: 1.0 }
                Button {
                    text: "Assistant",
                    variant: ButtonVariant::Text,
                    on_click: move |_| settings_open.set(true),
                }
                // Deleting is the one thing here that throws a conversation away, so it
                // says what it did and offers it back. Starting a new one no longer
                // destroys anything: the old conversation is still in the sidebar.
                Button {
                    text: "Delete",
                    variant: ButtonVariant::Text,
                    color: Paint::Role(ColorRole::Error),
                    enabled: conversations.read().len() > 1,
                    on_click: move |_| {
                        let gone = current();
                        let entries = conversations();
                        let Some(at) = entries.iter().position(|entry| entry.id == gone) else {
                            return;
                        };
                        let removed = entries[at].clone();
                        let lines = messages();
                        // The worker is already handled: beginning a turn bumps the token,
                        // and a worker whose token is no longer current stops at its next
                        // chunk rather than appending to a conversation that has gone.
                        turns.peek().begin();
                        conversations.write().remove(at);
                        messages
                            .write()
                            .retain(|message| message.conversation != gone);
                        // There is always somewhere to be. Deleting the last one leaves an
                        // empty conversation rather than a screen with no conversation in
                        // it, which is a state the rest of this screen cannot draw.
                        if conversations.read().is_empty() {
                            let id = next_conversation();
                            next_conversation.set(id + 1);
                            conversations.write().push(Conversation {
                                id,
                                title: String::new(),
                            });
                        }
                        let next = conversations.read()[at.min(conversations.read().len() - 1)].id;
                        current.set(next);
                        // Spelled out, because this file already has a `Message` and it is
                        // a line of a conversation. The library's is the one sentence an
                        // application says after something happened.
                        dioxus_compose::Message::new(format!(
                            "Deleted \u{201c}{}\u{201d}",
                            removed.label()
                        ))
                        .with_action("Undo", move |()| {
                            conversations.write().insert(at, removed.clone());
                            messages.set(lines.clone());
                            current.set(gone);
                        })
                        .with_duration(MessageDuration::Long)
                        .show();
                    },
                }
                Button {
                    text: if crowded { "New" } else { "New conversation" },
                    variant: ButtonVariant::Text,
                    on_click: move |_| {
                        // Nothing is thrown away: the conversation that was on screen
                        // stays in the sidebar, which is what the reference does and what
                        // makes the sidebar worth having.
                        let id = next_conversation();
                        next_conversation.set(id + 1);
                        conversations.write().push(Conversation {
                            id,
                            title: String::new(),
                        });
                        current.set(id);
                    },
                }
            })}

            // On a narrow window the thread is the window. On anything wider it is a
            // column of its own, centred, with the page showing either side of it.
            dioxus_compose::Box {
                fill_max_width: true,
                fill_max_height: true,
                alignment: Alignment::TopCenter,
            // The thread is a reading surface, and the incoming bubble is a fill on it.
            //
            // Separation by fill was the right call and the wrong page to do it on. The
            // page was the grouped background and the bubble was the quiet fill, which in
            // Cupertino is 0xe9e9eb on 0xf2f2f7: enough apart to pass a contrast check and
            // not enough to see. Position cannot carry it on its own either, because a
            // left-aligned run of unfilled text beside a filled run of the user's reads as
            // one speaker with a highlighter. So the fill stays and the page moves: a
            // conversation is something you read, the reading surface is `Surface`, and
            // the quiet fill is guaranteed to be visible against it. That is what the
            // three roles are for.
            Column {
                fill_max_width: measure.is_none(),
                width: measure,
                fill_max_height: true,
                background: Paint::Role(ColorRole::Surface),
                // The medium step, because that is what the bar insets its own contents
                // by. Anything else and the title and the thread under it start at two
                // different places.
                padding_role: SpaceRole::Md,
                space_role: SpaceRole::Md,

                // The scrollback and, while there is nothing in it, what the screen is.
                // The list is declared either way: a conversation that begins by building
                // a list is a conversation whose first message arrives a frame late.
                dioxus_compose::Box {
                    fill_max_width: true,
                    weight: 1.0,
                    alignment: Alignment::Center,
                LazyColumn {
                    fill_max_width: true,
                    fill_max_height: true,
                    item_count: count,
                    key_of: move |index: usize| keys[index].clone(),
                    item: move |position: usize| {
                        let index = rows[position];
                        let message = messages.read()[index].clone();
                        // A name over every bubble is a name repeated once per line. The
                        // side and the fill already say who is speaking, so the name is
                        // printed once at the head of a run and the rest of the run is
                        // read as the same speaker still talking.
                        let starts_a_run = position == 0
                            || messages.read()[rows[position - 1]].from_user != message.from_user;
                        // Who said it should be readable without reading, so it is the side
                        // the bubble sits on and the colour it is filled with, with the
                        // name left as confirmation rather than as the only clue. Both
                        // colours are roles, so the user's bubble is the accent of
                        // whichever design system is running and the reply is that
                        // system's quiet surface.
                        let (fill, ink) = if message.from_user {
                            (ColorRole::Primary, ColorRole::OnPrimary)
                        } else {
                            (ColorRole::SurfaceVariant, ColorRole::OnSurfaceVariant)
                        };
                        rsx! {
                            // The list has no spacing of its own, so the gap between one
                            // message and the next is padding on the row that holds it.
                            dioxus_compose::Box {
                                fill_max_width: true,
                                // Consecutive messages from one speaker sit close
                                // together and a change of speaker gets more air, which is
                                // what makes a conversation read as turns rather than as
                                // an evenly spaced column of boxes.
                                padding_role: if starts_a_run {
                                    SpaceRole::Sm
                                } else {
                                    SpaceRole::Xs
                                },
                                alignment: if message.from_user {
                                    Alignment::CenterEnd
                                } else {
                                    Alignment::CenterStart
                                },
                                Column {
                                    space_role: SpaceRole::Xs,
                                    alignment: if message.from_user {
                                        Alignment::CenterEnd
                                    } else {
                                        Alignment::CenterStart
                                    },
                                    if starts_a_run {
                                        Text {
                                            text: if message.from_user { "You" } else { "Assistant" },
                                            type_role: TypeRole::Caption,
                                            color: Paint::Role(ColorRole::OnSurfaceVariant),
                                        }
                                    }
                                    // The bubble sizes to its text, so a short reply is a
                                    // short bubble. Its corner is the design system's
                                    // large corner rather than a radius chosen here.
                                    Column {
                                        background: Paint::Role(fill),
                                        shape_role: ShapeRole::Large,
                                        padding_role: SpaceRole::Md,
                                        Text {
                                            // A message still arriving shows a caret so an
                                            // empty reply does not look like a dead one.
                                            text: if message.streaming {
                                                format!("{}\u{2589}", message.text)
                                            } else {
                                                message.text.clone()
                                            },
                                            type_role: TypeRole::Body,
                                            color: Paint::Role(ink),
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
                if count == 0 {
                    {opening_greeting()}
                }
                }

                // The composer, as the reference has it: one rounded bar floating at the
                // foot of the page, holding everything that belongs to sending a message.
                // It used to be a field with a button parked beside it inside a square
                // panel, which is two controls that happen to be adjacent.
                //
                // A `Card` rather than a `Surface`, because floating is the whole
                // difference between the two: the design system's raised container already
                // knows what lifting something off a page looks like in that system, and a
                // shadow depth chosen in this file would only be right in one of them.
                Card {
                    fill_max_width: true,
                    shape_role: ShapeRole::Full,
                    Row {
                        fill_max_width: true,
                        space_role: SpaceRole::Sm,
                        alignment: Alignment::CenterStart,
                        // No `on_key_down` here on purpose. The Renderer already treats
                        // Enter in a multiline field that has a submit handler as "send"
                        // and Shift+Enter as "new line", and `on_submit` carries the text
                        // the field holds at that instant. A key handler would have to read
                        // the separately reported value, which lags typing by the field's
                        // change debounce, so the last characters typed before Enter would
                        // be dropped.
                        TextField {
                            weight: 1.0,
                            multiline: true,
                            placeholder: "Message. Enter sends, Shift+Enter starts a new line",
                            on_value_change: move |value| draft.set(value),
                            on_submit: move |value: String| send(value),
                        }
                        // What the reference calls the model, which is the one thing
                        // about an answer you can choose before asking for it. It sits in
                        // the composer, where that choice is made, as well as in the
                        // settings, where everything about the assistant is.
                        Dropdown {
                            selected_index: settings().length.index(),
                            on_change: move |index: usize| {
                                let length = Length::ALL[index.min(Length::ALL.len() - 1)];
                                settings.set(Settings { length, ..settings() });
                            },
                            for length in Length::ALL {
                                Text { key: "{length.label()}", text: length.label() }
                            }
                        }
                        Button {
                            text: "Send",
                            variant: ButtonVariant::Filled,
                            shape_role: ShapeRole::Full,
                            on_click: move |_| send(draft()),
                        }
                    }
                }
            }
            }

            // The settings arrive from an edge rather than taking the screen: what they
            // change is the conversation behind them, and covering it to change it would
            // hide the thing being changed. Which edge is the Renderer's decision.
            Sheet {
                open: settings_open(),
                on_dismiss: move |_| settings_open.set(false),
                fill_max_width: true,
                {settings_panel(
                    settings(),
                    EventHandler::new(move |next| settings.set(next)),
                    EventHandler::new(move |()| settings_open.set(false)),
                )}
            }
        }
        }
    }
}

// Samples are demonstrations, so they let you see any of the design systems rather than
// only the one this machine happens to select. Unset, the app adapts to the host platform,
// which is what a real application wants.
fn main() {
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
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::collections::{HashMap, HashSet};
    use std::time::Duration;

    /// A click or a keystroke in a steady state screen changes one node. Anything above
    /// this means something is allocating per event rather than reusing a buffer.
    const HOST_ALLOCATION_CEILING: usize = 200;
    const REPEATED_INTERACTIONS: usize = 100;

    struct CountingAllocator;

    // Counting is per thread, not per process: the harness runs tests concurrently and the
    // assistant runs on a worker, so a global counter would attribute other threads'
    // allocations to whichever measurement happens to be open.
    thread_local! {
        static TRACKING: Cell<bool> = const { Cell::new(false) };
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    fn record_allocation() {
        // `try_with` because a thread tearing down its locals must not re-enter them.
        let _ = TRACKING.try_with(|tracking| {
            if tracking.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        });
    }

    // SAFETY: Every operation delegates to the process System allocator unchanged.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            // SAFETY: Delegating the caller-provided layout to System.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: Delegating the original pointer and layout to System.
            unsafe { System.dealloc(ptr, layout) };
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            record_allocation();
            // SAFETY: Delegating the original allocation and requested size to System.
            unsafe { System.realloc(ptr, layout, size) }
        }
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    struct AllocationMeasurement;

    impl AllocationMeasurement {
        fn start() -> Self {
            ALLOCATIONS.with(|count| count.set(0));
            TRACKING.with(|tracking| tracking.set(true));
            Self
        }

        fn finish(self) -> usize {
            TRACKING.with(|tracking| tracking.set(false));
            ALLOCATIONS.with(|count| count.get())
        }
    }

    /// The real chat screen, driven the way the Renderer drives it.
    struct Screen {
        host: Host,
        /// The first frame, kept so a control the screen declared once can still be found
        /// after other frames have gone by.
        first: Vec<u8>,
        scrollback: u32,
        range_handler: u64,
        composer: u32,
        submit_handler: u64,
        change_handler: u64,
        /// The text of every node, as the frames report it.
        texts: HashMap<u32, String>,
        /// The destination nodes that are still on screen, which is the sidebar.
        destinations: Vec<u32>,
        /// Every message the screen has said, in order, with its action label.
        messages: Vec<(String, String)>,
        event: Vec<u8>,
    }

    impl Screen {
        fn new() -> Self {
            let mut host = Host::new(app);
            let bytes = host
                .rebuild()
                .expect("the first frame failed to encode")
                .to_vec();
            let first = decode_batch(&bytes).expect("the first frame did not decode");
            let node_of = |widget: WidgetKind| {
                first
                    .iter()
                    .find_map(|mutation| match mutation {
                        Mutation::Create {
                            node_id,
                            widget: found,
                        } if *found == widget => Some(*node_id),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("the screen has no {widget:?}"))
            };
            let scrollback = node_of(WidgetKind::LazyColumn);
            let composer = node_of(WidgetKind::TextField);
            let handler_of = |node: u32, property: PropertyKind| {
                first
                    .iter()
                    .find_map(|mutation| match mutation {
                        Mutation::SetProp {
                            node_id,
                            property: found,
                            value: PropertyValue::Integer(id),
                        } if *node_id == node && *found == property => Some(*id as u64),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("no {property:?} handler was declared"))
            };
            let range_handler = handler_of(scrollback, PropertyKind::OnRangeRequested);
            let submit_handler = handler_of(composer, PropertyKind::OnSubmit);
            let change_handler = handler_of(composer, PropertyKind::OnValueChange);
            let texts = first
                .iter()
                .filter_map(|mutation| match mutation {
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::Text,
                        value: PropertyValue::String(text),
                    } => Some((*node_id, (*text).to_owned())),
                    _ => None,
                })
                .collect();
            let destinations = first
                .iter()
                .filter_map(|mutation| match mutation {
                    Mutation::Create {
                        node_id,
                        widget: WidgetKind::NavigationItem,
                    } => Some(*node_id),
                    _ => None,
                })
                .collect();
            drop(first);
            Self {
                host,
                first: bytes,
                scrollback,
                range_handler,
                composer,
                submit_handler,
                change_handler,
                texts,
                destinations,
                messages: Vec::new(),
                event: Vec::new(),
            }
        }

        /// Every node the screen created as this widget, in declaration order.
        fn nodes_of(&self, widget: WidgetKind) -> Vec<u32> {
            decode_batch(&self.first)
                .expect("the first frame did not decode")
                .iter()
                .filter_map(|mutation| match mutation {
                    Mutation::Create {
                        node_id,
                        widget: found,
                    } if *found == widget => Some(*node_id),
                    _ => None,
                })
                .collect()
        }

        fn handler_of(&self, node: u32, property: PropertyKind) -> u64 {
            decode_batch(&self.first)
                .expect("the first frame did not decode")
                .iter()
                .find_map(|mutation| match mutation {
                    Mutation::SetProp {
                        node_id,
                        property: found,
                        value: PropertyValue::Integer(id),
                    } if *node_id == node && *found == property => Some(*id as u64),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("node {node} declared no {property:?} handler"))
        }

        /// Applies a batch, keeping what the screen is showing and what it has said.
        fn absorb(&mut self, batch: &[u8]) {
            for mutation in decode_batch(batch).expect("a frame did not decode") {
                match mutation {
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::Text,
                        value: PropertyValue::String(text),
                    } => {
                        self.texts.insert(node_id, text.to_owned());
                    }
                    Mutation::Create {
                        node_id,
                        widget: WidgetKind::NavigationItem,
                    } => self.destinations.push(node_id),
                    Mutation::Remove { node_id } => {
                        self.destinations.retain(|found| *found != node_id);
                    }
                    Mutation::ShowMessage { text, action, .. } => {
                        self.messages.push((text.to_owned(), action.to_owned()));
                    }
                    _ => {}
                }
            }
        }

        /// What the sidebar is offering, by label. Sorted, because what the destination
        /// set holds is the claim here and the order it is drawn in is the Renderer's.
        fn destinations(&self) -> Vec<String> {
            let mut labels: Vec<String> = self
                .destinations
                .iter()
                .filter_map(|node| self.texts.get(node).cloned())
                .collect();
            labels.sort();
            labels
        }

        /// Sends an event and keeps what came back.
        fn dispatch(&mut self, node_id: u32, handler_id: u64, payload: EventPayload<'_>) {
            self.encode(node_id, handler_id, payload);
            let batch = self
                .host
                .dispatch_event(&self.event)
                .expect("the event failed")
                .0
                .to_vec();
            self.absorb(&batch);
        }

        /// Presses what a person would read as this label.
        fn press(&mut self, label: &str) {
            let node_id = *self
                .texts
                .iter()
                .find(|(_, text)| *text == label)
                .map(|(node_id, _)| node_id)
                .unwrap_or_else(|| panic!("the screen has nothing labelled {label}"));
            let handler = self.handler_of(node_id, PropertyKind::OnClick);
            self.dispatch(node_id, handler, EventPayload::Clicked);
        }

        /// Polls frames until the assistant has stopped typing, the way the Renderer does
        /// after the Host asks for one.
        fn settle(&mut self) -> String {
            for _ in 0..600 {
                std::thread::sleep(Duration::from_millis(5));
                let batch = self
                    .host
                    .render_frame(0)
                    .expect("a frame failed to encode")
                    .to_vec();
                self.absorb(&batch);
                let reply = self.reply();
                // The caret is on a message that is still arriving.
                if !reply.is_empty() && !reply.ends_with('\u{2589}') {
                    return reply;
                }
            }
            panic!("the reply never finished arriving: {:?}", self.reply());
        }

        /// The last thing the assistant said, as the screen shows it.
        fn reply(&self) -> String {
            self.texts
                .iter()
                .filter(|(_, text)| {
                    text.starts_with("You said:") || text.contains("Streaming works")
                })
                .max_by_key(|(node_id, _)| **node_id)
                .map(|(_, text)| text.clone())
                .unwrap_or_default()
        }

        fn encode(&mut self, node_id: u32, handler_id: u64, payload: EventPayload<'_>) {
            encode_event(
                &HostEvent {
                    node_id,
                    handler_id,
                    payload,
                },
                &mut self.event,
            )
            .expect("the event did not encode");
        }

        /// Until the Renderer asks for a range the scrollback holds no items at all, so
        /// the window has to be opened before there is any message widget to grow.
        fn open_window(&mut self) {
            let (node, handler) = (self.scrollback, self.range_handler);
            self.encode(
                node,
                handler,
                EventPayload::RangeRequested {
                    start: 0,
                    count: 20,
                },
            );
            self.host
                .dispatch_event(&self.event)
                .expect("the range request failed");
        }

        fn send(&mut self, text: &str) {
            let (node, handler) = (self.composer, self.submit_handler);
            self.encode(node, handler, EventPayload::TextSubmitted(text));
            let batch = self
                .host
                .dispatch_event(&self.event)
                .expect("the send failed")
                .0
                .to_vec();
            self.absorb(&batch);
        }

        fn type_into_composer(&mut self, text: &str) -> usize {
            let (node, handler) = (self.composer, self.change_handler);
            self.encode(node, handler, EventPayload::TextChanged(text));
            let measurement = AllocationMeasurement::start();
            let (batch, result) = self
                .host
                .dispatch_event(&self.event)
                .expect("the keystroke failed");
            std::hint::black_box((batch.len(), result));
            measurement.finish()
        }
    }

    /// One frame's worth of what the wire carried.
    #[derive(Default)]
    struct Frame {
        created: usize,
        removed: usize,
        text_nodes: HashSet<u32>,
        longest_text: usize,
    }

    fn next_frame(host: &mut Host) -> Frame {
        let batch = host.render_frame(0).expect("a frame failed to encode");
        let mut frame = Frame::default();
        for mutation in decode_batch(batch).expect("a frame did not decode") {
            match mutation {
                Mutation::Create { .. } => frame.created += 1,
                Mutation::Remove { .. } => frame.removed += 1,
                Mutation::SetProp {
                    node_id,
                    property: PropertyKind::Text,
                    value: PropertyValue::String(text),
                } => {
                    frame.text_nodes.insert(node_id);
                    frame.longest_text = frame.longest_text.max(text.len());
                }
                _ => {}
            }
        }
        frame
    }

    /// The conversation, under every design system, in both schemes, at all three widths.
    ///
    /// A thread is the screen where a fill that is a shade away from the page behind it
    /// stops being a contrast number and becomes a bubble nobody can see. The scrollback
    /// windows its rows, so the recorder answers the range request a real Renderer would
    /// have made before the first pixel.
    #[test]
    fn fr14_the_conversation_is_recorded_under_every_design_system_and_width() {
        sample_frames::record("Chat", app, |screen| {
            assert_eq!(
                screen.fill_lists(8),
                1,
                "the screen should hold exactly one windowing list, or the picture is of \
                 something other than the scrollback"
            );
        });
    }

    /// The same screen with the assistant's settings open.
    ///
    /// A second recording rather than a flag on the first, because a sheet covers what it
    /// is over: one picture cannot be of both. This one holds a radio group, a switch and
    /// a slider, which is three of the newest widgets in the vocabulary and the place a
    /// design system that has not drawn them yet would show it.
    #[test]
    fn fr21_the_settings_sheet_is_recorded_under_every_design_system_and_width() {
        sample_frames::record("ChatSettings", app, |screen| {
            screen.fill_lists(8);
            assert!(
                screen.press("Assistant"),
                "the screen has no way to open the assistant's settings"
            );
        });
    }

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
    #[test]
    fn pr3_a_reply_arrives_from_the_worker_thread() {
        let mut screen = Screen::new();
        screen.open_window();
        screen.send("tell me about threads");

        // Poll frames the way the Renderer would after the Host asked for one. The reply
        // starts after a short pause, so a second of frames is far more than it needs.
        let mut longest = 0usize;
        for _ in 0..600 {
            std::thread::sleep(Duration::from_millis(5));
            longest = longest.max(next_frame(&mut screen.host).longest_text);
            if longest > 200 {
                break;
            }
        }
        assert!(
            longest > 200,
            "the worker's reply never reached a frame; the longest text sent was {longest} bytes"
        );
    }

    /// A reply streaming in must not rebuild the scrollback around it. The message that is
    /// growing is the only one whose text changes, the conversation above it keeps the
    /// widgets it already had, and nothing is created or destroyed frame after frame.
    #[test]
    fn fr9_a_streaming_reply_leaves_the_rest_of_the_scrollback_alone() {
        let mut screen = Screen::new();
        screen.open_window();
        screen.send("tell me about streaming");

        let mut streaming_frames = 0usize;
        let mut widest = 0usize;
        for _ in 0..600 {
            std::thread::sleep(Duration::from_millis(5));
            let frame = next_frame(&mut screen.host);
            if frame.text_nodes.is_empty() {
                continue;
            }
            streaming_frames += 1;
            widest = widest.max(frame.text_nodes.len());
            assert_eq!(
                frame.created, 0,
                "a frame created {} widgets while a reply was streaming; the scrollback is \
                 being rebuilt rather than appended to",
                frame.created
            );
            assert_eq!(
                frame.removed, 0,
                "a frame removed {} widgets while a reply was streaming",
                frame.removed
            );
            if frame.longest_text > 200 {
                break;
            }
        }
        assert!(
            streaming_frames > 5,
            "only {streaming_frames} frames carried any text; the reply never streamed"
        );
        // The growing message, and at its edges the status line that says whether the
        // assistant is still replying.
        assert!(
            widest <= 2,
            "{widest} separate nodes had their text replaced in one streaming frame; only \
             the message that is growing should change"
        );
    }

    /// Tokens arriving between two frames cost one record, not one record each. The worker
    /// hands over a few characters at a time far faster than a frame goes out, so a frame
    /// taken after several chunks have landed carries the message once.
    #[test]
    fn fr9_tokens_landing_between_frames_coalesce_into_one_record() {
        let mut screen = Screen::new();
        screen.open_window();
        screen.send("tell me about streaming");

        let mut coalesced = false;
        let mut previous = 0usize;
        for _ in 0..40 {
            // Long enough that several chunks land between one frame and the next.
            std::thread::sleep(Duration::from_millis(100));
            let frame = next_frame(&mut screen.host);
            if frame.text_nodes.is_empty() {
                continue;
            }
            let grew_by = frame.longest_text.saturating_sub(previous);
            previous = frame.longest_text;
            // More than one chunk of text arrived, and the frame still named the message
            // once rather than once per chunk.
            if grew_by > 6 && frame.text_nodes.len() == 1 {
                coalesced = true;
                break;
            }
        }
        assert!(
            coalesced,
            "no frame carried more than one chunk of the reply in a single record"
        );
    }

    /// Typing into the composer is the steady state of this screen. A keystroke changes
    /// one node, so the Host must not allocate in proportion to the conversation behind it.
    #[test]
    fn nfr9_host_path_allocation_ceiling() {
        let mut screen = Screen::new();
        screen.open_window();
        for _ in 0..2 {
            screen.type_into_composer("warm");
        }

        let allocations = screen.type_into_composer("a message being typed");

        assert!(
            allocations <= HOST_ALLOCATION_CEILING,
            "the Host allocated {allocations} times while handling a keystroke; the ceiling \
             is {HOST_ALLOCATION_CEILING}. A keystroke in a steady state screen changes one \
             node, so any growth here means something is allocating per event rather than \
             reusing a buffer."
        );
    }

    /// The widths a thread takes, read back off the wire after the Renderer reports a
    /// window of the given width.
    fn widths_at(width_dp: f32) -> Vec<f32> {
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

    /// A wide window does not get a wide thread. The column stops at the reading measure
    /// for its class and the page shows either side of it, and a narrow window keeps the
    /// full width because there is nothing to give back.
    #[test]
    fn fr20_the_thread_stops_growing_once_the_window_is_wide() {
        assert!(
            widths_at(420.0).is_empty(),
            "a compact window should not size the thread"
        );
        assert!(widths_at(700.0).contains(&dioxus_compose::WindowSizeClass::MEDIUM_MIN_WIDTH_DP));
        assert!(
            widths_at(1200.0).contains(&dioxus_compose::WindowSizeClass::EXPANDED_MIN_WIDTH_DP)
        );
    }

    /// The bar's contents and the thread are held to the same measure, so the title starts
    /// where the conversation starts.
    ///
    /// A window has one left edge for the conversation in it. The bar used to span the
    /// window while the thread was centred, so on a desktop window the title began a
    /// hundred and seventy dp to the left of the thread it named. Two nodes carrying the
    /// measure is what that agreement looks like on the wire: one is the row inside the
    /// bar, the other is the thread.
    #[test]
    fn fr20_the_bar_holds_its_contents_to_the_same_measure_as_the_thread() {
        for (width, measure) in [
            (700.0, dioxus_compose::WindowSizeClass::MEDIUM_MIN_WIDTH_DP),
            (
                1200.0,
                dioxus_compose::WindowSizeClass::EXPANDED_MIN_WIDTH_DP,
            ),
        ] {
            let widths = widths_at(width);
            assert_eq!(
                widths.iter().filter(|found| **found == measure).count(),
                2,
                "at {width}dp the bar and the thread should both be {measure}: {widths:?}"
            );
        }
    }

    /// The assistant's settings reach the worker. A reply length that changed nothing
    /// about the reply would be a control wired to a signal and nothing else.
    #[test]
    fn fr21_the_reply_length_setting_changes_what_the_assistant_says() {
        let mut screen = Screen::new();
        screen.open_window();
        screen.send("tell me about streaming");
        let normal = screen.settle();

        // The radio buttons are the only ones on the screen and they are declared in the
        // order the lengths are listed, so the first of them is Brief.
        let mut screen = Screen::new();
        screen.open_window();
        let brief_button = screen.nodes_of(WidgetKind::RadioButton)[0];
        let handler = screen.handler_of(brief_button, PropertyKind::OnValueChange);
        screen.dispatch(brief_button, handler, EventPayload::ValueChanged(1.0));
        screen.send("tell me about streaming");
        let brief = screen.settle();

        assert!(
            brief.len() < normal.len(),
            "the brief reply is not shorter: {brief:?} against {normal:?}"
        );
        assert!(
            normal.starts_with(&brief),
            "the brief reply should be the opening of the full one: {brief:?}"
        );
    }

    /// Deleting throws a conversation away, so it offers it back. Starting a new one no
    /// longer destroys anything, because the old conversation stays in the sidebar.
    #[test]
    fn fr21_deleting_a_conversation_offers_it_back() {
        let mut screen = Screen::new();
        screen.open_window();
        screen.send("tell me about streaming");
        screen.settle();

        // "New" rather than "New conversation": nothing has reported a window size, so
        // the screen is laid out for the narrowest one and the button carries its short
        // label.
        screen.press("New");
        screen.press("Delete");
        assert_eq!(
            screen.messages,
            vec![(
                "Deleted \u{201c}New chat\u{201d}".to_owned(),
                "Undo".to_owned()
            )],
            "deleting should say what it did and offer it back"
        );
    }

    /// The conversations are the destination set, which is what the reference's sidebar
    /// is. One declaration, and the Renderer draws it as a bar, a rail or a sidebar from
    /// the width it measured.
    #[test]
    fn fr22_the_conversations_are_the_destination_set() {
        let mut screen = Screen::new();
        screen.open_window();
        let before = screen.destinations();
        assert_eq!(
            before,
            vec!["New chat".to_owned()],
            "a fresh screen should offer the one conversation it has"
        );

        screen.send("tell me about streaming");
        screen.settle();
        screen.press("New");
        let after = screen.destinations();
        assert_eq!(
            after,
            vec!["New chat".to_owned(), "tell me about streaming".to_owned()],
            "the conversation that was on screen should still be in the sidebar, named \
             after its opening line"
        );
    }

    /// The reference puts everything that belongs to sending a message inside one rounded
    /// bar. A field with a button parked next to it is two controls that happen to be
    /// adjacent, which is what this used to be.
    #[test]
    fn fr22_the_composer_is_one_rounded_bar_holding_the_send() {
        let screen = Screen::new();
        let batch = decode_batch(&screen.first).expect("the first frame did not decode");

        let mut parents = HashMap::new();
        for mutation in &batch {
            if let Mutation::Insert {
                parent_id, node_id, ..
            } = mutation
            {
                parents.insert(*node_id, *parent_id);
            }
        }
        let row = parents[&screen.composer];
        let bar = parents[&row];

        let send = *screen
            .texts
            .iter()
            .find(|(_, text)| *text == "Send")
            .map(|(node_id, _)| node_id)
            .expect("the screen has nothing labelled Send");
        assert_eq!(
            parents[&send], row,
            "the send button is outside the row the field is in, so the composer is not \
             one control"
        );

        let rounded = batch.iter().any(|mutation| {
            matches!(
                mutation,
                Mutation::SetModifier { node_id, modifier: Modifier::ShapeRole(ShapeRole::Full), .. }
                    if *node_id == bar
            )
        });
        assert!(rounded, "the composer is not the reference's pill");
    }

    /// Nothing has been said yet, so the middle of the screen says what the screen is.
    /// It is not a message: an introduction under the assistant's name is something the
    /// assistant never said, sitting in the transcript for good.
    #[test]
    fn fr22_an_empty_conversation_says_what_it_is_in_the_middle() {
        let mut screen = Screen::new();
        screen.open_window();
        let greeting = "Ask me something";
        assert!(
            screen.texts.values().any(|text| text == greeting),
            "an empty conversation does not say what the screen is"
        );
        assert!(
            screen
                .texts
                .values()
                .all(|text| !text.starts_with("You said:")),
            "an empty conversation already holds a reply"
        );

        // And it leaves as soon as there is something to read, which is what lets it say
        // the thing that is only true before then.
        let node = *screen
            .texts
            .iter()
            .find(|(_, text)| *text == greeting)
            .map(|(node_id, _)| node_id)
            .expect("the greeting has no node");
        let (composer, handler) = (screen.composer, screen.submit_handler);
        screen.encode(composer, handler, EventPayload::TextSubmitted("hello"));
        let batch = screen
            .host
            .dispatch_event(&screen.event)
            .expect("the send failed")
            .0
            .to_vec();
        // A removal takes the subtree with it, so what has to be gone is the greeting or
        // something it hangs from.
        let mut parents = HashMap::new();
        for mutation in decode_batch(&screen.first).expect("the first frame did not decode") {
            if let Mutation::Insert {
                parent_id, node_id, ..
            } = mutation
            {
                parents.insert(node_id, parent_id);
            }
        }
        let mut chain = vec![node];
        while let Some(parent) = parents.get(chain.last().expect("the chain is never empty")) {
            chain.push(*parent);
        }
        let removed = decode_batch(&batch)
            .expect("the send did not decode")
            .iter()
            .any(|mutation| {
                matches!(mutation, Mutation::Remove { node_id } if chain.contains(node_id))
            });
        assert!(
            removed,
            "the greeting stayed on screen once the conversation had started"
        );
    }

    /// The hundredth keystroke has to cost what the third one cost. A per-event buffer
    /// that is grown rather than reused shows up here as a count that climbs.
    #[test]
    fn nfr9_allocations_do_not_grow_across_repeated_interactions() {
        let mut screen = Screen::new();
        screen.open_window();
        for _ in 0..2 {
            screen.type_into_composer("warm");
        }

        let allocations: Vec<_> = (0..REPEATED_INTERACTIONS)
            .map(|_| screen.type_into_composer("a message being typed"))
            .collect();
        let expected = allocations[0];

        assert!(
            allocations.iter().all(|&count| count == expected),
            "per-interaction allocations grew or varied: {allocations:?}"
        );
    }
}
