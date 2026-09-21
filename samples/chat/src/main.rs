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

fn app() -> Element {
    let window = use_window_size();
    let measure = thread_width(&window);
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

            TopAppBar {
                fill_max_width: true,
                Text { text: "Chat", type_role: TypeRole::Title, weight: 1.0 }
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
                padding_role: SpaceRole::Lg,
                space_role: SpaceRole::Md,

                LazyColumn {
                    fill_max_width: true,
                    weight: 1.0,
                    item_count: count,
                    key_of: move |index: usize| keys[index].clone(),
                    item: move |index: usize| {
                        let message = messages.read()[index].clone();
                        // A change of speaker gets more air than a continuation, which is
                        // what makes a conversation read as turns rather than as an evenly
                        // spaced column of boxes.
                        let starts_a_run = index == 0
                            || messages.read()[index - 1].from_user != message.from_user;
                        // A message still arriving shows a caret, so an empty reply does
                        // not look like a dead one.
                        let text = if message.streaming {
                            format!("{}\u{2589}", message.text)
                        } else {
                            message.text.clone()
                        };
                        rsx! {
                            // The list has no spacing of its own, so the gap between one
                            // message and the next is padding on the row that holds it.
                            dioxus_compose::Box {
                                fill_max_width: true,
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
                                // Only what you said is a bubble. The reply is set on the
                                // page itself, left aligned and the full width of the
                                // thread, because it is the thing being read: a long
                                // answer boxed in a tinted rectangle is harder to read
                                // than the same answer as text, and the side it sits on
                                // already says who is speaking. This is what every current
                                // assistant does and it is not a stylistic preference, it
                                // is the difference between a page and a chat log.
                                if message.from_user {
                                    Column {
                                        background: Paint::Role(ColorRole::SurfaceVariant),
                                        shape_role: ShapeRole::Large,
                                        padding_role: SpaceRole::Md,
                                        Text {
                                            text,
                                            type_role: TypeRole::Body,
                                            color: Paint::Role(ColorRole::OnSurfaceVariant),
                                        }
                                    }
                                } else {
                                    Column {
                                        fill_max_width: true,
                                        padding_role: SpaceRole::Sm,
                                        Text {
                                            text,
                                            type_role: TypeRole::Body,
                                            color: Paint::Role(ColorRole::OnSurface),
                                        }
                                    }
                                }
                            }
                        }
                    },
                }

                // The composer, grouped so it reads as one control at the foot of the
                // conversation rather than as a field and a button that happen to be
                // side by side.
                //
                // A capsule, because every current chat application draws it as one and
                // because the thing it sits over is a scrolling thread: a rectangle at
                // the foot of a list reads as the end of the list, and a capsule reads as
                // something floating in front of it. The role is `Full` rather than a
                // radius, so it is still the design system that decides how round that is.
                //
                // It is drawn with an edge rather than with a fill, because the thread it
                // sits at the foot of is the reading surface. A filled panel on a reading
                // surface would either match the page, which is nothing, or match the
                // incoming bubble, which would make the place you type look like something
                // the assistant said.
                Surface {
                    fill_max_width: true,
                    shape_role: ShapeRole::Full,
                    border_width: 1.0,
                    border_color: Paint::Role(ColorRole::Outline),
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
                        Button {
                            text: "Send",
                            variant: ButtonVariant::Filled,
                            on_click: move |_| send(draft()),
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
    use std::collections::HashSet;
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
        scrollback: u32,
        range_handler: u64,
        composer: u32,
        submit_handler: u64,
        change_handler: u64,
        event: Vec<u8>,
    }

    impl Screen {
        fn new() -> Self {
            let mut host = Host::new(app);
            let first = decode_batch(host.rebuild().expect("the first frame failed to encode"))
                .expect("the first frame did not decode");
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
            drop(first);
            Self {
                host,
                scrollback,
                range_handler,
                composer,
                submit_handler,
                change_handler,
                event: Vec::new(),
            }
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
            self.host
                .dispatch_event(&self.event)
                .expect("the send failed");
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
