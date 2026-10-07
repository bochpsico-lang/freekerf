//! A loaded G-code program and its analysis (`GrblFile`).

use crate::command::Command;
use crate::range::{ProgramRange, Quadrant};
use crate::state::{MachineState, TimingConfig};
use rust_decimal::Decimal;

/// Options of the analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisOptions {
    /// Machine limits for the time estimate.
    pub timing: TimingConfig,
    /// "Support Hardware PWM" (default true): `S0` does not burn.
    pub support_pwm: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            timing: TimingConfig::default(),
            support_pwm: true,
        }
    }
}

/// A command with the estimated time at its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramLine {
    /// The command.
    pub command: Command,
    /// Cumulative estimated time at the end of this command, in milliseconds.
    pub offset_ms: i64,
}

/// A G-code program: its commands, ranges and estimated time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    lines: Vec<ProgramLine>,
    range: ProgramRange,
    estimated_ms: i64,
}

impl Program {
    /// An empty program.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses `text` (any line ending; blank lines skipped) and analyzes it.
    #[must_use]
    pub fn parse(text: &str, options: &AnalysisOptions) -> Self {
        let mut p = Self::new();
        p.append_text(text);
        p.analyze(options);
        p
    }

    /// Parses UTF-8 bytes (invalid sequences replaced, BOM ignored).
    #[must_use]
    pub fn parse_bytes(bytes: &[u8], options: &AnalysisOptions) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        Self::parse(text, options)
    }

    /// Adds the lines of `text` without analyzing (call [`Program::analyze`] after).
    pub fn append_text(&mut self, text: &str) {
        let lines: Vec<&str> = split_lines(text)
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        self.lines
            .extend(parse_lines(&lines).into_iter().map(|command| ProgramLine {
                command,
                offset_ms: 0,
            }));
    }

    /// Appends commands (not analyzed until [`Program::analyze`]).
    pub fn extend<I: IntoIterator<Item = Command>>(&mut self, commands: I) {
        self.lines.extend(commands.into_iter().map(|command| ProgramLine {
            command,
            offset_ms: 0,
        }));
    }

    /// Recomputes ranges and times from the first command.
    pub fn analyze(&mut self, options: &AnalysisOptions) {
        let mut state = MachineState::new(options.support_pwm);
        let mut range = ProgramRange::default();
        range.update_xy(Some(Decimal::ZERO), Some(Decimal::ZERO), false);
        let mut total: i64 = 0;
        for line in &mut self.lines {
            let delay = state.analyze(&line.command, Some(&options.timing));
            range.spindle.s.update(state.s().value());
            if let Some(arc) = state.last_arc() {
                let b = arc.bbox;
                range.update_rect(b.x, b.y, b.width, b.height, state.laser_burning());
            } else {
                range.update_xy(
                    Some(state.x().value()),
                    Some(state.y().value()),
                    state.laser_burning(),
                );
            }
            total = total.saturating_add(delay);
            line.offset_ms = total;
        }
        self.range = range;
        self.estimated_ms = total;
    }

    /// The lines.
    #[must_use]
    pub fn lines(&self) -> &[ProgramLine] {
        &self.lines
    }

    /// Iterates the commands.
    pub fn commands(&self) -> impl ExactSizeIterator<Item = &Command> {
        self.lines.iter().map(|l| &l.command)
    }

    /// Number of commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// True without commands.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Ranges.
    #[must_use]
    pub const fn range(&self) -> &ProgramRange {
        &self.range
    }

    /// Estimated duration in milliseconds.
    #[must_use]
    pub const fn estimated_ms(&self) -> i64 {
        self.estimated_ms
    }

    /// Quadrant of the drawing.
    #[must_use]
    pub fn quadrant(&self) -> Quadrant {
        self.range.quadrant()
    }

    /// The program as text, one normalized command per line (`SaveGCODE` body).
    #[must_use]
    pub fn to_text(&self, newline: &str) -> String {
        let mut s = String::new();
        for l in &self.lines {
            s.push_str(l.command.text());
            s.push_str(newline);
        }
        s
    }
}

/// Splits on `\r\n`, `\n` or `\r` (`StreamReader.ReadLine`).
pub fn split_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        match rest.find(['\r', '\n']) {
            Some(i) => {
                let line = &rest[..i];
                let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[i + skip..];
                Some(line)
            }
            None => {
                let line = rest;
                rest = "";
                Some(line)
            }
        }
    })
}

#[cfg(feature = "parallel")]
fn parse_lines(lines: &[&str]) -> Vec<Command> {
    use rayon::prelude::*;
    // parsing is independent per line; the analysis that follows is sequential
    if lines.len() > 4096 {
        lines.par_iter().map(|l| Command::new(l)).collect()
    } else {
        lines.iter().map(|l| Command::new(l)).collect()
    }
}

#[cfg(not(feature = "parallel"))]
fn parse_lines(lines: &[&str]) -> Vec<Command> {
    lines.iter().map(|l| Command::new(l)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lines_handles_every_ending() {
        let v: Vec<&str> = split_lines("a\r\nb\rc\n\nd").collect();
        assert_eq!(v, vec!["a", "b", "c", "", "d"]);
        assert_eq!(split_lines("").count(), 0);
        assert_eq!(split_lines("x\n").collect::<Vec<_>>(), vec!["x"]);
    }

    #[test]
    fn blank_lines_are_skipped_and_comment_lines_kept() {
        let p = Program::parse("(c)\n\n  \nG0 X1\n", &AnalysisOptions::default());
        let texts: Vec<&str> = p.commands().map(Command::text).collect();
        assert_eq!(texts, vec!["", "G0 X1"]);
        assert_eq!(p.len(), 2);
        assert!(!p.is_empty());
        assert_eq!(p.to_text("\n"), "\nG0 X1\n");
        assert_eq!(p.lines()[1].offset_ms, p.estimated_ms());
    }

    #[test]
    fn negative_feed_gives_a_negative_offset() {
        // A negative feed gives a negative arc time and the cumulative offset
        // goes down; LaserGRBL does the same (conformance golden
        // `negative_feed.nc`: offsets_s [0.0, -376.991]).
        let p = Program::parse("F-1g1P\ni1", &AnalysisOptions::default());
        let offsets: Vec<i64> = p.lines().iter().map(|l| l.offset_ms).collect();
        assert_eq!(offsets, vec![0, -376_991]);
        assert_eq!(p.estimated_ms(), -376_991);
    }

    #[test]
    fn bom_and_invalid_utf8_are_tolerated() {
        let p = Program::parse_bytes(b"\xef\xbb\xbfG0 X1\n\xffG1 X2", &AnalysisOptions::default());
        assert_eq!(p.len(), 2);
        assert_eq!(p.commands().next().unwrap().text(), "G0 X1");
    }

    #[test]
    fn parallel_and_sequential_parsing_agree() {
        let text: String = (0..5000)
            .map(|i| format!("G1 X{} Y{} S{}\n", i % 50, i % 7, i % 255))
            .collect();
        let mut a = Program::new();
        a.append_text(&text);
        let mut b = Program::new();
        b.extend(split_lines(&text).map(Command::new));
        assert_eq!(a.commands().collect::<Vec<_>>(), b.commands().collect::<Vec<_>>());
    }
}
