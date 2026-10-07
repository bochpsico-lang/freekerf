//! The analysis report of `freekerf analyze --json`.
//!
//! Same keys and number formatting as `conformance`'s `golden.py`, so the golden
//! files of the conformance suite can be compared with this output directly.

use rust_decimal::Decimal;
use serde::Serialize;

use crate::number::f64_from_decimal;
use crate::program::Program;

/// Power range of the report.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SpindleRange {
    /// Lowest S.
    pub min: f64,
    /// Highest S.
    pub max: f64,
    /// LaserGRBL's validity rule (two values, positive max).
    pub valid: bool,
}

/// JSON report of one program.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// File name (without directories).
    pub input: String,
    /// Normalized command texts.
    pub commands: Vec<String>,
    /// What each command puts on the serial line (without the newline).
    pub wire: Vec<String>,
    /// Cumulative estimated time at the end of each command, in seconds.
    pub offsets_s: Vec<f64>,
    /// Estimated total time in seconds.
    pub estimated_time_s: f64,
    /// `[xmin, ymin, xmax, ymax]` where the laser burns.
    pub drawing_range: Option<[f64; 4]>,
    /// `[xmin, ymin, xmax, ymax]` where the head moves (origin included).
    pub moving_range: Option<[f64; 4]>,
    /// Powers used.
    pub spindle_range: Option<SpindleRange>,
    /// Quadrant of the drawing.
    pub quadrant: String,
    /// Lines a device receives for a full job run (filled by the caller that ran it).
    pub stream: Vec<String>,
}

impl Report {
    /// Builds the report of `program` (the `stream` is left empty).
    #[must_use]
    pub fn new(input: &str, program: &Program) -> Self {
        let range = program.range();
        let to_box = |b: Option<[Decimal; 4]>| b.map(|v| v.map(num));
        Self {
            input: input.to_owned(),
            commands: program.commands().map(|c| c.text().to_owned()).collect(),
            wire: program
                .commands()
                .map(|c| {
                    let mut w = c.wire();
                    w.pop();
                    w
                })
                .collect(),
            offsets_s: program
                .lines()
                .iter()
                .map(|l| millis_to_seconds(l.offset_ms))
                .collect(),
            estimated_time_s: millis_to_seconds(program.estimated_ms()),
            drawing_range: to_box(range.drawing.as_box()),
            moving_range: to_box(range.moving.as_box()),
            spindle_range: range.spindle.s.bounds().map(|(min, max)| SpindleRange {
                min: num(min),
                max: num(max),
                valid: range.spindle.is_valid(),
            }),
            quadrant: program.quadrant().to_string(),
            stream: Vec::new(),
        }
    }
}

/// `round(float(str(d)), 6)` of `golden.py`.
#[must_use]
pub fn num(value: Decimal) -> f64 {
    round_to(f64_from_decimal(value), 6)
}

/// Milliseconds as seconds, rounded to 3 decimals.
#[must_use]
pub fn millis_to_seconds(ms: i64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // job times are far below 2^53 ms
    round_to(ms as f64 / 1000.0, 3)
}

/// Python's `round(x, n)`: correctly rounded from the exact binary value.
fn round_to(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::AnalysisOptions;

    #[test]
    fn report_of_a_square() {
        let p = Program::parse(
            "G0 X10 Y10\nM3 S800\nG1 X30 F1200\nM5\n",
            &AnalysisOptions::default(),
        );
        let r = Report::new("square.nc", &p);
        assert_eq!(r.wire, vec!["G0X10Y10", "M3S800", "G1X30F1200", "M5"]);
        assert_eq!(r.drawing_range, Some([10.0, 10.0, 30.0, 10.0]));
        assert_eq!(r.moving_range, Some([0.0, 0.0, 30.0, 10.0]));
        assert_eq!(
            r.spindle_range,
            Some(SpindleRange {
                min: 0.0,
                max: 800.0,
                valid: true
            })
        );
        assert_eq!(r.quadrant, "I");
        let json = serde_json::to_value(&r).unwrap();
        assert!(json.get("stream").is_some());
    }

    #[test]
    fn empty_program_has_no_drawing_and_no_spindle_range() {
        let r = Report::new("e.nc", &Program::new());
        assert_eq!(r.drawing_range, None);
        assert_eq!(r.spindle_range, None);
        assert_eq!(r.quadrant, "Unknown");
        assert_eq!(millis_to_seconds(1047), 1.047);
        assert_eq!(num(Decimal::new(1_234_567_891, 9)), 1.234_568);
    }
}
