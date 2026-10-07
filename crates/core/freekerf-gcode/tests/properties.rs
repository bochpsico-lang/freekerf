//! Property tests: the parser never panics and normalization is stable.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use freekerf_gcode::number::{decimal_from_f64, format_decimal, parse_number};
use freekerf_gcode::{AnalysisOptions, Command, Program};
use proptest::prelude::*;

fn gcode_line() -> impl Strategy<Value = String> {
    // mostly G-code-looking text, sometimes arbitrary
    prop_oneof![
        3 => "[ GMXYZIJRFSPgmxyzijrfsp0-9.\\-();$=]{0,40}",
        1 => any::<String>(),
    ]
}

proptest! {
    #[test]
    fn parsing_never_panics_and_is_idempotent(line in gcode_line()) {
        let c = Command::new(&line);
        // a line is trimmed before it is normalized, so normalizing again only trims
        let again = Command::new(c.text());
        if !c.has_parse_error() {
            prop_assert_eq!(again.text(), c.text().trim());
            prop_assert_eq!(again.words(), c.words());
        }
        prop_assert_eq!(c.wire().len(), c.wire_len());
        prop_assert!(c.wire().ends_with('\n'));
    }

    #[test]
    fn words_format_and_parse_back(line in gcode_line()) {
        let c = Command::new(&line);
        for w in c.words() {
            let back = Command::new(&w.to_string());
            if w.letter.is_ascii_alphabetic() {
                prop_assert_eq!(back.get(w.letter), Some(w.value));
            }
        }
    }

    #[test]
    fn numbers_round_trip(v in -1.0e9f64..1.0e9) {
        let d = decimal_from_f64(v).unwrap();
        let text = format_decimal(d);
        prop_assert_eq!(parse_number(&text), Some(d));
    }

    #[test]
    fn analysis_never_panics(lines in proptest::collection::vec(gcode_line(), 0..40)) {
        let p = Program::parse(&lines.join("\n"), &AnalysisOptions::default());
        // Offsets are not monotonic: a negative dwell or a negative feed (an arc
        // at F-1) gives a negative duration, as in LaserGRBL (see
        // `program::tests::negative_feed_gives_a_negative_offset`).
        // What always holds: the estimate is the offset of the last line.
        let last = p.lines().last().map_or(0, |l| l.offset_ms);
        prop_assert_eq!(p.estimated_ms(), last);
    }
}
