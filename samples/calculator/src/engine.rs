//! Arithmetic and display formatting, kept apart from the UI so both can be tested
//! without a renderer.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Operation {
    fn apply(self, left: f64, right: f64) -> f64 {
        match self {
            Self::Add => left + right,
            Self::Subtract => left - right,
            Self::Multiply => left * right,
            Self::Divide => left / right,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "\u{2212}",
            Self::Multiply => "\u{00d7}",
            Self::Divide => "\u{00f7}",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label {
            "+" => Some(Self::Add),
            "\u{2212}" | "-" => Some(Self::Subtract),
            "\u{00d7}" | "*" | "x" | "X" => Some(Self::Multiply),
            "\u{00f7}" | "/" => Some(Self::Divide),
            _ => None,
        }
    }
}

/// Digits kept in an entry. Past this a double cannot tell two entries apart, so more
/// digits would only be a longer lie.
const MAX_ENTRY_DIGITS: usize = 15;

/// Significant digits shown. A double carries between 15 and 17, and the last two or three
/// are where the binary representation shows through: `0.1 + 0.2` is `0.3` at twelve
/// significant digits and `0.30000000000000004` at seventeen.
const SIGNIFICANT_DIGITS: i32 = 12;

/// Values at or above this are shown in exponent form, because the plain form would be
/// longer than the significant digits that back it.
const EXPONENT_ABOVE: f64 = 1e12;
/// Values below this, and not zero, are shown in exponent form for the same reason.
const EXPONENT_BELOW: f64 = 1e-9;

/// A number as a calculator shows it: no trailing zeros, no false precision, and an
/// exponent only where the plain form would be unreadable.
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "Error".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    let magnitude = value.abs();
    if magnitude >= EXPONENT_ABOVE || magnitude < EXPONENT_BELOW {
        return format_exponent(value);
    }
    // Leading zeros are not significant, so a value below one gets more decimals rather
    // than fewer: this is what keeps `1/3` at twelve threes instead of eleven.
    let integer_digits = magnitude.log10().floor() as i32 + 1;
    let decimals = (SIGNIFICANT_DIGITS - integer_digits).clamp(0, 15) as usize;
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        return "0".to_owned();
    }
    text
}

fn format_exponent(value: f64) -> String {
    let text = format!("{:.*e}", (SIGNIFICANT_DIGITS - 1) as usize, value);
    let Some((mantissa, exponent)) = text.split_once('e') else {
        return text;
    };
    let mantissa = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    if exponent.starts_with('-') {
        format!("{mantissa}e{exponent}")
    } else {
        format!("{mantissa}e+{exponent}")
    }
}

/// The calculator's whole state.
///
/// `entry` is what the user is typing right now. When it is empty the display shows
/// `value`, which is the running result.
#[derive(Clone, Debug)]
pub struct Calculator {
    entry: String,
    typing: bool,
    value: f64,
    pending: Option<Operation>,
    /// The operation and right operand to reuse when `=` is pressed again.
    repeat: Option<(Operation, f64)>,
    error: bool,
}

impl Default for Calculator {
    fn default() -> Self {
        Self::new()
    }
}

impl Calculator {
    pub fn new() -> Self {
        Self {
            entry: String::new(),
            typing: false,
            value: 0.0,
            pending: None,
            repeat: None,
            error: false,
        }
    }

    /// What the big line reads.
    pub fn display(&self) -> String {
        if self.error {
            return "Error".to_owned();
        }
        if self.typing {
            return match self.entry.as_str() {
                "" => "0".to_owned(),
                "-" => "-0".to_owned(),
                entry => entry.to_owned(),
            };
        }
        format_number(self.value)
    }

    /// The small line above the display: the running value and the operation waiting for
    /// its right-hand side.
    pub fn status(&self) -> String {
        match self.pending {
            Some(operation) => format!("{} {}", format_number(self.value), operation.symbol()),
            None => String::new(),
        }
    }

    /// The value the next operation will use: what is being typed, or the running result.
    fn operand(&self) -> f64 {
        if self.typing {
            self.entry.parse().unwrap_or(0.0)
        } else {
            self.value
        }
    }

    fn settle(&mut self, result: f64) {
        self.value = result;
        self.entry.clear();
        self.typing = false;
        self.error = !result.is_finite();
    }

    /// One key, named by the label printed on it. Keyboard characters come in through
    /// [`Calculator::press_char`], which maps them onto the same labels.
    pub fn press(&mut self, label: &str) {
        match label {
            "C" => *self = Self::new(),
            "\u{232b}" => self.backspace(),
            "%" => self.percent(),
            "\u{00b1}" => self.flip_sign(),
            "." => self.decimal_point(),
            "=" => self.equals(),
            digit if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => {
                self.digit(digit.as_bytes()[0] as char);
            }
            operator => {
                if let Some(operation) = Operation::from_label(operator) {
                    self.operation(operation);
                }
            }
        }
    }

    /// A typed character, for the keyboard route.
    pub fn press_char(&mut self, character: char) {
        match character {
            '0'..='9' => self.digit(character),
            '.' | ',' => self.decimal_point(),
            '+' | '-' | '*' | 'x' | 'X' | '/' => {
                if let Some(operation) = Operation::from_label(&character.to_string()) {
                    self.operation(operation);
                }
            }
            '%' => self.percent(),
            '=' | '\n' | '\r' => self.equals(),
            'c' | 'C' => *self = Self::new(),
            'n' | 'N' => self.flip_sign(),
            '\u{8}' | '\u{7f}' => self.backspace(),
            _ => {}
        }
    }

    fn digit(&mut self, digit: char) {
        if self.error {
            *self = Self::new();
        }
        if !self.typing {
            self.entry.clear();
            self.typing = true;
        }
        if self.entry == "0" {
            self.entry.clear();
        } else if self.entry == "-0" {
            self.entry = "-".to_owned();
        }
        if self.entry.chars().filter(char::is_ascii_digit).count() >= MAX_ENTRY_DIGITS {
            return;
        }
        self.entry.push(digit);
    }

    fn decimal_point(&mut self) {
        if self.error {
            *self = Self::new();
        }
        if !self.typing {
            self.entry = "0.".to_owned();
            self.typing = true;
            return;
        }
        if self.entry.contains('.') {
            return;
        }
        if self.entry.is_empty() || self.entry == "-" {
            self.entry.push('0');
        }
        self.entry.push('.');
    }

    fn flip_sign(&mut self) {
        if self.typing {
            if let Some(rest) = self.entry.strip_prefix('-') {
                self.entry = rest.to_owned();
            } else {
                self.entry.insert(0, '-');
            }
        } else if self.value != 0.0 {
            self.value = -self.value;
        }
    }

    /// Percent reads as "of the running value" next to a plus or a minus, which is what
    /// makes `200 + 10 %` come out as 220, and as "divided by a hundred" everywhere else.
    fn percent(&mut self) {
        if self.error {
            return;
        }
        let operand = self.operand();
        let result = match self.pending {
            Some(Operation::Add) | Some(Operation::Subtract) => self.value * operand / 100.0,
            _ => operand / 100.0,
        };
        self.entry = format_number(result);
        self.typing = true;
    }

    fn backspace(&mut self) {
        if self.error {
            *self = Self::new();
            return;
        }
        if self.typing {
            self.entry.pop();
            if self.entry.is_empty() || self.entry == "-" {
                self.entry.clear();
                self.typing = false;
                self.value = 0.0;
            }
        } else {
            self.value = 0.0;
        }
    }

    fn operation(&mut self, operation: Operation) {
        if self.error {
            return;
        }
        let operand = self.operand();
        let result = match self.pending {
            // Two operators in a row replace each other rather than folding a value in
            // twice, so `3 + ×` means `3 ×`.
            Some(pending) if self.typing => pending.apply(self.value, operand),
            _ => operand,
        };
        self.settle(result);
        self.pending = Some(operation);
        self.repeat = None;
    }

    fn equals(&mut self) {
        if self.error {
            return;
        }
        if let Some(pending) = self.pending {
            let operand = self.operand();
            let result = pending.apply(self.value, operand);
            self.settle(result);
            self.repeat = Some((pending, operand));
            self.pending = None;
        } else if let Some((operation, operand)) = self.repeat {
            let result = operation.apply(self.value, operand);
            self.settle(result);
        } else {
            let operand = self.operand();
            self.settle(operand);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(keys: &str) -> String {
        let mut calculator = Calculator::new();
        for character in keys.chars() {
            calculator.press_char(character);
        }
        calculator.display()
    }

    #[test]
    fn adds_and_multiplies() {
        assert_eq!(run("12+34="), "46");
        assert_eq!(run("6*7="), "42");
        assert_eq!(run("9-11="), "-2");
    }

    #[test]
    fn chains_operations_left_to_right() {
        assert_eq!(run("2+3*4="), "20");
    }

    #[test]
    fn repeated_equals_repeats_the_last_operation() {
        assert_eq!(run("2+3==="), "11");
    }

    #[test]
    fn a_repeating_result_is_cut_at_twelve_significant_digits() {
        assert_eq!(run("1/3="), "0.333333333333");
        assert_eq!(run("2/3="), "0.666666666667");
    }

    #[test]
    fn binary_representation_does_not_leak_into_the_display() {
        assert_eq!(run("0.1+0.2="), "0.3");
    }

    #[test]
    fn a_long_result_falls_back_to_an_exponent() {
        assert_eq!(run("99999999*99999999="), "9.9999998e+15");
        assert_eq!(format_number(0.000000000123), "1.23e-10");
    }

    #[test]
    fn dividing_by_zero_reports_an_error_and_recovers() {
        let mut calculator = Calculator::new();
        for character in "5/0=".chars() {
            calculator.press_char(character);
        }
        assert_eq!(calculator.display(), "Error");
        calculator.press("C");
        assert_eq!(calculator.display(), "0");
    }

    #[test]
    fn decimal_point_is_added_once() {
        assert_eq!(run("1.2.3"), "1.23");
        assert_eq!(run("."), "0.");
    }

    #[test]
    fn sign_flip_applies_to_the_entry_and_to_the_result() {
        let mut calculator = Calculator::new();
        for character in "5".chars() {
            calculator.press_char(character);
        }
        calculator.press("\u{00b1}");
        assert_eq!(calculator.display(), "-5");
        calculator.press("=");
        calculator.press("\u{00b1}");
        assert_eq!(calculator.display(), "5");
    }

    #[test]
    fn percent_reads_as_a_share_of_the_running_value_next_to_a_sum() {
        assert_eq!(run("200+10%="), "220");
        assert_eq!(run("50%"), "0.5");
    }

    #[test]
    fn backspace_removes_one_character_of_the_entry() {
        assert_eq!(run("123\u{8}"), "12");
        assert_eq!(run("5\u{8}"), "0");
    }

    #[test]
    fn an_entry_stops_growing_once_a_double_cannot_tell_it_apart() {
        assert_eq!(run("12345678901234567890").len(), 15);
    }

    #[test]
    fn a_second_operator_replaces_the_first() {
        assert_eq!(run("3+*4="), "12");
    }
}
