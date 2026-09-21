//! A working desktop calculator, with the tape every desk calculator that had a printer
//! used to have.
//!
//! Written the way an application using this library would be written: `rsx!` and hooks,
//! no Kotlin and no protocol types.
//!
//! There is no keyboard entry, and the reason is worth saying out loud because it used to
//! be here. Only Enter can cross the boundary as a key, so digits had to be read out of a
//! focused text field by comparing what it held before against what it holds now. That
//! works, and it needs a field on the screen to hold the focus: a line of development
//! scaffolding sitting under the keypad, doing nothing a calculator does. A keypad is
//! pressed. When the protocol can name the keys, typing comes back as typing.

use dioxus_compose::prelude::*;

mod engine;

use engine::Calculator;

/// The narrow key grid, top to bottom and left to right.
///
/// On a phone there is one column of keys and nowhere else for the four function keys, so
/// they share the top row with an operator.
const ROWS: [[&str; 4]; 5] = [
    ["C", "\u{232b}", "%", "\u{00f7}"],
    ["7", "8", "9", "\u{00d7}"],
    ["4", "5", "6", "\u{2212}"],
    ["1", "2", "3", "+"],
    ["\u{00b1}", "0", ".", "="],
];

/// The wide key grid: digits and operators only.
///
/// Once there is width to spend, the functions leave the grid and the pad becomes what a
/// desk calculator is, four rows of digits with the operators down the side.
const WIDE_ROWS: [[&str; 4]; 4] = [
    ["7", "8", "9", "\u{00f7}"],
    ["4", "5", "6", "\u{00d7}"],
    ["1", "2", "3", "\u{2212}"],
    ["\u{00b1}", "0", ".", "+"],
];

/// The column beside the wide grid: what a calculator does to an entry rather than to a
/// number, with equals at the foot where the hand ends up.
const FUNCTION_COLUMN: [&str; 4] = ["C", "\u{232b}", "%", "="];

/// How much of the wide keypad's width the digit grid takes. The functions are one column
/// beside the grid's four.
const GRID_SHARE: f32 = 4.0;

/// A keypad is a field of keys, so every key has a surface and the three kinds of key are
/// told apart by which surface they get.
///
/// The operators and equals are the accent, the digits are the quiet filled key that most
/// of the pad is made of, and the four function keys are outlined, which is a key with an
/// edge rather than a key with a fill. A plain text button is not a key at all: it reads as
/// a label lying on the case.
fn variant_for(label: &str) -> ButtonVariant {
    match label {
        "=" | "\u{00f7}" | "\u{00d7}" | "\u{2212}" | "+" => ButtonVariant::Filled,
        "C" | "\u{232b}" | "%" | "\u{00b1}" => ButtonVariant::Outlined,
        _ => ButtonVariant::Tonal,
    }
}

/// Whether a key carries content rather than a command.
///
/// The digits and the decimal point are the number being entered, and a number is content.
/// A tonal button is a secondary *action* in every design system, so its label is the
/// accent colour: left alone, a keypad prints its digits in the link colour. The digits
/// therefore name their own ink and the command keys keep the tint, which leaves one
/// meaning on the pad reading as accent, something that acts on the number.
fn is_content_key(label: &str) -> bool {
    matches!(
        label,
        "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "."
    )
}

/// The label ink for a key, or `None` to let the variant decide.
fn color_for(label: &str) -> Option<Paint> {
    is_content_key(label).then_some(Paint::Role(ColorRole::OnSurface))
}

/// The keypad, in the shape the window has room for.
fn keypad(wide: bool, press: EventHandler<&'static str>) -> Element {
    if wide {
        return rsx! {
            Row {
                fill_max_width: true,
                fill_max_height: true,
                space_role: SpaceRole::Sm,
                Column {
                    weight: GRID_SHARE,
                    fill_max_height: true,
                    space_role: SpaceRole::Sm,
                    for (index , row) in WIDE_ROWS.iter().enumerate() {
                        Row {
                            key: "wide-row-{index}",
                            fill_max_width: true,
                            weight: 1.0,
                            space_role: SpaceRole::Sm,
                            for label in row.iter().copied() {
                                Button {
                                    key: "{label}",
                                    text: label,
                                    weight: 1.0,
                                    fill_max_height: true,
                                    variant: variant_for(label),
                                    color: color_for(label),
                                    on_click: move |_| press.call(label),
                                }
                            }
                        }
                    }
                }
                Column {
                    weight: 1.0,
                    fill_max_height: true,
                    space_role: SpaceRole::Sm,
                    for label in FUNCTION_COLUMN.iter().copied() {
                        Button {
                            key: "{label}",
                            text: label,
                            weight: 1.0,
                            fill_max_width: true,
                            variant: variant_for(label),
                            color: color_for(label),
                            on_click: move |_| press.call(label),
                        }
                    }
                }
            }
        };
    }
    // The narrow pad: every row an equal share of the height, every key an equal share of
    // its row. Weight is what makes this a grid rather than five lines of differently
    // sized buttons.
    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            space_role: SpaceRole::Sm,
            for (index , row) in ROWS.iter().enumerate() {
                Row {
                    key: "row-{index}",
                    fill_max_width: true,
                    weight: 1.0,
                    space_role: SpaceRole::Sm,
                    for label in row.iter().copied() {
                        Button {
                            key: "{label}",
                            text: label,
                            weight: 1.0,
                            fill_max_height: true,
                            variant: variant_for(label),
                            color: color_for(label),
                            on_click: move |_| press.call(label),
                        }
                    }
                }
            }
        }
    }
}

/// How large the entered number is set, in sp.
///
/// This is the one place in the samples where a size is named rather than a rung, and the
/// reason is that the ladder is a ladder for documents. Its top rung is a headline over a
/// page of body text: 34sp in Cupertino, 40 in Fluent, 57 in Material. A calculator is not
/// a document. The number is the whole instrument and everything else on the screen is
/// support for entering it, so at any rung of a reading ladder the panel ends up a large
/// box with a small number pinned inside it.
///
/// The alternative was a tenth rung above `Display`. It was rejected because every design
/// system would then have to publish a size for a rung that only an instrument readout
/// would ever ask for, and a ladder gains a rung nothing reads. Keeping `TypeRole::Display`
/// and overriding the size alone means the family, the weight and the letter spacing are
/// still the design system's, which is the part of the rung that is worth having.
const READOUT_SP: f32 = 64.0;

/// The readout panel. A calculator's display is set into the case rather than printed on
/// it, so it is a surface of its own.
fn readout(status: String, display: String) -> Element {
    rsx! {
        Surface {
            fill_max_width: true,
            Column {
                fill_max_width: true,
                space_role: SpaceRole::Xs,
                // The pending operation is a line only while there is one. An empty line
                // still has a height, and the panel is meant to be as tall as the type it
                // holds and no taller.
                if !status.is_empty() {
                    Text {
                        text: status,
                        fill_max_width: true,
                        text_align: TextAlign::End,
                        type_role: TypeRole::Caption,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                        max_lines: 1,
                        overflow: TextOverflow::Ellipsis,
                    }
                }
                Text {
                    text: display,
                    fill_max_width: true,
                    text_align: TextAlign::End,
                    type_role: TypeRole::Display,
                    font_size: READOUT_SP,
                    line_height: READOUT_SP,
                    color: Paint::Role(ColorRole::OnSurface),
                    max_lines: 1,
                    overflow: TextOverflow::Ellipsis,
                }
            }
        }
    }
}

/// One finished calculation, kept so it can be read back and used again.
#[derive(Clone, PartialEq)]
struct TapeEntry {
    /// Never reused, so it is the list key and a row that scrolls past another row is not
    /// mistaken for the same row changing.
    id: u64,
    expression: String,
    result: String,
    value: f64,
}

/// What the app bar calls the tape, and what the sheet is opened by.
const TAPE_LABEL: &str = "Tape";

/// The tape: what has been worked out, newest at the top, each line a key back into the
/// calculation it came from.
///
/// Every desk calculator with a printer has one of these, and it is the answer to the
/// thing a calculator is worst at: you work something out, you work the next thing out,
/// and the first answer is gone.
fn tape(entries: Vec<TapeEntry>, recall: EventHandler<f64>, clear: EventHandler<()>) -> Element {
    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            space_role: SpaceRole::Sm,
            Row {
                fill_max_width: true,
                space_role: SpaceRole::Sm,
                alignment: Alignment::CenterStart,
                Text { text: TAPE_LABEL, type_role: TypeRole::Subtitle, weight: 1.0 }
                // Throwing the tape away is the one thing here that destroys something, so
                // it is the error ink and it offers the tape back afterwards.
                Button {
                    text: "Clear",
                    variant: ButtonVariant::Text,
                    color: Paint::Role(ColorRole::Error),
                    enabled: !entries.is_empty(),
                    on_click: move |_| clear.call(()),
                }
            }
            Separator {}
            if entries.is_empty() {
                dioxus_compose::Box {
                    fill_max_width: true,
                    weight: 1.0,
                    alignment: Alignment::Center,
                    Text {
                        text: "Nothing worked out yet. Press equals and it lands here.",
                        type_role: TypeRole::Body,
                        text_align: TextAlign::Center,
                        color: Paint::Role(ColorRole::OnSurfaceVariant),
                    }
                }
            } else {
                // Newest first: the thing you are most likely to want back is the thing
                // you worked out last, and it should not be at the bottom of a scroll.
                ScrollColumn {
                    fill_max_width: true,
                    weight: 1.0,
                    for entry in entries.iter().rev() {
                        Button {
                            key: "{entry.id}",
                            text: "{entry.expression} = {entry.result}",
                            fill_max_width: true,
                            variant: ButtonVariant::Text,
                            color: Paint::Role(ColorRole::OnSurface),
                            on_click: {
                                let value = entry.value;
                                move |_| recall.call(value)
                            },
                        }
                    }
                }
            }
        }
    }
}

/// How the instrument and the tape share an expanded window.
///
/// The keypad is what the hand is on, so it gets the larger share, and the tape is a
/// column of short lines that gains nothing from being wider.
const INSTRUMENT_SHARE: f32 = 3.0;
const TAPE_SHARE: f32 = 2.0;

fn app() -> Element {
    let window = use_window_size();
    let mut calculator = use_signal(Calculator::new);
    let mut entries = use_signal(Vec::<TapeEntry>::new);
    let mut next_entry = use_signal(|| 1_u64);
    let mut tape_open = use_signal(|| false);

    let display = calculator.read().display();
    let status = calculator.read().status();
    // A phone holds one column of keys. Anything wider has room for the functions to leave
    // the grid and stand in a column of their own.
    let wide = !window.is_compact();
    // A desktop window has room for the tape to stand beside the keypad. Narrower than
    // that it is a sheet, which is the same tape arriving from an edge instead.
    let tape_beside = window.is_expanded();
    // The keypad stops widening somewhere: keys that grew to fill a desktop window would
    // be the size of a hand.
    let pad_width =
        (!tape_beside && window.is_expanded()).then_some(WindowSizeClass::EXPANDED_MIN_WIDTH_DP);

    // One key press, whichever key it was. The tape is written here rather than in the
    // engine because a tape is a thing the application keeps, not a thing arithmetic has.
    let press = move |label: &'static str| {
        let finished = {
            let mut state = calculator.write();
            state.press(label);
            state.take_completed()
        };
        let Some(finished) = finished else { return };
        if finished.failed {
            // A result a calculator cannot show is worth one sentence, and the sentence
            // is not a widget this code places: it is handed over once and the Renderer
            // decides where it goes and how long it stays.
            Message::new(format!("{} has no answer", finished.expression)).show();
            return;
        }
        let id = next_entry();
        next_entry.set(id + 1);
        entries.write().push(TapeEntry {
            id,
            expression: finished.expression,
            result: finished.result,
            value: finished.value,
        });
    };

    let recall = EventHandler::new(move |value: f64| {
        calculator.write().recall(value);
        tape_open.set(false);
    });
    let clear_tape = EventHandler::new(move |()| {
        let thrown_away = std::mem::take(&mut *entries.write());
        let lines = if thrown_away.len() == 1 {
            "line"
        } else {
            "lines"
        };
        Message::new(format!("Cleared {} {lines}", thrown_away.len()))
            .with_action("Undo", move |()| entries.set(thrown_away.clone()))
            .with_duration(MessageDuration::Long)
            .show();
    });

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,

            TopAppBar {
                fill_max_width: true,
                Text { text: "Calculator", type_role: TypeRole::Title, weight: 1.0 }
                // Only where the tape is not already on screen. A button that opens what
                // you are looking at is a button that does nothing.
                if !tape_beside {
                    Button {
                        text: TAPE_LABEL,
                        variant: ButtonVariant::Text,
                        on_click: move |_| tape_open.set(true),
                    }
                }
            }

            Row {
                fill_max_width: true,
                fill_max_height: true,
                Column {
                    weight: INSTRUMENT_SHARE,
                    fill_max_height: true,
                    // The bar insets its own contents by the medium step, so the
                    // instrument under it uses the same one and the two line up.
                    padding_role: SpaceRole::Md,
                    space_role: SpaceRole::Md,

                    {readout(status, display)}
                    // The pad takes the height the readout leaves, and on a desktop
                    // window it stops widening and centres instead.
                    dioxus_compose::Box {
                        fill_max_width: true,
                        weight: 1.0,
                        alignment: Alignment::TopCenter,
                        Column {
                            fill_max_width: pad_width.is_none(),
                            width: pad_width,
                            fill_max_height: true,
                            {keypad(wide, EventHandler::new(press))}
                        }
                    }
                }
                if tape_beside {
                    Divider { vertical: true }
                    Column {
                        weight: TAPE_SHARE,
                        fill_max_height: true,
                        padding_role: SpaceRole::Md,
                        {tape(entries(), recall, clear_tape)}
                    }
                }
            }

            // The same tape, arriving from an edge, for the windows with no room beside
            // the keys. Which edge is the Renderer's decision: up from the bottom of a
            // phone, in from the side of a tablet.
            Sheet {
                open: tape_open() && !tape_beside,
                on_dismiss: move |_| tape_open.set(false),
                fill_max_width: true,
                {tape(entries(), recall, clear_tape)}
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
    use dioxus_compose::schema::{
        EventPayload, MessageDuration, PropertyKind, TypeRole, WidgetKind,
    };
    use std::collections::HashMap;

    /// Whether a label belongs to the keypad rather than to the rest of the screen.
    ///
    /// The screen holds buttons that are not keys, and a test about the shape of the
    /// keypad has to be able to say so: the tape's own controls appearing in a count of
    /// keys is how a keypad test starts failing for a reason that has nothing to do with
    /// the keypad.
    fn is_key(label: &str) -> bool {
        ROWS.iter()
            .flatten()
            .chain(WIDE_ROWS.iter().flatten())
            .chain(FUNCTION_COLUMN.iter())
            .any(|key| *key == label)
    }

    /// The key labels the screen holds, in the order the keypad declares them, after the
    /// Renderer reports a window of the given width.
    fn keys_at(width_dp: f32) -> Vec<String> {
        dioxus_compose::window::reset_window_size();
        let mut host = Host::new(app);
        let first = host.rebuild().expect("the first frame failed to encode");
        let mut labels = collect_button_labels(first);
        let event = HostEvent {
            node_id: 0,
            handler_id: 0,
            payload: EventPayload::WindowSizeChanged {
                width_dp,
                height_dp: 800.0,
                class: dioxus_compose::WindowSizeClass::from_width_dp(width_dp),
            },
        };
        let mut bytes = Vec::new();
        encode_event(&event, &mut bytes).expect("the resize did not encode");
        let (batch, _) = host.dispatch_event(&bytes).expect("the resize failed");
        let after = collect_button_labels(batch);
        if !after.is_empty() {
            labels = after;
        }
        labels.retain(|label| is_key(label));
        dioxus_compose::window::reset_window_size();
        labels
    }

    fn collect_button_labels(batch: &[u8]) -> Vec<String> {
        let mutations = decode_batch(batch).expect("the batch did not decode");
        let buttons: Vec<u32> = mutations
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::Create {
                    node_id,
                    widget: WidgetKind::Button,
                } => Some(*node_id),
                _ => None,
            })
            .collect();
        mutations
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::SetProp {
                    node_id,
                    property: PropertyKind::Text,
                    value: PropertyValue::String(text),
                } if buttons.contains(node_id) => Some((*text).to_owned()),
                _ => None,
            })
            .collect()
    }

    /// A phone keypad and a desk keypad are different keypads, not the same one scaled.
    /// Narrow, the four function keys share the top row with an operator. Wide, they are a
    /// column of their own and the grid is four rows of digits.
    #[test]
    fn fr20_the_keypad_changes_shape_with_the_window() {
        let narrow = keys_at(420.0);
        assert_eq!(narrow.first().map(String::as_str), Some("C"));
        assert_eq!(narrow.len(), 20);

        let wide = keys_at(900.0);
        assert_eq!(wide.first().map(String::as_str), Some("7"));
        assert_eq!(wide.len(), 20);
        // The functions come last because they are a column beside the grid rather than
        // its first row.
        assert_eq!(
            wide[16..],
            ["C", "\u{232b}", "%", "="].map(str::to_owned)[..]
        );
        assert_ne!(narrow, wide);
    }

    #[test]
    fn every_key_has_a_variant_and_an_action() {
        let mut calculator = Calculator::new();
        for row in ROWS {
            for label in row {
                let _ = variant_for(label);
                calculator.press(label);
            }
        }
        assert!(!calculator.display().is_empty());
    }

    /// The whole declared tree, checked against the encoder. A widget or an attribute the
    /// schema cannot carry fails the frame rather than the one node, so this is the test
    /// that would have caught it.
    #[test]
    fn the_declared_tree_encodes_without_a_protocol_error() {
        let mut dom = VirtualDom::new(app);
        let mut renderer = dioxus_compose::renderer::ComposeRenderer::new();
        renderer.begin_frame();
        dom.rebuild(&mut renderer);
        renderer
            .finish_frame()
            .expect("the calculator tree encodes");
    }

    /// The calculator's screen, under every design system, in both schemes, at all three
    /// window widths.
    ///
    /// A batch that encodes is not the same as a screen someone can read. Material 3
    /// shipped a readout filled with a colour that matched the page behind it, drawn full
    /// size, in the right colour and invisible, and every assertion in this file passed
    /// the whole time. The only thing that settles it is looking.
    ///
    /// So `DXC_FRAME_DIR` writes the frames out, exactly as the Renderer decodes them, and
    /// the Renderer's screenshot test turns each one into a picture. Unset, which is the
    /// normal run, this still builds all thirty-six: a width or a design system nobody can
    /// encode is the failure the tables exist to avoid.
    #[test]
    fn fr14_the_screen_is_recorded_under_every_design_system_and_width() {
        sample_frames::record("Calculator", app, |_| {});
    }

    #[test]
    fn display_is_formatted_not_raw() {
        assert_eq!(engine::format_number(1.0 / 3.0), "0.333333333333");
    }

    /// The real screen, driven the way the Renderer drives it: an encoded event naming the
    /// button's own handler, and the display read back out of the frame that came of it.
    /// Calling the engine directly would skip everything that can actually break between a
    /// finger and a number on screen.
    ///
    /// The tables are kept up to date as frames arrive rather than read once, because the
    /// screen grows: a tape line is a button that did not exist when the screen was built,
    /// and a harness that only looked at the first frame could never press one.
    struct Screen {
        host: Host,
        /// Node to the label printed on it.
        texts: HashMap<u32, String>,
        /// Node to the handler its `on_click` was given.
        clicks: HashMap<u32, u64>,
        /// Node to the node it was inserted under, so a removal takes the subtree with it
        /// the way the Renderer's node table does. Without it a tape line that has been
        /// thrown away is still there as far as this harness can tell.
        parents: HashMap<u32, u32>,
        display: u32,
        /// Every message the screen has said, in order, with its action label.
        messages: Vec<(String, String)>,
        event: Vec<u8>,
    }

    impl Screen {
        fn new() -> Self {
            let mut screen = Self {
                host: Host::new(app),
                texts: HashMap::new(),
                clicks: HashMap::new(),
                parents: HashMap::new(),
                display: 0,
                messages: Vec::new(),
                event: Vec::new(),
            };
            let first = screen
                .host
                .rebuild()
                .expect("the first frame failed to encode")
                .to_vec();
            let display_roles = screen.absorb(&first);
            assert_eq!(
                display_roles.len(),
                1,
                "the screen should have exactly one display sized text"
            );
            screen.display = display_roles[0];
            screen
        }

        /// Applies one batch and returns the nodes in it that are set at the display rung.
        fn absorb(&mut self, batch: &[u8]) -> Vec<u32> {
            let mut display_roles = Vec::new();
            for mutation in decode_batch(batch).expect("the frame did not decode") {
                match mutation {
                    Mutation::Insert {
                        parent_id, node_id, ..
                    }
                    | Mutation::Move {
                        parent_id, node_id, ..
                    } => {
                        self.parents.insert(node_id, parent_id);
                    }
                    Mutation::Remove { node_id } => self.forget(node_id),
                    Mutation::ShowMessage { text, action, .. } => {
                        self.messages.push((text.to_owned(), action.to_owned()));
                    }
                    Mutation::SetProp {
                        node_id,
                        property,
                        value,
                    } => match (property, value) {
                        (PropertyKind::Text, PropertyValue::String(text)) => {
                            self.texts.insert(node_id, text.to_owned());
                        }
                        (PropertyKind::OnClick, PropertyValue::Integer(id)) => {
                            self.clicks.insert(node_id, id as u64);
                        }
                        (PropertyKind::TypeRole, PropertyValue::Integer(role))
                            if role == i64::from(TypeRole::Display as u8) =>
                        {
                            display_roles.push(node_id);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            display_roles
        }

        /// Drops a node and everything under it.
        fn forget(&mut self, node_id: u32) {
            let children: Vec<u32> = self
                .parents
                .iter()
                .filter(|(_, parent)| **parent == node_id)
                .map(|(child, _)| *child)
                .collect();
            for child in children {
                self.forget(child);
            }
            self.texts.remove(&node_id);
            self.clicks.remove(&node_id);
            self.parents.remove(&node_id);
        }

        fn dispatch(&mut self, node_id: u32, handler_id: u64, payload: EventPayload<'_>) -> i64 {
            encode_event(
                &HostEvent {
                    node_id,
                    handler_id,
                    payload,
                },
                &mut self.event,
            )
            .expect("the event did not encode");
            let (batch, result) = self
                .host
                .dispatch_event(&self.event)
                .expect("the event failed");
            let batch = batch.to_vec();
            self.absorb(&batch);
            result
        }

        /// The node showing this exact label, if the screen is showing one.
        fn labelled(&self, label: &str) -> Option<u32> {
            self.texts
                .iter()
                .find(|(_, text)| *text == label)
                .map(|(node_id, _)| *node_id)
        }

        /// Press what a person would read as this label.
        fn press(&mut self, label: &str) {
            let node_id = self
                .labelled(label)
                .unwrap_or_else(|| panic!("the screen has nothing labelled {label}"));
            let handler = *self
                .clicks
                .get(&node_id)
                .unwrap_or_else(|| panic!("{label} is not something that can be pressed"));
            self.dispatch(node_id, handler, EventPayload::Clicked);
        }

        fn display(&self) -> &str {
            self.texts
                .get(&self.display)
                .map(String::as_str)
                .expect("the display carries no text")
        }
    }

    /// A sequence of key presses, through the encoded event path rather than the engine, and
    /// the answer read back off the display the way a person reads it.
    #[test]
    fn fr4_a_sequence_of_key_presses_produces_the_expected_display() {
        let mut screen = Screen::new();
        assert_eq!(screen.display(), "0");

        for label in ["1", "2", "+", "3", "0", "="] {
            screen.press(label);
        }
        assert_eq!(screen.display(), "42");

        for label in ["\u{00d7}", "2", "="] {
            screen.press(label);
        }
        assert_eq!(screen.display(), "84");

        screen.press("C");
        assert_eq!(screen.display(), "0");
    }

    /// Division, the sign key and the backspace, which are the keys where an off by one in
    /// the wiring would still look like a working calculator until someone checked.
    #[test]
    fn fr4_the_correcting_keys_reach_the_display() {
        let mut screen = Screen::new();
        for label in ["1", "2", "3", "\u{232b}"] {
            screen.press(label);
        }
        assert_eq!(screen.display(), "12");

        screen.press("\u{00f7}");
        screen.press("4");
        screen.press("=");
        assert_eq!(screen.display(), "3");

        screen.press("\u{00b1}");
        assert_eq!(screen.display(), "-3");
    }

    /// Equals writes a line on the tape, and the line is a way back into the number it
    /// holds. A tape that could only be read would be a log.
    #[test]
    fn fr4_a_finished_calculation_lands_on_the_tape_and_can_be_pressed_back_in() {
        let mut screen = Screen::new();
        for label in ["1", "2", "+", "3", "0", "="] {
            screen.press(label);
        }
        assert_eq!(screen.display(), "42");
        assert!(
            screen.labelled("12 + 30 = 42").is_some(),
            "the finished calculation is not on the tape"
        );

        screen.press("C");
        assert_eq!(screen.display(), "0");
        screen.press("12 + 30 = 42");
        assert_eq!(
            screen.display(),
            "42",
            "pressing a tape line should put its answer back in the entry"
        );
    }

    /// Nothing that cannot be worked out goes on the tape, and the user is told once.
    #[test]
    fn fr21_a_calculation_with_no_answer_is_reported_as_a_message() {
        let mut screen = Screen::new();
        for label in ["5", "\u{00f7}", "0", "="] {
            screen.press(label);
        }
        assert_eq!(screen.display(), "Error");
        assert_eq!(
            screen.messages,
            vec![("5 \u{00f7} 0 has no answer".to_owned(), String::new())],
            "a result the calculator cannot show should be said once, not written down"
        );
        assert!(
            screen.labelled("Clear").is_some(),
            "the tape panel should still be on the screen"
        );
    }

    /// Throwing the tape away offers it back, the way deleting does everywhere else in
    /// these samples.
    #[test]
    fn fr21_clearing_the_tape_offers_it_back() {
        let mut screen = Screen::new();
        for label in ["6", "\u{00d7}", "7", "="] {
            screen.press(label);
        }
        assert!(screen.labelled("6 \u{00d7} 7 = 42").is_some());

        screen.press("Clear");
        assert!(
            screen.labelled("6 \u{00d7} 7 = 42").is_none(),
            "the tape should be empty after it is cleared"
        );
        assert_eq!(
            screen.messages,
            vec![("Cleared 1 line".to_owned(), "Undo".to_owned())],
            "clearing should say what it threw away and offer it back"
        );
    }

    /// A message with an action has to name a duration the wire can carry, and the long
    /// one is what an undo needs: an offer that leaves before it can be read is not one.
    #[test]
    fn fr21_the_undo_offer_stays_the_longer_of_the_two_durations() {
        let mut screen = Screen::new();
        for label in ["6", "\u{00d7}", "7", "="] {
            screen.press(label);
        }
        let node_id = screen.labelled("Clear").expect("the tape has no clear");
        let handler = screen.clicks[&node_id];
        encode_event(
            &HostEvent {
                node_id,
                handler_id: handler,
                payload: EventPayload::Clicked,
            },
            &mut screen.event,
        )
        .expect("the event did not encode");
        let (batch, _) = screen
            .host
            .dispatch_event(&screen.event)
            .expect("the event failed");
        let durations: Vec<MessageDuration> = decode_batch(batch)
            .expect("the frame did not decode")
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::ShowMessage { duration, .. } => Some(*duration),
                _ => None,
            })
            .collect();
        assert_eq!(durations, vec![MessageDuration::Long]);
    }
}
