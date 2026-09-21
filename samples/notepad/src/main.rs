//! A working text editor.
//!
//! Written the way an application using this library would be written: `rsx!` and hooks,
//! no Kotlin and no protocol types. Reading and writing files happens on a worker thread
//! and comes back as a signal update, so the UI thread never waits on a disk.

use std::path::PathBuf;

use dioxus_compose::prelude::*;

mod document;

use document::{Counts, Outcome, counts, display_path};

/// The file the editor starts on, so the sample has something to open and save without a
/// file picker.
fn starting_path() -> PathBuf {
    std::env::temp_dir().join("dioxus-compose-notepad.txt")
}

/// The editor field.
///
/// A plain function rather than a `#[component]`, because `#[component]` expands to a
/// `#[derive(Props)]` and the derive macro is not exported to applications.
///
/// It is the raw `textfield` element rather than the `TextField` component, and the
/// contents arrive through a quoted attribute name, because neither the component nor the
/// declared attribute list offers a way to put text into a field. Opening a file has to.
///
/// `stamp` changes only when the text is replaced from outside the field, and the change of
/// key rebuilds the field so it picks the new contents up. Ordinary typing leaves `stamp`
/// alone, so the caret and the IME composition are never disturbed.
fn editor(stamp: u64, contents: String, on_edit: EventHandler<String>) -> Element {
    rsx! {
        for generation in [stamp] {
            textfield {
                key: "editor-{generation}",
                "text": "{contents}",
                multiline: true,
                fill_max_width: true,
                fill_max_height: true,
                placeholder: "Type here, or open a file",
                onvaluechange: move |event: Event<String>| on_edit.call((*event.data()).clone()),
            }
        }
    }
}

/// A single-line field whose contents are set once, for the path.
///
/// It takes the weight the toolbar gives it, so the path grows with the window instead of
/// being sized by whatever happens to be typed in it.
fn path_field(contents: String, on_edit: EventHandler<String>) -> Element {
    rsx! {
        textfield {
            "text": "{contents}",
            weight: 1.0,
            // A path is a machine string, so it is set in the monospace rung: the
            // separators line up and it stops competing with the document's own text.
            type_role: i64::from(u16::from(TypeRole::Mono)),
            placeholder: "Path to a file",
            onvaluechange: move |event: Event<String>| on_edit.call((*event.data()).clone()),
        }
    }
}

fn app() -> Element {
    let window = use_window_size();
    // A document is read across its lines, so past a certain width a page that keeps
    // growing is a page nobody can read. On a desktop window the page stops at the width
    // an expanded window starts at and takes a margin either side, which is what a page
    // is. Narrower than that the page is the window, because there is nothing to spare.
    let page_width = if window.is_expanded() {
        Some(WindowSizeClass::EXPANDED_MIN_WIDTH_DP)
    } else {
        None
    };
    let crowded = window.is_compact();
    let mut text = use_signal(String::new);
    let mut stamp = use_signal(|| 0_u64);
    let mut path = use_signal(|| display_path(&starting_path()));
    let mut status = use_signal(|| "Ready".to_owned());
    let mut busy = use_signal(|| false);

    // The worker's result comes back here, on the UI thread, and the signal writes happen
    // where every other signal write in the app happens.
    let mut settle = move |outcome: Outcome| {
        busy.set(false);
        match outcome {
            Outcome::Opened {
                path: opened,
                text: contents,
            } => {
                status.set(format!(
                    "Opened {} ({} characters)",
                    display_path(&opened),
                    contents.chars().count()
                ));
                text.set(contents);
                path.set(display_path(&opened));
                // The field's contents came from outside it, so the field is rebuilt.
                stamp += 1;
            }
            Outcome::Saved { path: saved, bytes } => {
                status.set(format!("Saved {} ({bytes} bytes)", display_path(&saved)));
            }
            Outcome::Failed(message) => status.set(message),
        }
    };

    // One piece of file work: a real thread does the blocking call, a one-shot channel
    // carries the answer back, and the awaiting task applies it on the UI thread.
    let mut run_on_worker = move |work: Box<dyn FnOnce() -> Outcome + Send + 'static>| {
        busy.set(true);
        let (sender, receiver) = futures_channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = sender.send(work());
        });
        dioxus_core::spawn(async move {
            match receiver.await {
                Ok(outcome) => settle(outcome),
                Err(_) => settle(Outcome::Failed(
                    "The file worker stopped before it answered".to_owned(),
                )),
            }
        });
    };

    let Counts {
        words,
        characters,
        lines,
    } = counts(&text());
    let working = busy();

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,

            // The document's actions belong in the bar, not in a line of buttons above the
            // text. The title takes the weight, which pushes them to the far end.
            TopAppBar {
                fill_max_width: true,
                Text { text: "Notepad", type_role: TypeRole::Title, weight: 1.0 }
                Button {
                    text: "New",
                    variant: ButtonVariant::Text,
                    enabled: !working,
                    on_click: move |_| {
                        text.set(String::new());
                        stamp += 1;
                        status.set("New document".to_owned());
                    },
                }
                Button {
                    text: "Open",
                    variant: ButtonVariant::Tonal,
                    enabled: !working,
                    on_click: move |_| {
                        let target = PathBuf::from(path());
                        run_on_worker(Box::new(move || document::open(target)));
                    },
                }
                Button {
                    text: "Save",
                    variant: ButtonVariant::Filled,
                    enabled: !working,
                    on_click: move |_| {
                        let target = PathBuf::from(path());
                        let contents = text();
                        run_on_worker(Box::new(move || document::save(target, contents)));
                    },
                }
            }

            Column {
                fill_max_width: true,
                fill_max_height: true,
                padding_role: SpaceRole::Lg,
                space_role: SpaceRole::Md,

                // The location bar: one grouped strip that says which file the actions
                // above work on.
                Surface {
                    fill_max_width: true,
                    Row {
                        fill_max_width: true,
                        space_role: SpaceRole::Sm,
                        alignment: Alignment::CenterStart,
                        // The word is dropped on a phone: the field says what it is in
                        // its own placeholder, and a path needs every pixel of the line.
                        if !crowded {
                            Text {
                                text: "File",
                                type_role: TypeRole::Label,
                                color: Paint::Role(ColorRole::OnSurfaceVariant),
                            }
                        }
                        {path_field(path(), EventHandler::new(move |value| path.set(value)))}
                    }
                }

                // The page. A document is an object you write on, so it is a surface of
                // its own with the rest of the window as its margin, and it takes what the
                // toolbar and the status line leave.
                dioxus_compose::Box {
                    fill_max_width: true,
                    weight: 1.0,
                    alignment: Alignment::TopCenter,
                Surface {
                    fill_max_width: page_width.is_none(),
                    width: page_width,
                    fill_max_height: true,
                    // The editor scrolls on its own, so a document longer than the window
                    // stays reachable without the Host knowing where the scroll is.
                    ScrollColumn {
                        fill_max_width: true,
                        fill_max_height: true,
                        {editor(stamp(), text(), EventHandler::new(move |value| text.set(value)))}
                    }
                }
                }

                // The status line: what the last file operation did on the left, the
                // document's measurements on the right.
                Row {
                    fill_max_width: true,
                    space_role: SpaceRole::Md,
                    alignment: Alignment::CenterStart,
                    Text {
                        text: status(),
                        weight: 1.0,
                        type_role: TypeRole::Caption,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                        max_lines: 1,
                        overflow: TextOverflow::Ellipsis,
                    }
                    Text {
                        text: "{words} words",
                        type_role: TypeRole::Caption,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                    Text {
                        text: "{characters} characters",
                        type_role: TypeRole::Caption,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                    Text {
                        text: "{lines} lines",
                        type_role: TypeRole::Caption,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
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
    use std::collections::HashMap;

    /// Hangul, a combining mark and an emoji: three ways a byte count and a character count
    /// come apart, all of which have to survive the trip to disk and back.
    const KOREAN: &str = "안녕하세요\n오늘도 좋은 하루 보내세요 \u{1f44b}";

    /// The real editor, driven the way the Renderer drives it.
    struct Editor {
        host: Host,
        /// The path field and the document field, in the order the screen declares them.
        fields: Vec<u32>,
        /// Node to the handler its `onvaluechange` was given.
        changes: HashMap<u32, u64>,
        /// Button label to its node and click handler.
        buttons: HashMap<String, (u32, u64)>,
        texts: HashMap<u32, String>,
        event: Vec<u8>,
    }

    impl Editor {
        fn new() -> Self {
            let mut editor = Self {
                host: Host::new(app),
                fields: Vec::new(),
                changes: HashMap::new(),
                buttons: HashMap::new(),
                texts: HashMap::new(),
                event: Vec::new(),
            };
            let batch = editor
                .host
                .rebuild()
                .expect("the first frame failed to encode");
            let decoded = decode_batch(batch).expect("the first frame did not decode");
            let mut fields = Vec::new();
            let mut clicks = HashMap::new();
            let mut changes = HashMap::new();
            let mut texts = HashMap::new();
            for mutation in &decoded {
                match mutation {
                    Mutation::Create {
                        node_id,
                        widget: WidgetKind::TextField,
                    } => fields.push(*node_id),
                    Mutation::SetProp {
                        node_id,
                        property,
                        value,
                    } => match (property, value) {
                        (PropertyKind::Text, PropertyValue::String(text)) => {
                            texts.insert(*node_id, (*text).to_owned());
                        }
                        (PropertyKind::OnClick, PropertyValue::Integer(id)) => {
                            clicks.insert(*node_id, *id as u64);
                        }
                        (PropertyKind::OnValueChange, PropertyValue::Integer(id)) => {
                            changes.insert(*node_id, *id as u64);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            drop(decoded);
            editor.buttons = clicks
                .iter()
                .filter_map(|(node_id, handler)| {
                    texts
                        .get(node_id)
                        .map(|label| (label.clone(), (*node_id, *handler)))
                })
                .collect();
            editor.fields = fields;
            editor.changes = changes;
            editor.texts = texts;
            assert_eq!(
                editor.fields.len(),
                2,
                "the screen should have a path field and a document field"
            );
            editor
        }

        fn absorb(&mut self, batch: &[u8]) {
            for mutation in decode_batch(batch).expect("a frame did not decode") {
                match mutation {
                    Mutation::Create {
                        node_id,
                        widget: WidgetKind::TextField,
                    } => {
                        // The document field is rebuilt under a new key whenever its
                        // contents come from outside it, so the node id changes.
                        if !self.fields.contains(&node_id) {
                            self.fields.push(node_id);
                        }
                    }
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::Text,
                        value: PropertyValue::String(text),
                    } => {
                        self.texts.insert(node_id, text.to_owned());
                    }
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::OnValueChange,
                        value: PropertyValue::Integer(id),
                    } => {
                        self.changes.insert(node_id, id as u64);
                    }
                    _ => {}
                }
            }
        }

        fn dispatch(&mut self, node_id: u32, handler_id: u64, payload: EventPayload<'_>) {
            encode_event(
                &HostEvent {
                    node_id,
                    handler_id,
                    payload,
                },
                &mut self.event,
            )
            .expect("the event did not encode");
            let (batch, _) = self
                .host
                .dispatch_event(&self.event)
                .expect("the event failed");
            // The borrow of the Host's buffer has to end before the next call, so the
            // frame is read into the table here.
            let owned = batch.to_vec();
            self.absorb(&owned);
        }

        /// The document field, which is the last text field the screen declared.
        fn document_field(&self) -> u32 {
            *self
                .fields
                .last()
                .expect("the screen has no document field")
        }

        /// Typing into the document.
        fn type_document(&mut self, text: &str) {
            let node_id = self.document_field();
            let handler = *self
                .changes
                .get(&node_id)
                .expect("the document field declared no change handler");
            self.dispatch(node_id, handler, EventPayload::TextChanged(text));
        }

        /// Setting the path the Open and Save buttons work on.
        fn set_path(&mut self, path: &str) {
            let node_id = self.fields[0];
            let handler = *self
                .changes
                .get(&node_id)
                .expect("the path field declared no change handler");
            self.dispatch(node_id, handler, EventPayload::TextChanged(path));
        }

        fn click(&mut self, label: &str) {
            let (node_id, handler) = *self
                .buttons
                .get(label)
                .unwrap_or_else(|| panic!("the screen has no {label} button"));
            self.dispatch(node_id, handler, EventPayload::Clicked);
        }

        /// File work happens on a worker and lands on the UI thread in a later frame, so
        /// the test polls frames the way the Renderer would.
        fn settle_until(&mut self, contains: &str) {
            for _ in 0..400 {
                std::thread::sleep(std::time::Duration::from_millis(5));
                let batch = self
                    .host
                    .render_frame(0)
                    .expect("a frame failed to encode")
                    .to_vec();
                self.absorb(&batch);
                if self.status().contains(contains) {
                    return;
                }
            }
            panic!(
                "the screen never reported {contains}; the status line says {}",
                self.status()
            );
        }

        /// The status line is the only text on the screen that reports what the file work
        /// did, so it is recognised by what it says.
        fn status(&self) -> String {
            self.texts
                .values()
                .find(|text| {
                    text.starts_with("Opened ")
                        || text.starts_with("Saved ")
                        || text.starts_with("Could not ")
                        || text.as_str() == "Ready"
                        || text.as_str() == "New document"
                })
                .cloned()
                .unwrap_or_default()
        }

        fn document_text(&self) -> String {
            self.texts
                .get(&self.document_field())
                .cloned()
                .unwrap_or_default()
        }

        fn counter(&self, suffix: &str) -> String {
            self.texts
                .values()
                .find(|text| text.ends_with(suffix))
                .cloned()
                .unwrap_or_default()
        }
    }

    /// Every property this screen sets has to be one the wire can name. A property the
    /// schema does not have fails the whole batch rather than just itself, so a screen that
    /// builds in Rust can still be blank on screen.
    #[test]
    fn the_first_frame_encodes_without_a_protocol_error() {
        assert!(Host::new(app).rebuild().is_ok());
    }

    /// Editing, saving, clearing and reopening, all through the screen. Korean text has to
    /// come back exactly as it went in: the file is bytes, the field is characters, and
    /// every step between them is somewhere the two can be confused.
    #[test]
    fn fr6_a_document_round_trips_through_editing_and_saving_with_korean_intact() {
        let path = std::env::temp_dir().join("sample-notepad-round-trip.txt");
        let _ = std::fs::remove_file(&path);

        let mut editor = Editor::new();
        editor.set_path(&display_path(&path));
        editor.type_document(KOREAN);
        assert_eq!(editor.document_text(), KOREAN);

        editor.click("Save");
        editor.settle_until("Saved ");
        assert_eq!(
            std::fs::read_to_string(&path).expect("the file was not written"),
            KOREAN
        );

        // Clear the editor, so reopening has to put the text back rather than leave it.
        editor.click("New");
        assert_eq!(editor.document_text(), "");

        editor.click("Open");
        editor.settle_until("Opened ");
        assert_eq!(
            editor.document_text(),
            KOREAN,
            "the reopened document does not match what was saved"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// The counters under the editor count characters, not bytes. One Hangul syllable is
    /// one character even though it takes three bytes of UTF-8, so a screen that counted
    /// bytes would report three times the length of a Korean document.
    #[test]
    fn fr6_the_counters_report_characters_rather_than_bytes() {
        let mut editor = Editor::new();
        editor.type_document(KOREAN);

        let expected = KOREAN.chars().count();
        assert!(
            expected < KOREAN.len(),
            "the fixture is not multi-byte text"
        );
        assert_eq!(
            editor.counter(" characters"),
            format!("{expected} characters")
        );
        assert_eq!(editor.counter(" lines"), "2 lines");
    }

    /// The page widths and the strip's labels after the Renderer reports a window of the
    /// given width.
    fn page_at(width_dp: f32) -> (Vec<f32>, Vec<String>) {
        dioxus_compose::window::reset_window_size();
        let mut host = Host::new(app);
        let first = host.rebuild().expect("the first frame failed to encode");
        let mut widths = widths_of(first);
        let mut labels = texts_of(first);
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
        let after_widths = widths_of(batch);
        let after_labels = texts_of(batch);
        if !after_labels.is_empty() {
            widths = after_widths;
            labels = after_labels;
        }
        dioxus_compose::window::reset_window_size();
        (widths, labels)
    }

    fn widths_of(batch: &[u8]) -> Vec<f32> {
        decode_batch(batch)
            .expect("the batch did not decode")
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::SetModifier {
                    modifier: dioxus_compose::Modifier::Width(width),
                    ..
                } => Some(*width),
                _ => None,
            })
            .collect()
    }

    fn texts_of(batch: &[u8]) -> Vec<String> {
        decode_batch(batch)
            .expect("the batch did not decode")
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::SetProp {
                    property: PropertyKind::Text,
                    value: PropertyValue::String(text),
                    ..
                } => Some((*text).to_owned()),
                _ => None,
            })
            .collect()
    }

    /// A desktop window gives the document a margin and stops the page growing with the
    /// window. A phone keeps the page full width and drops the word in front of the path,
    /// which the field's own placeholder already says.
    #[test]
    fn fr20_the_page_takes_a_margin_on_a_desktop_window() {
        let (narrow_widths, narrow_labels) = page_at(420.0);
        assert!(narrow_widths.is_empty(), "{narrow_widths:?}");
        assert!(!narrow_labels.iter().any(|text| text == "File"));

        let (wide_widths, wide_labels) = page_at(1200.0);
        assert!(
            wide_widths.contains(&dioxus_compose::WindowSizeClass::EXPANDED_MIN_WIDTH_DP),
            "the page did not take a measure: {wide_widths:?}"
        );
        assert!(wide_labels.iter().any(|text| text == "File"));
    }

    /// A path that is not there is a message on the status line, not a crash and not a
    /// silently emptied document.
    #[test]
    fn fr6_opening_a_missing_file_reports_why() {
        let path = std::env::temp_dir().join("sample-notepad-absent-file.txt");
        let _ = std::fs::remove_file(&path);

        let mut editor = Editor::new();
        editor.type_document("keep me");
        editor.set_path(&display_path(&path));
        editor.click("Open");
        editor.settle_until("Could not open");

        assert_eq!(
            editor.document_text(),
            "keep me",
            "a failed open threw the document away"
        );
    }
}
