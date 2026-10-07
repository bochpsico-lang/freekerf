//! Modal state and position tracking (`StateBuilder` / `StatePositionBuilder`).
//!
//! Feeding commands in order keeps the parser state (motion mode, distance mode,
//! spindle state...) and the tool position, and estimates how long each command
//! takes. The estimate uses only the configured maximum X rate and the feed, like
//! LaserGRBL (no acceleration).

use rust_decimal::Decimal;

use crate::arc::ArcHelper;
use crate::command::{Command, Word};
use crate::number::{decimal_from_f64, f64_from_decimal, millis_from};

/// Machine limits used by the time estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingConfig {
    /// `$110`, maximum X rate in mm/min (used for rapids and as the feed cap).
    pub max_rate_x_mm_min: Decimal,
}

impl Default for TimingConfig {
    /// LaserGRBL's default when no configuration was read: 4000 mm/min.
    fn default() -> Self {
        Self { max_rate_x_mm_min: Decimal::from(4000) }
    }
}

/// A modal group: one of a fixed set of words, the first one being the default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModalGroup {
    options: Vec<Word>,
    current: Word,
    settled: bool,
}

impl ModalGroup {
    fn new(options: &[&str]) -> Self {
        let options: Vec<Word> = options.iter().filter_map(|o| Word::parse(o)).collect();
        let current = options.first().copied().unwrap_or(Word::new('G', Decimal::ZERO));
        Self { options, current, settled: false }
    }

    fn update(&mut self, word: Option<Word>) {
        if let Some(w) = word
            && self.options.contains(&w) {
                self.current = w;
                self.settled = true;
            }
    }

    /// The current value of the group.
    #[must_use]
    pub const fn current(&self) -> Word {
        self.current
    }

    /// True once a command set this group explicitly.
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        self.settled
    }

    fn number_is(&self, n: i64) -> bool {
        self.current.value == Decimal::from(n)
    }
}

/// An axis position that is either absolute (plus the work offset) or relative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axis {
    letter: char,
    value: Decimal,
    previous: Decimal,
    settled: bool,
}

impl Axis {
    const fn new(letter: char) -> Self {
        Self { letter, value: Decimal::ZERO, previous: Decimal::ZERO, settled: false }
    }

    fn update(&mut self, value: Option<Decimal>, absolute: bool, offset: Decimal) {
        self.previous = self.value;
        if let Some(v) = value {
            let next = if absolute { offset.checked_add(v) } else { self.value.checked_add(v) };
            if let Some(n) = next {
                self.value = n;
            }
            self.settled = true;
        }
    }

    /// Current position.
    #[must_use]
    pub const fn value(&self) -> Decimal {
        self.value
    }

    /// Position before the last command.
    #[must_use]
    pub const fn previous(&self) -> Decimal {
        self.previous
    }

    /// The axis as a word (`X10.5`).
    #[must_use]
    pub const fn word(&self) -> Word {
        Word::new(self.letter, self.value)
    }
}

/// A value that keeps the last one seen (F, S).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastValue {
    letter: char,
    value: Decimal,
    settled: bool,
}

impl LastValue {
    const fn new(letter: char) -> Self {
        Self { letter, value: Decimal::ZERO, settled: false }
    }

    fn update(&mut self, value: Option<Decimal>) {
        if let Some(v) = value {
            self.value = v;
            self.settled = true;
        }
    }

    /// Current value.
    #[must_use]
    pub const fn value(&self) -> Decimal {
        self.value
    }

    /// True once a command set it.
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        self.settled
    }

    /// The value as a word (`S1000`).
    #[must_use]
    pub const fn word(&self) -> Word {
        Word::new(self.letter, self.value)
    }
}

/// Parser state and position after the commands fed so far.
#[derive(Debug, Clone)]
pub struct MachineState {
    /// Motion mode: G0, G1, G2, G3, G38.x, G80.
    pub motion_mode: ModalGroup,
    coordinate_select: ModalGroup,
    plane_select: ModalGroup,
    distance_mode: ModalGroup,
    arc_distance_mode: ModalGroup,
    feed_rate_mode: ModalGroup,
    // G20/G21 are never updated (LaserGRBL keeps millimetres, FINDINGS F-18)
    units_mode: ModalGroup,
    cutter_radius_compensation: ModalGroup,
    tool_length_offset: ModalGroup,
    program_mode: ModalGroup,
    coolant_state: ModalGroup,
    spindle_state: ModalGroup,

    x: Axis,
    y: Axis,
    z: Axis,
    wco: [Decimal; 3],
    f: LastValue,
    s: LastValue,
    support_pwm: bool,
    last_arc: Option<ArcHelper>,
}

impl Default for MachineState {
    fn default() -> Self {
        Self::new(true)
    }
}

impl MachineState {
    /// Initial state. `support_pwm` is the "Support Hardware PWM" setting: with PWM a
    /// laser at `S0` does not count as burning.
    #[must_use]
    pub fn new(support_pwm: bool) -> Self {
        Self {
            motion_mode: ModalGroup::new(&["G0", "G1", "G2", "G3", "G38.2", "G38.3", "G38.4", "G38.5", "G80"]),
            coordinate_select: ModalGroup::new(&["G54", "G55", "G56", "G57", "G58", "G59"]),
            plane_select: ModalGroup::new(&["G17", "G18", "G19"]),
            distance_mode: ModalGroup::new(&["G90", "G91"]),
            arc_distance_mode: ModalGroup::new(&["G91.1"]),
            feed_rate_mode: ModalGroup::new(&["G94", "G93"]),
            units_mode: ModalGroup::new(&["G21", "G20"]),
            cutter_radius_compensation: ModalGroup::new(&["G40"]),
            tool_length_offset: ModalGroup::new(&["G49", "G43.1"]),
            program_mode: ModalGroup::new(&["M0", "M1", "M2", "M30"]),
            coolant_state: ModalGroup::new(&["M9", "M7", "M8"]),
            spindle_state: ModalGroup::new(&["M5", "M3", "M4"]),
            x: Axis::new('X'),
            y: Axis::new('Y'),
            z: Axis::new('Z'),
            wco: [Decimal::ZERO; 3],
            f: LastValue::new('F'),
            s: LastValue::new('S'),
            support_pwm,
            last_arc: None,
        }
    }

    /// Feeds one command; returns its estimated duration in milliseconds when
    /// `timing` is given (`AnalyzeCommand`).
    pub fn analyze(&mut self, cmd: &Command, timing: Option<&TimingConfig>) -> i64 {
        self.update_modals(cmd);
        self.update_wco(cmd);
        self.update_xyz(cmd);
        self.update_fs(cmd);
        timing.map_or(0, |t| self.execution_time(cmd, t))
    }

    fn update_modals(&mut self, cmd: &Command) {
        if cmd.is_jog() {
            return;
        }
        let g = cmd.word('G');
        self.motion_mode.update(g);
        self.coordinate_select.update(g);
        self.plane_select.update(g);
        self.distance_mode.update(g);
        self.arc_distance_mode.update(g);
        self.feed_rate_mode.update(g);
        self.cutter_radius_compensation.update(g);
        self.tool_length_offset.update(g);
        let m = cmd.word('M');
        self.program_mode.update(m);
        self.coolant_state.update(m);
        self.spindle_state.update(m);
    }

    fn update_wco(&mut self, cmd: &Command) {
        if cmd.is_set_wco() {
            for (i, (axis, letter)) in [self.x, self.y, self.z].iter().zip(['X', 'Y', 'Z']).enumerate() {
                if let Some(v) = cmd.get(letter)
                    && let Some(w) = axis.value.checked_sub(v) {
                        self.wco[i] = w;
                    }
            }
        }
    }

    fn update_xyz(&mut self, cmd: &Command) {
        let absolute = if cmd.is_jog() {
            cmd.is_absolute_coord()
        } else if cmd.is_movement() {
            self.is_absolute()
        } else {
            return;
        };
        self.x.update(cmd.get('X'), absolute, self.wco[0]);
        self.y.update(cmd.get('Y'), absolute, self.wco[1]);
        self.z.update(cmd.get('Z'), absolute, self.wco[2]);
    }

    fn update_fs(&mut self, cmd: &Command) {
        if cmd.is_jog() {
            return;
        }
        self.f.update(cmd.get('F'));
        self.s.update(cmd.get('S'));
    }

    fn execution_time(&mut self, cmd: &Command, timing: &TimingConfig) -> i64 {
        let f = match cmd.get('F') {
            Some(f) if cmd.is_jog() => f,
            _ => self.f.value,
        };
        let max = timing.max_rate_x_mm_min;
        if self.motion_mode.number_is(0) && cmd.is_linear_movement() {
            let len = self.segment_length(cmd);
            millis_from(f64_from_decimal(len) / f64_from_decimal(max), 60_000.0).unwrap_or(0)
        } else if self.is_g1g2g3() && cmd.is_movement() && !f.is_zero() {
            let len = self.segment_length(cmd);
            let rate = f.min(max);
            millis_from(f64_from_decimal(len) / f64_from_decimal(rate), 60_000.0).unwrap_or(0)
        } else if cmd.is_pause() {
            cmd.get('P')
                .or_else(|| cmd.get('S'))
                .and_then(|v| millis_from(f64_from_decimal(v), 1000.0))
                .unwrap_or(0)
        } else {
            0
        }
    }

    fn segment_length(&mut self, cmd: &Command) -> Decimal {
        self.last_arc = None;
        if cmd.is_linear_movement() {
            linear_distance(self.x.previous, self.y.previous, self.x.value, self.y.value)
        } else if cmd.is_arc_movement() {
            let arc = ArcHelper::new(self, cmd);
            let len = decimal_from_f64(arc.abs_length()).unwrap_or(Decimal::ZERO);
            self.last_arc = Some(arc);
            len
        } else {
            Decimal::ZERO
        }
    }

    /// The arc of the last timed arc command. Like LaserGRBL it is only cleared by the
    /// next timed movement, so it may belong to an earlier command.
    #[must_use]
    pub const fn last_arc(&self) -> Option<&ArcHelper> {
        self.last_arc.as_ref()
    }

    /// Resets the position after homing (`$H`).
    pub fn homing(&mut self) {
        self.x = Axis::new('X');
        self.y = Axis::new('Y');
        self.z = Axis::new('Z');
    }

    /// True while the laser would be emitting (`LaserBurning`).
    #[must_use]
    pub fn laser_burning(&self) -> bool {
        (!self.support_pwm || self.s.value > Decimal::ZERO)
            && (self.spindle_state.number_is(3) || (self.spindle_state.number_is(4) && !self.motion_mode.number_is(0)))
    }

    /// True in G90.
    #[must_use]
    pub fn is_absolute(&self) -> bool {
        self.distance_mode.number_is(90)
    }

    /// True in G2 (clockwise arc).
    #[must_use]
    pub fn is_g2(&self) -> bool {
        self.motion_mode.number_is(2)
    }

    /// True in G2 or G3.
    #[must_use]
    pub fn is_g2g3(&self) -> bool {
        self.motion_mode.number_is(2) || self.motion_mode.number_is(3)
    }

    fn is_g1g2g3(&self) -> bool {
        self.motion_mode.number_is(1) || self.is_g2g3()
    }

    /// True when the last command moved in X/Y or was an arc (`TrueMovement`).
    #[must_use]
    pub fn true_movement(&self) -> bool {
        self.x.value != self.x.previous || self.y.value != self.y.previous || self.is_g2g3()
    }

    /// X axis.
    #[must_use]
    pub const fn x(&self) -> &Axis {
        &self.x
    }

    /// Y axis.
    #[must_use]
    pub const fn y(&self) -> &Axis {
        &self.y
    }

    /// Z axis.
    #[must_use]
    pub const fn z(&self) -> &Axis {
        &self.z
    }

    /// Feed (F).
    #[must_use]
    pub const fn f(&self) -> &LastValue {
        &self.f
    }

    /// Power (S).
    #[must_use]
    pub const fn s(&self) -> &LastValue {
        &self.s
    }

    /// Work coordinate offset set with G92 (X, Y, Z).
    #[must_use]
    pub const fn wco(&self) -> [Decimal; 3] {
        self.wco
    }

    /// True when a G92 offset is active.
    #[must_use]
    pub fn has_wco(&self) -> bool {
        self.wco.iter().any(|v| !v.is_zero())
    }

    /// Words of the modal groups that a command set explicitly, in the order Grbl
    /// lists them (motion mode excluded), used to rebuild the state on job resume.
    #[must_use]
    pub fn settled_modals(&self) -> Vec<Word> {
        [
            &self.coordinate_select,
            &self.plane_select,
            &self.distance_mode,
            &self.arc_distance_mode,
            &self.feed_rate_mode,
            &self.cutter_radius_compensation,
            &self.tool_length_offset,
            &self.program_mode,
            &self.coolant_state,
            &self.spindle_state,
        ]
        .into_iter()
        .filter(|g| g.is_settled())
        .map(ModalGroup::current)
        .collect()
    }

    /// The units group (always the G21 default: inch programs are analyzed as mm).
    #[must_use]
    pub const fn units_mode(&self) -> &ModalGroup {
        &self.units_mode
    }
}

/// `MathHelper.LinearDistance`: computed in `double`, returned as a decimal.
#[must_use]
pub fn linear_distance(x0: Decimal, y0: Decimal, x1: Decimal, y1: Decimal) -> Decimal {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let sq = dx.checked_mul(dx).and_then(|a| dy.checked_mul(dy).and_then(|b| a.checked_add(b)));
    sq.and_then(|v| decimal_from_f64(f64_from_decimal(v).sqrt())).unwrap_or(Decimal::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lines: &[&str]) -> (MachineState, Vec<i64>) {
        let mut st = MachineState::default();
        let timing = TimingConfig { max_rate_x_mm_min: Decimal::from(6000) };
        let times = lines.iter().map(|l| st.analyze(&Command::new(l), Some(&timing))).collect();
        (st, times)
    }

    #[test]
    fn rapid_uses_max_rate_and_feed_is_capped() {
        let (_, t) = run(&["G0 X60", "G1 X0 F600", "G1 X60 F60000", "G1 F0", "G1 X0"]);
        assert_eq!(t, vec![600, 6000, 600, 0, 0]);
    }

    #[test]
    fn dwell_uses_p_then_s_in_seconds() {
        let (_, t) = run(&["G4 P0.5", "G4 S1.25", "G4"]);
        assert_eq!(t, vec![500, 1250, 0]);
    }

    #[test]
    fn relative_moves_and_offsets() {
        let (st, _) = run(&["G0 X50 Y50", "G92 X0 Y0", "G1 X10 F1000", "G91", "G1 X5 Y-5"]);
        assert_eq!(st.x().value(), Decimal::from(65));
        assert_eq!(st.y().value(), Decimal::from(45));
        assert!(st.has_wco());
        assert_eq!(st.wco()[0], Decimal::from(50));
        assert!(!st.is_absolute());
    }

    #[test]
    fn laser_burning_rules() {
        let (st, _) = run(&["M3 S0"]);
        assert!(!st.laser_burning());
        let (st, _) = run(&["M3 S10"]);
        assert!(st.laser_burning());
        let (st, _) = run(&["M4 S10", "G0 X1"]);
        assert!(!st.laser_burning());
        let (st, _) = run(&["M4 S10", "G1 X1"]);
        assert!(st.laser_burning());
        let mut no_pwm = MachineState::new(false);
        no_pwm.analyze(&Command::new("M3"), None);
        assert!(no_pwm.laser_burning());
    }

    #[test]
    fn settled_modals_skip_motion_and_defaults_not_set() {
        let (st, _) = run(&["G1 X1", "G91", "M3 S5", "G20"]);
        let words: Vec<String> = st.settled_modals().iter().map(ToString::to_string).collect();
        assert_eq!(words, vec!["G91", "M3"]);
        assert!(!st.units_mode().is_settled());
        assert!(st.motion_mode.is_settled());
    }

    #[test]
    fn jog_commands_move_without_changing_modes() {
        let mut st = MachineState::default();
        let t = TimingConfig::default();
        st.analyze(&Command::jog("$J=G91X10F1000"), Some(&t));
        assert_eq!(st.x().value(), Decimal::from(10));
        assert_eq!(st.f().value(), Decimal::ZERO);
        st.analyze(&Command::jog("$J=G90X2F1000"), Some(&t));
        assert_eq!(st.x().value(), Decimal::from(2));
        st.homing();
        assert_eq!(st.x().value(), Decimal::ZERO);
    }

    #[test]
    fn true_movement_and_words() {
        let (st, _) = run(&["G1 X1 F100"]);
        assert!(st.true_movement());
        assert_eq!(st.x().word().to_string(), "X1");
        assert_eq!(st.f().word().to_string(), "F100");
        assert!(st.f().is_settled());
        assert!(!st.s().is_settled());
        assert_eq!(st.z().previous(), Decimal::ZERO);
        assert_eq!(linear_distance(Decimal::ZERO, Decimal::ZERO, Decimal::from(3), Decimal::from(4)), Decimal::from(5));
    }
}
