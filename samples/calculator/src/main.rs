//! A working desktop calculator.
//!
//! Written the way an application using this library would be written: `rsx!` and hooks,
//! no Kotlin and no protocol types.

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

/// The longest shared prefix of two strings, rounded down to a character boundary so a
/// multi-byte character is never split.
fn shared_prefix(previous: &str, next: &str) -> usize {
    let mut shared = previous
        .as_bytes()
        .iter()
        .zip(next.as_bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while shared > 0 && (!previous.is_char_boundary(shared) || !next.is_char_boundary(shared)) {
        shared -= 1;
    }
    shared
}

/// The keypad, in the shape the window has room for.
fn keypad(calculator: Signal<Calculator>, wide: bool) -> Element {
    let mut calculator = calculator;
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
                                    on_click: move |_| calculator.write().press(label),
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
                            on_click: move |_| calculator.write().press(label),
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
                            on_click: move |_| calculator.write().press(label),
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
fn readout(status: String, display: String, tall: bool) -> Element {
    rsx! {
        Surface {
            fill_max_width: true,
            fill_max_height: tall,
            Column {
                fill_max_width: true,
                fill_max_height: tall,
                space_role: SpaceRole::Xs,
                // Beside the keys the panel is a column of its own, so the number sits at
                // the foot of it rather than floating in the middle of it.
                arrangement: if tall { Arrangement::End } else { Arrangement::Start },
                // The pending operation is a line only while there is one. An empty line
                // still has a height, and the panel is meant to be as tall as the type it
                // holds and no taller.
                if !status.is_empty() {
                    // Text has no width of its own, so the right alignment of a calculator
                    // display comes from a full-width Box around it.
                    dioxus_compose::Box {
                        fill_max_width: true,
                        alignment: Alignment::CenterEnd,
                        Text {
                            text: status,
                            type_role: TypeRole::Caption,
                            color: Paint::Role(ColorRole::OnSurfaceVariant),
                            max_lines: 1,
                            overflow: TextOverflow::Ellipsis,
                        }
                    }
                }
                dioxus_compose::Box {
                    fill_max_width: true,
                    alignment: Alignment::CenterEnd,
                    Text {
                        text: display,
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
}

fn app() -> Element {
    let window = use_window_size();
    let mut calculator = use_signal(Calculator::new);
    // The keyboard route. Key events only reach the Host for Enter, so every other key is
    // read out of the capture field's value instead: the difference between what the field
    // showed last and what it shows now is the keys that were pressed.
    let mut typed = use_signal(String::new);

    let display = calculator.read().display();
    let status = calculator.read().status();
    // A phone holds one column of keys. Anything wider has room for the functions to leave
    // the grid and stand in a column of their own. A desktop window has more width than a
    // keypad can use: keys that grew to fill it would be the size of a hand, so the pad
    // stops at the width an expanded window starts at and centres under the readout.
    let wide = !window.is_compact();
    let pad_width = if window.is_expanded() {
        Some(WindowSizeClass::EXPANDED_MIN_WIDTH_DP)
    } else {
        None
    };

    let capture = rsx! {
        // The keyboard route, and nothing a calculator has on its case.
        //
        // It exists because key events reach the Host only through a focused field, so the
        // typing path needs somewhere to put the focus. That makes it scaffolding, and it
        // is dressed as scaffolding: no panel behind it, the caption rung rather than the
        // body rung, and the quiet ink. A surface here would have put a development
        // affordance at the same weight as the display.
        dioxus_compose::Box {
            fill_max_width: true,
            TextField {
                fill_max_width: true,
                type_role: TypeRole::Caption,
                placeholder: "Keyboard: 0-9 . + - * / % = Enter",
                on_value_change: move |value: String| {
                    let previous = typed();
                    let shared = shared_prefix(&previous, &value);
                    let removed = previous[shared..].chars().count();
                    let mut state = calculator.write();
                    for _ in 0..removed {
                        state.press("\u{232b}");
                    }
                    for character in value[shared..].chars() {
                        state.press_char(character);
                    }
                    drop(state);
                    typed.set(value);
                },
                on_key_down: move |event: KeyEvent| {
                    if event.key() == Key::Enter {
                        calculator.write().press("=");
                        event.consume();
                    }
                },
            }
        }
    };

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,

            TopAppBar {
                fill_max_width: true,
                Text { text: "Calculator", type_role: TypeRole::Title, weight: 1.0 }
            }

            Column {
                fill_max_width: true,
                fill_max_height: true,
                padding_role: SpaceRole::Lg,
                space_role: SpaceRole::Md,

                {readout(status, display, false)}
                // The pad takes the height the readout and the capture line leave, and on
                // a desktop window it stops widening and centres instead.
                dioxus_compose::Box {
                    fill_max_width: true,
                    weight: 1.0,
                    alignment: Alignment::TopCenter,
                    Column {
                        fill_max_width: pad_width.is_none(),
                        width: pad_width,
                        fill_max_height: true,
                        {keypad(calculator, wide)}
                    }
                }
                {capture}
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
    use dioxus_compose::schema::{EventPayload, Key, PropertyKind, TypeRole, WidgetKind};
    use std::collections::HashMap;

    #[test]
    fn shared_prefix_never_splits_a_character() {
        assert_eq!(shared_prefix("\u{d55c}", "\u{d55c}\u{ae00}"), 3);
        assert_eq!(shared_prefix("\u{d55c}", "\u{ae00}"), 0);
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

    /// The calculator's first frame under each of the six design systems, in both schemes.
    ///
    /// A batch that encodes is not the same as a screen someone can read. Material 3
    /// shipped a readout filled with a colour that matched the page behind it, drawn full
    /// size, in the right colour and invisible, and every assertion in this file passed
    /// the whole time. The only thing that settles it is looking.
    ///
    /// So this writes the frames out when `DXC_FRAME_DIR` is set, as exactly the bytes the
    /// Renderer decodes, and the Renderer's screenshot test turns each one into a PNG.
    /// Unset, which is the normal run, it still checks that all twelve encode: a design
    /// system nobody can render is the failure this whole set of tables exists to avoid.
    #[test]
    fn fr14_the_first_frame_encodes_under_every_design_system() {
        use dioxus_compose::schema::{ColorScheme, DesignSystem, Theme};

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
                let batch = host.rebuild().unwrap_or_else(|error| {
                    panic!("{system:?} {scheme:?} does not encode: {error:?}")
                });
                assert!(
                    !batch.is_empty(),
                    "{system:?} {scheme:?} produced an empty first frame, so there is nothing to draw"
                );
                if let Some(directory) = &directory {
                    let path = std::path::Path::new(directory)
                        .join(format!("Calculator-{system:?}-{scheme:?}.bin"));
                    write_frames(&path, &[batch]);
                }
            }
        }
    }

    #[test]
    fn display_is_formatted_not_raw() {
        assert_eq!(engine::format_number(1.0 / 3.0), "0.333333333333");
    }

    /// The real keypad, driven the way the Renderer drives it: an encoded event naming the
    /// button's own handler, and the display read back out of the frame that came of it.
    /// Calling the engine directly would skip everything that can actually break between a
    /// finger and a number on screen.
    struct Keypad {
        host: Host,
        /// Button label to the handler its `on_click` was given.
        keys: HashMap<String, (u32, u64)>,
        field: u32,
        change_handler: u64,
        key_handler: u64,
        display: u32,
        texts: HashMap<u32, String>,
        event: Vec<u8>,
    }

    impl Keypad {
        fn new() -> Self {
            let mut host = Host::new(app);
            let first = decode_batch(host.rebuild().expect("the first frame failed to encode"))
                .expect("the first frame did not decode");

            let mut texts: HashMap<u32, String> = HashMap::new();
            let mut clicks: HashMap<u32, u64> = HashMap::new();
            let mut display_roles: Vec<u32> = Vec::new();
            let mut field = None;
            let mut change_handler = None;
            let mut key_handler = None;
            for mutation in &first {
                match mutation {
                    Mutation::Create {
                        node_id,
                        widget: WidgetKind::TextField,
                    } => field = Some(*node_id),
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
                            change_handler = Some(*id as u64);
                        }
                        (PropertyKind::OnKeyDown, PropertyValue::Integer(id)) => {
                            key_handler = Some(*id as u64);
                        }
                        (PropertyKind::TypeRole, PropertyValue::Integer(role))
                            if *role == i64::from(TypeRole::Display as u8) =>
                        {
                            display_roles.push(*node_id);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            drop(first);

            let keys = clicks
                .iter()
                .filter_map(|(node_id, handler)| {
                    texts
                        .get(node_id)
                        .map(|label| (label.clone(), (*node_id, *handler)))
                })
                .collect();
            assert_eq!(
                display_roles.len(),
                1,
                "the screen should have exactly one display sized text"
            );

            Self {
                host,
                keys,
                field: field.expect("the screen has no keyboard capture field"),
                change_handler: change_handler.expect("the field declared no change handler"),
                key_handler: key_handler.expect("the field declared no key handler"),
                display: display_roles[0],
                texts,
                event: Vec::new(),
            }
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
            for mutation in decode_batch(batch).expect("the frame did not decode") {
                if let Mutation::SetProp {
                    node_id,
                    property: PropertyKind::Text,
                    value: PropertyValue::String(text),
                } = mutation
                {
                    self.texts.insert(node_id, text.to_owned());
                }
            }
            result
        }

        /// Press a key by the label a person would read on it.
        fn press(&mut self, label: &str) {
            let (node_id, handler) = *self
                .keys
                .get(label)
                .unwrap_or_else(|| panic!("the keypad has no {label} key"));
            self.dispatch(node_id, handler, EventPayload::Clicked);
        }

        /// The keyboard route: the field reports what it now holds, and the difference
        /// against what it held before is the keys that were pressed.
        fn type_text(&mut self, value: &str) {
            let (node_id, handler) = (self.field, self.change_handler);
            self.dispatch(node_id, handler, EventPayload::TextChanged(value));
        }

        fn press_enter(&mut self) -> i64 {
            let (node_id, handler) = (self.field, self.key_handler);
            self.dispatch(
                node_id,
                handler,
                EventPayload::KeyDown {
                    key: Key::Enter,
                    shift_key: false,
                    ctrl_key: false,
                    alt_key: false,
                    meta_key: false,
                },
            )
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
        let mut keypad = Keypad::new();
        assert_eq!(keypad.display(), "0");

        for label in ["1", "2", "+", "3", "0", "="] {
            keypad.press(label);
        }
        assert_eq!(keypad.display(), "42");

        for label in ["\u{00d7}", "2", "="] {
            keypad.press(label);
        }
        assert_eq!(keypad.display(), "84");

        keypad.press("C");
        assert_eq!(keypad.display(), "0");
    }

    /// Division, the sign key and the backspace, which are the keys where an off by one in
    /// the wiring would still look like a working calculator until someone checked.
    #[test]
    fn fr4_the_correcting_keys_reach_the_display() {
        let mut keypad = Keypad::new();
        for label in ["1", "2", "3", "\u{232b}"] {
            keypad.press(label);
        }
        assert_eq!(keypad.display(), "12");

        keypad.press("\u{00f7}");
        keypad.press("4");
        keypad.press("=");
        assert_eq!(keypad.display(), "3");

        keypad.press("\u{00b1}");
        assert_eq!(keypad.display(), "-3");
    }

    /// Typing is a second route to the same engine. The field is uncontrolled, so what
    /// arrives is the whole value each time and the app works out what changed.
    #[test]
    fn fr4_typing_reaches_the_display_and_enter_is_consumed() {
        let mut keypad = Keypad::new();
        keypad.type_text("7");
        keypad.type_text("7*");
        keypad.type_text("7*6");
        assert_eq!(keypad.display(), "6");

        let consumed = keypad.press_enter();
        assert_eq!(keypad.display(), "42");
        assert_eq!(
            consumed, 1,
            "Enter has to be reported as handled, or the field inserts a newline as well"
        );
    }

    /// Deleting characters in the capture field undoes the presses they made, so the two
    /// routes cannot drift apart.
    #[test]
    fn fr4_deleting_typed_characters_undoes_those_presses() {
        let mut keypad = Keypad::new();
        keypad.type_text("123");
        assert_eq!(keypad.display(), "123");
        keypad.type_text("12");
        assert_eq!(keypad.display(), "12");
        keypad.type_text("");
        assert_eq!(keypad.display(), "0");
    }
}
