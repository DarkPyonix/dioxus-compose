//! A working desktop calculator.
//!
//! Written the way an application using this library would be written: `rsx!` and hooks,
//! no Kotlin and no protocol types.

use dioxus_compose::prelude::*;

mod engine;

use engine::Calculator;

/// The key grid, top to bottom and left to right.
const ROWS: [[&str; 4]; 5] = [
    ["C", "\u{232b}", "%", "\u{00f7}"],
    ["7", "8", "9", "\u{00d7}"],
    ["4", "5", "6", "\u{2212}"],
    ["1", "2", "3", "+"],
    ["\u{00b1}", "0", ".", "="],
];

fn variant_for(label: &str) -> ButtonVariant {
    match label {
        "=" => ButtonVariant::Filled,
        "\u{00f7}" | "\u{00d7}" | "\u{2212}" | "+" => ButtonVariant::Tonal,
        "C" | "\u{232b}" | "%" | "\u{00b1}" => ButtonVariant::Outlined,
        _ => ButtonVariant::Text,
    }
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

fn app() -> Element {
    let mut calculator = use_signal(Calculator::new);
    // The keyboard route. Key events only reach the Host for Enter, so every other key is
    // read out of the capture field's value instead: the difference between what the field
    // showed last and what it shows now is the keys that were pressed.
    let mut typed = use_signal(String::new);

    let display = calculator.read().display();
    let status = calculator.read().status();

    rsx! {
        Column {
            fill_max_width: true,
            fill_max_height: true,
            spacing: 6.0,
            TopAppBar {
                Text { text: "Calculator", type_role: TypeRole::Title }
            }
            // Text has no width of its own, so the right alignment of a calculator display
            // comes from a full-width Box around it.
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
            dioxus_compose::Box {
                fill_max_width: true,
                alignment: Alignment::CenterEnd,
                Text {
                    text: display,
                    type_role: TypeRole::Display,
                    max_lines: 1,
                    overflow: TextOverflow::Ellipsis,
                }
            }
            for (index , row) in ROWS.iter().enumerate() {
                Row {
                    key: "row-{index}",
                    fill_max_width: true,
                    arrangement: Arrangement::SpaceEvenly,
                    for label in row.iter().copied() {
                        Button {
                            key: "{label}",
                            text: label,
                            variant: variant_for(label),
                            on_click: move |_| calculator.write().press(label),
                        }
                    }
                }
            }
            Text {
                text: "Keyboard: click the field below, then type 0-9 . + - * / % = and Enter",
                type_role: TypeRole::Caption,
                color: Paint::Role(ColorRole::OnSurfaceVariant),
            }
            TextField {
                placeholder: "Keyboard input",
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
    }
}

fn main() {
    dioxus_compose::launch(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_prefix_never_splits_a_character() {
        assert_eq!(shared_prefix("\u{d55c}", "\u{d55c}\u{ae00}"), 3);
        assert_eq!(shared_prefix("\u{d55c}", "\u{ae00}"), 0);
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

    #[test]
    fn display_is_formatted_not_raw() {
        assert_eq!(engine::format_number(1.0 / 3.0), "0.333333333333");
    }
}
