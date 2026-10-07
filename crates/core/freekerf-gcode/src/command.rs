//! One line of G-code (or a Grbl `$` system command) as LaserGRBL sees it.
//!
//! Port of `GrblCommand` (`GrblCommand.cs`). The line is normalized when it is built:
//! comments are removed and runs of spaces collapse to one; letters and numbers
//! become [`Word`]s. Parsing stops silently at the first malformed number or repeated
//! letter (kept on purpose, see `doc/divergences.md` DIV-001): the words before the
//! error are kept and the line text is left as it was.

use rust_decimal::Decimal;

use crate::number::{format_decimal, parse_number};

/// A letter with its number, e.g. `X10.5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Word {
    /// The letter as written (upper case unless the command preserved the case).
    pub letter: char,
    /// The number, with its scale (`1.50` stays `1.50` when formatted).
    pub value: Decimal,
}

impl Word {
    /// Builds a word.
    #[must_use]
    pub const fn new(letter: char, value: Decimal) -> Self {
        Self { letter, value }
    }

    /// Parses a word written as a letter and an invariant number (`"G0"`, `"X-1.5"`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut chars = text.chars();
        let letter = chars.next()?;
        parse_number(chars.as_str()).map(|value| Self { letter, value })
    }

    /// True when the letter matches and the number is numerically equal.
    #[must_use]
    pub fn is(&self, letter: char, value: i64) -> bool {
        self.letter == letter && self.value == Decimal::from(value)
    }
}

impl std::fmt::Display for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.letter, format_decimal(self.value))
    }
}

/// A command line, normalized at construction (`BuildHelper` of LaserGRBL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    text: String,
    words: Vec<Word>,
    parse_error: bool,
    repeat: u32,
    jog: bool,
}

impl Command {
    /// A command from a line of a file or typed by the user: trimmed, upper case.
    #[must_use]
    pub fn new(line: &str) -> Self {
        Self::with_options(line, 0, false)
    }

    /// A command keeping the case of the letters (custom code, `$N=V` writes).
    #[must_use]
    pub fn preserving_case(line: &str) -> Self {
        Self::with_options(line, 0, true)
    }

    /// Full constructor: `repeat` counts retries (repeat-on-error streaming).
    #[must_use]
    pub fn with_options(line: &str, repeat: u32, preserve_case: bool) -> Self {
        let trimmed = line.trim();
        let text = if preserve_case { trimmed.to_owned() } else { to_upper(trimmed) };
        let mut cmd = Self { text, words: Vec::new(), parse_error: false, repeat, jog: false };
        cmd.build();
        cmd
    }

    /// A Grbl 1.1 jog line (`$J=...`): analyzed as the motion after the `$J=` prefix.
    #[must_use]
    pub fn jog(line: &str) -> Self {
        let body = line.get(3..).unwrap_or("");
        let mut cmd = Self::new(body);
        cmd.jog = true;
        cmd
    }

    /// A command made of words separated by spaces (`G91 G94 M5`).
    #[must_use]
    pub fn from_words(words: &[Word]) -> Self {
        let mut line = String::new();
        for w in words {
            line.push_str(&w.to_string());
            line.push(' ');
        }
        Self::new(&line)
    }

    /// `first` followed by the text of `rest` (used to add a motion mode on resume).
    #[must_use]
    pub fn prefixed(first: Word, rest: &Self) -> Self {
        Self::new(&format!("{first} {}", rest.text))
    }

    fn build(&mut self) {
        if self.is_system() {
            return; // Grbl `$` commands are not parsed
        }
        let mut words: Vec<Word> = Vec::new();
        let mut letter: Option<char> = None;
        let mut num = String::new();
        let mut comment = false;
        let mut old_space = false;
        let mut out = String::with_capacity(self.text.len());

        let add = |letter: char, num: &str, words: &mut Vec<Word>| -> bool {
            match parse_number(num) {
                Some(value) if !words.iter().any(|w| w.letter == letter) => {
                    words.push(Word { letter, value });
                    true
                }
                _ => false,
            }
        };

        for c in self.text.chars() {
            if c == ';' {
                break;
            }
            if c == '(' {
                comment = true;
            }
            let space = c == ' ';
            if !comment {
                if space && !old_space {
                    out.push(' ');
                } else if !space {
                    out.push(c);
                }
            }
            old_space = space;
            if !comment {
                if c.is_alphabetic() {
                    if let Some(l) = letter
                        && !add(l, &num, &mut words) {
                            self.words = words;
                            self.parse_error = true;
                            return; // text left as it was
                        }
                    letter = Some(c);
                    num.clear();
                } else if c.is_numeric() || c == '.' || c == '-' {
                    num.push(c);
                }
            }
            if c == ')' {
                comment = false;
            }
        }
        self.text = out;
        if let Some(l) = letter
            && !add(l, &num, &mut words) {
                self.parse_error = true;
            }
        self.words = words;
    }

    /// The (normalized) text of the command.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The bytes put on the serial line, newline included (`SerialData`): spaces are
    /// removed except for `$` commands, which are sent as they are.
    #[must_use]
    pub fn wire(&self) -> String {
        let trimmed = self.text.trim_matches(['\r', '\n', ' ']);
        let mut s = if self.is_system() { trimmed.to_owned() } else { trimmed.replace(' ', "") };
        s.push('\n');
        s
    }

    /// Length in bytes of [`Command::wire`].
    #[must_use]
    pub fn wire_len(&self) -> usize {
        let trimmed = self.text.trim_matches(['\r', '\n', ' ']);
        let body = if self.is_system() { trimmed.len() } else { trimmed.bytes().filter(|b| *b != b' ').count() };
        body + 1
    }

    /// Parsed words, in order.
    #[must_use]
    pub fn words(&self) -> &[Word] {
        &self.words
    }

    /// True when parsing stopped at a malformed number or a repeated letter.
    #[must_use]
    pub const fn has_parse_error(&self) -> bool {
        self.parse_error
    }

    /// Retry counter (0 for the first attempt).
    #[must_use]
    pub const fn repeat_count(&self) -> u32 {
        self.repeat
    }

    /// True for a `$J=` jog command.
    #[must_use]
    pub const fn is_jog(&self) -> bool {
        self.jog
    }

    /// The word with `letter`, if any.
    #[must_use]
    pub fn get(&self, letter: char) -> Option<Decimal> {
        self.words.iter().find(|w| w.letter == letter).map(|w| w.value)
    }

    /// The word with `letter`, if any.
    #[must_use]
    pub fn word(&self, letter: char) -> Option<Word> {
        self.words.iter().find(|w| w.letter == letter).copied()
    }

    fn g_is(&self, n: i64) -> bool {
        self.get('G') == Some(Decimal::from(n))
    }

    fn m_is(&self, n: i64) -> bool {
        self.get('M') == Some(Decimal::from(n))
    }

    /// A Grbl system command (starts with `$`).
    #[must_use]
    pub fn is_system(&self) -> bool {
        self.text.starts_with('$')
    }

    /// Nothing to send (only possible before normalization removed everything).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// A `$N=V` settings write (sent synchronously, it writes the EEPROM).
    #[must_use]
    pub fn is_settings_write(&self) -> bool {
        self.is_system() && is_settings_line(&self.text)
    }

    /// `G92` (set work coordinate offset).
    #[must_use]
    pub fn is_set_wco(&self) -> bool {
        self.g_is(92)
    }

    /// A straight move: has X, Y or Z and no arc parameter.
    #[must_use]
    pub fn is_linear_movement(&self) -> bool {
        !self.is_set_wco()
            && (self.get('X').is_some() || self.get('Y').is_some() || self.get('Z').is_some())
            && self.get('I').is_none()
            && self.get('J').is_none()
            && self.get('R').is_none()
    }

    /// An arc: has I, J or R.
    #[must_use]
    pub fn is_arc_movement(&self) -> bool {
        !self.is_set_wco() && (self.get('I').is_some() || self.get('J').is_some() || self.get('R').is_some())
    }

    /// Any move.
    #[must_use]
    pub fn is_movement(&self) -> bool {
        self.is_linear_movement() || self.is_arc_movement()
    }

    /// `G4` dwell.
    #[must_use]
    pub fn is_pause(&self) -> bool {
        self.g_is(4)
    }

    /// `G90` in this command.
    #[must_use]
    pub fn is_absolute_coord(&self) -> bool {
        self.g_is(90)
    }

    /// `G91` in this command.
    #[must_use]
    pub fn is_relative_coord(&self) -> bool {
        self.g_is(91)
    }

    /// `M3` or `M4`.
    #[must_use]
    pub fn is_laser_on(&self) -> bool {
        self.m_is(3) || self.m_is(4)
    }

    /// `M5`.
    #[must_use]
    pub fn is_laser_off(&self) -> bool {
        self.m_is(5)
    }
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// `^[$](\d+)\s*=(.*)` of `GrblConfST.IsSetConf`.
#[must_use]
pub fn is_settings_line(line: &str) -> bool {
    parse_settings_line(line).is_some()
}

/// Splits `$N=V` into its number and its (trimmed) value.
#[must_use]
pub fn parse_settings_line(line: &str) -> Option<(u32, &str)> {
    let rest = line.strip_prefix('$')?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let number: u32 = rest[..digits].parse().ok()?;
    let after = rest[digits..].trim_start_matches(char::is_whitespace);
    let value = after.strip_prefix('=')?;
    Some((number, value.trim()))
}

fn to_upper(s: &str) -> String {
    s.chars()
        .map(|c| {
            let mut up = c.to_uppercase();
            match (up.next(), up.next()) {
                (Some(u), None) => u,
                _ => c, // no single-char upper case (e.g. ß): keep it, as .NET does
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_spaces_and_comments() {
        assert_eq!(Command::new("g0   x1   y1 ; move").text(), "G0 X1 Y1 ");
        assert_eq!(Command::new("M3 S100 (inline comment) G1 X2").text(), "M3 S100  G1 X2");
        assert_eq!(Command::new("  G1 X4 (two) Y4 (comments)").text(), "G1 X4  Y4 ");
        assert_eq!(Command::new("(header comment line)").text(), "");
        assert_eq!(Command::new(";only").text(), "");
        assert_eq!(Command::new("g1x3y3f500").text(), "G1X3Y3F500");
    }

    #[test]
    fn wire_strips_spaces_except_for_system_commands() {
        assert_eq!(Command::new("G1 X4  Y4 ").wire(), "G1X4Y4\n");
        assert_eq!(Command::new("$J=G91 X1").wire(), "$J=G91 X1\n");
        assert_eq!(Command::new("").wire(), "\n");
        for line in ["G1 X4  Y4 ", "$J=G91 X1", "", "M3 S100  G1 X2"] {
            let c = Command::new(line);
            assert_eq!(c.wire_len(), c.wire().len());
        }
    }

    #[test]
    fn words_are_parsed_and_queried() {
        let c = Command::new("G2 X10 Y0 I5 J0");
        assert_eq!(c.get('X'), Some(Decimal::from(10)));
        assert!(c.is_arc_movement() && c.is_movement() && !c.is_linear_movement());
        assert!(Command::new("G1 X1").is_linear_movement());
        assert!(!Command::new("G92 X0").is_movement());
        assert!(Command::new("G92 X0").is_set_wco());
        assert!(Command::new("G4 P1").is_pause());
        assert!(Command::new("G90").is_absolute_coord());
        assert!(Command::new("G91").is_relative_coord());
        assert!(Command::new("M4 S1").is_laser_on());
        assert!(Command::new("M5").is_laser_off());
        assert_eq!(Command::new("X1.50").word('X').unwrap().to_string(), "X1.50");
        assert!(Command::new("G1 X1").word('Y').is_none());
    }

    #[test]
    fn malformed_number_stops_parsing_and_keeps_text() {
        let c = Command::new("G1 X1..5 Y2");
        assert_eq!(c.text(), "G1 X1..5 Y2");
        assert_eq!(c.words(), &[Word::parse("G1").unwrap()]);
        assert!(c.has_parse_error());

        let c = Command::new("G90 G1 X3");
        assert_eq!(c.words().len(), 1);
        assert!(c.has_parse_error());

        // the error in the last word happens after the text was normalized
        let c = Command::new("X5  Y5 Z");
        assert_eq!(c.text(), "X5 Y5 Z");
        assert_eq!(c.words().len(), 2);
        assert!(c.has_parse_error());
    }

    #[test]
    fn system_commands_are_not_parsed() {
        let c = Command::new("$H");
        assert!(c.is_system() && c.words().is_empty());
        assert!(Command::new("$110=4000").is_settings_write());
        assert!(Command::new("$110 = 4000").is_settings_write());
        assert!(!Command::new("$H").is_settings_write());
        assert_eq!(parse_settings_line("$32=1 "), Some((32, "1")));
        assert_eq!(parse_settings_line("$x=1"), None);
        assert_eq!(parse_settings_line("$1"), None);
        assert_eq!(parse_settings_line("G1"), None);
    }

    #[test]
    fn case_is_kept_on_request() {
        let c = Command::preserving_case("$sta/ssid=Home");
        assert_eq!(c.text(), "$sta/ssid=Home");
        assert_eq!(Command::new("ßx1").text(), "ßX1");
        assert_eq!(Command::with_options("g1", 2, false).repeat_count(), 2);
    }

    #[test]
    fn jog_and_composition() {
        let j = Command::jog("$J=G91X2.5Y2.5F800");
        assert!(j.is_jog());
        assert_eq!(j.get('F'), Some(Decimal::from(800)));
        assert_eq!(Command::jog("$J").text(), "");
        let words = [Word::parse("G91").unwrap(), Word::parse("M5").unwrap()];
        assert_eq!(Command::from_words(&words).text(), "G91 M5");
        assert_eq!(Command::from_words(&[]).text(), "");
        let resumed = Command::prefixed(Word::parse("G1").unwrap(), &Command::new("X1 Y2"));
        assert_eq!(resumed.text(), "G1 X1 Y2");
        assert_eq!(resumed.to_string(), "G1 X1 Y2");
        assert!(Word::parse("G1").unwrap().is('G', 1));
        assert!(Word::parse("").is_none());
    }
}
