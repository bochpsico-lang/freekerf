//! A Grbl 1.1 device, sans-IO: bytes in, bytes out, time given by the caller.
//!
//! Used as the "Emulator" connection of the app (simulator mode) and as the fake
//! board of the tests. The model follows Grbl 1.1 and `conformance`'s `FakeGrbl`:
//!
//! * real-time bytes (`?`, `!`, `~`, ctrl-x and every byte ≥ 0x80) act at once and
//!   never enter the RX buffer;
//! * other bytes enter an RX buffer of [`DeviceConfig::rx_size`] bytes. `rx_used`
//!   counts bytes received and not yet acknowledged; bytes that do not fit are
//!   counted in `overflows` (Grbl drops them);
//! * a complete line is executed (removed from the buffer, answered `ok`/`error:N`)
//!   at once, or only when [`Device::release`] is called if `auto_ack` is off; never
//!   during a feed hold;
//! * with [`DeviceConfig::timed`] an executed motion takes its estimated time in a
//!   planner of 15 blocks, and the state stays `Run` until it is done.
//!
//! ```
//! use freekerf_emulator::{Device, DeviceConfig};
//!
//! let mut dev = Device::new(DeviceConfig::default());
//! dev.receive(b"G1 X10 F600\n", 0);
//! assert_eq!(dev.lines(), ["G1 X10 F600"]);
//! assert_eq!(String::from_utf8(dev.take_output()).unwrap(), "ok\r\n");
//! ```

use std::collections::{BTreeMap, VecDeque};

use freekerf_gcode::number::f64_from_decimal;
use freekerf_gcode::{Command, MachineState, TimingConfig};
use regex::Regex;

/// Grbl 1.1 real-time: `?`.
pub const RT_STATUS: u8 = 0x3F;
/// `!`.
pub const RT_HOLD: u8 = 0x21;
/// `~`.
pub const RT_RESUME: u8 = 0x7E;
/// ctrl-x.
pub const RT_RESET: u8 = 0x18;
/// Safety door.
pub const RT_DOOR: u8 = 0x84;
/// Jog cancel.
pub const RT_JOG_CANCEL: u8 = 0x85;

const PLANNER_BLOCKS: usize = 15;

/// Default `$$` of the emulator (a typical 400×300 laser with laser mode on).
#[must_use]
pub fn default_settings() -> BTreeMap<u32, String> {
    [
        (0, "10"),
        (1, "25"),
        (2, "0"),
        (3, "0"),
        (4, "0"),
        (5, "0"),
        (6, "0"),
        (10, "1"),
        (11, "0.010"),
        (12, "0.002"),
        (13, "0"),
        (20, "0"),
        (21, "0"),
        (22, "0"),
        (23, "0"),
        (24, "25.000"),
        (25, "500.000"),
        (26, "250"),
        (27, "1.000"),
        (30, "1000"),
        (31, "0"),
        (32, "1"),
        (100, "80.000"),
        (101, "80.000"),
        (102, "250.000"),
        (110, "6000.000"),
        (111, "6000.000"),
        (112, "500.000"),
        (120, "500.000"),
        (121, "500.000"),
        (122, "10.000"),
        (130, "400.000"),
        (131, "300.000"),
        (132, "200.000"),
    ]
    .into_iter()
    .map(|(k, v)| (k, v.to_owned()))
    .collect()
}

/// How the device behaves.
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// Version in the banner and `$I`.
    pub version: String,
    /// Banner; default `Grbl {version} ['$' for help]`.
    pub welcome: Option<String>,
    /// RX buffer size (Grbl 1.1: 128).
    pub rx_size: usize,
    /// Execute lines as soon as they are complete.
    pub auto_ack: bool,
    /// `$$` values.
    pub settings: BTreeMap<u32, String>,
    /// `$I` options line.
    pub opt_line: String,
    /// `$I` version line (default `[VER:{version}.20170801:]`).
    pub ver_line: Option<String>,
    /// Report `Bf:` in status reports.
    pub report_buffer: bool,
    /// Report `Ov:` in status reports.
    pub report_overrides: bool,
    /// Lines matching a pattern are answered `error:code`.
    pub error_rules: Vec<(Regex, u32)>,
    /// Print the banner when the port opens.
    pub send_welcome_on_open: bool,
    /// Answer `ok` before the data lines of `$$`/`$I` (some clones).
    pub ok_first: bool,
    /// Motions take their estimated time (simulator); off: instant (tests).
    pub timed: bool,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            version: "1.1f".into(),
            welcome: None,
            rx_size: 128,
            auto_ack: true,
            settings: default_settings(),
            opt_line: "[OPT:V,15,128]".into(),
            ver_line: None,
            report_buffer: true,
            report_overrides: true,
            error_rules: Vec::new(),
            send_welcome_on_open: true,
            ok_first: false,
            timed: false,
        }
    }
}

/// The simulated board.
#[derive(Debug)]
pub struct Device {
    cfg: DeviceConfig,
    out: Vec<u8>,
    lines: Vec<String>,
    realtime: Vec<u8>,
    responses: Vec<String>,
    partial: Vec<u8>,
    rxq: VecDeque<Vec<u8>>,
    rx_used: usize,
    max_rx_used: usize,
    overflows: usize,
    state: &'static str,
    hold: bool,
    check_mode: bool,
    alarm: Option<u32>,
    gstate: MachineState,
    planner: VecDeque<u64>,
    planner_free_at: u64,
    jogging: bool,
    feed: f32,
    spindle: f32,
    ov: [i32; 3],
    resets: usize,
    now: u64,
}

impl Device {
    /// A device in `Idle`.
    #[must_use]
    pub fn new(cfg: DeviceConfig) -> Self {
        Self {
            cfg,
            out: Vec::new(),
            lines: Vec::new(),
            realtime: Vec::new(),
            responses: Vec::new(),
            partial: Vec::new(),
            rxq: VecDeque::new(),
            rx_used: 0,
            max_rx_used: 0,
            overflows: 0,
            state: "Idle",
            hold: false,
            check_mode: false,
            alarm: None,
            gstate: MachineState::default(),
            planner: VecDeque::new(),
            planner_free_at: 0,
            jogging: false,
            feed: 0.0,
            spindle: 0.0,
            ov: [100; 3],
            resets: 0,
            now: 0,
        }
    }

    // ------------------------------------------------------------------ wiring

    /// The host opened the port (prints the banner unless disabled).
    pub fn connected(&mut self) {
        if self.cfg.send_welcome_on_open {
            self.announce();
        }
    }

    /// Prints the banner (as after a DTR reset).
    pub fn announce(&mut self) {
        let w = self.welcome_line();
        self.emit(&w);
    }

    fn welcome_line(&self) -> String {
        self.cfg.welcome.clone().unwrap_or_else(|| format!("Grbl {} ['$' for help]", self.cfg.version))
    }

    fn emit(&mut self, text: &str) {
        self.responses.push(text.to_owned());
        self.out.extend_from_slice(text.as_bytes());
        self.out.extend_from_slice(b"\r\n");
    }

    /// Bytes to send to the host (drained).
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// Sends an unsolicited line (e.g. `[MSG:...]`).
    pub fn push(&mut self, line: &str) {
        self.emit(line);
    }

    // ------------------------------------------------------------------ input

    /// Bytes from the host.
    pub fn receive(&mut self, data: &[u8], now: u64) {
        self.advance(now);
        for &b in data {
            if is_realtime(b) {
                self.handle_realtime(b);
                continue;
            }
            if self.rx_used >= self.cfg.rx_size {
                self.overflows += 1;
            }
            self.rx_used += 1;
            self.max_rx_used = self.max_rx_used.max(self.rx_used);
            self.partial.push(b);
            if b == b'\n' {
                let line = std::mem::take(&mut self.partial);
                let text = String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']).to_owned();
                self.lines.push(text);
                self.rxq.push_back(line);
            }
        }
        self.pump();
    }

    /// Advances the simulated time (timed mode).
    pub fn tick(&mut self, now: u64) {
        self.advance(now);
        self.pump();
    }

    /// When [`Device::tick`] should run next (timed mode with work queued).
    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        self.planner.front().copied()
    }

    fn advance(&mut self, now: u64) {
        self.now = self.now.max(now);
        while self.planner.front().is_some_and(|t| *t <= self.now) && !self.hold {
            self.planner.pop_front();
        }
        if self.planner.is_empty() {
            if self.state == "Run" || self.state == "Jog" {
                self.state = "Idle";
            }
            self.jogging = false;
        }
    }

    fn pump(&mut self) {
        while self.cfg.auto_ack && !self.rxq.is_empty() && !self.hold && self.planner.len() < PLANNER_BLOCKS {
            self.execute_next();
        }
    }

    /// Executes up to `n` buffered lines (manual-ack mode); returns how many ran.
    pub fn release(&mut self, n: usize) -> usize {
        let mut done = 0;
        while done < n && !self.rxq.is_empty() && !self.hold {
            self.execute_next();
            done += 1;
        }
        done
    }

    /// Turns automatic execution on or off.
    pub fn set_auto_ack(&mut self, value: bool) {
        self.cfg.auto_ack = value;
        self.pump();
    }

    /// Raises an alarm: buffer flushed, `ALARM:code` printed.
    pub fn alarm_now(&mut self, code: u32) {
        self.alarm = Some(code);
        self.state = "Alarm";
        self.rxq.clear();
        self.planner.clear();
        self.rx_used = 0;
        self.emit(&format!("ALARM:{code}"));
    }

    // ------------------------------------------------------------------ real-time

    fn handle_realtime(&mut self, b: u8) {
        self.realtime.push(b);
        match b {
            RT_STATUS => {
                let r = self.status_report();
                self.emit(&r);
            }
            RT_HOLD => {
                if matches!(self.state, "Run" | "Idle" | "Jog") {
                    self.hold = true;
                    self.state = "Hold";
                }
            }
            RT_RESUME => {
                if self.hold {
                    self.hold = false;
                    self.state = if self.rxq.is_empty() && self.planner.is_empty() { "Idle" } else { "Run" };
                    // simplified: the planner clock keeps running during a hold
                    self.pump();
                }
            }
            RT_RESET => self.reset(),
            RT_DOOR => {
                self.hold = true;
                self.state = "Door";
            }
            RT_JOG_CANCEL => {
                if self.state == "Jog" {
                    self.state = "Idle";
                    self.planner.clear();
                    self.jogging = false;
                }
            }
            0x90..=0x9D => self.override_byte(b),
            _ => {}
        }
    }

    fn override_byte(&mut self, b: u8) {
        let [mut f, mut r, mut s] = self.ov;
        match b {
            0x90 => f = 100,
            0x91 => f += 10,
            0x92 => f -= 10,
            0x93 => f += 1,
            0x94 => f -= 1,
            0x95 => r = 100,
            0x96 => r = 50,
            0x97 => r = 25,
            0x99 => s = 100,
            0x9A => s += 10,
            0x9B => s -= 10,
            0x9C => s += 1,
            0x9D => s -= 1,
            _ => {}
        }
        self.ov = [f.clamp(10, 200), r, s.clamp(10, 200)];
    }

    fn reset(&mut self) {
        self.resets += 1;
        self.rxq.clear();
        self.partial.clear();
        self.planner.clear();
        self.rx_used = 0;
        self.hold = false;
        self.check_mode = false;
        self.jogging = false;
        self.ov = [100; 3];
        self.gstate = MachineState::default();
        self.state = if self.alarm.is_some() { "Alarm" } else { "Idle" };
        self.emit("");
        let w = self.welcome_line();
        self.emit(&w);
    }

    /// The status report Grbl would print now.
    #[must_use]
    pub fn status_report(&self) -> String {
        let mut st = self.state.to_owned();
        if st == "Idle" && !self.rxq.is_empty() {
            st = "Run".into(); // lines buffered and not executed yet
        }
        if st == "Hold" {
            st = "Hold:0".into();
        }
        let mpos = self.mpos();
        let mut parts = vec![st, format!("MPos:{:.3},{:.3},{:.3}", mpos[0], mpos[1], mpos[2])];
        if self.cfg.report_buffer {
            let free = self.cfg.rx_size.saturating_sub(self.rx_used);
            let blocks = PLANNER_BLOCKS.saturating_sub(self.rxq.len() + self.planner.len());
            parts.push(format!("Bf:{blocks},{free}"));
        }
        parts.push(format!("FS:{},{}", self.feed, self.spindle));
        let wco = self.gstate.wco().map(f64_from_decimal);
        if wco.iter().any(|v| *v != 0.0) {
            parts.push(format!("WCO:{:.3},{:.3},{:.3}", wco[0], wco[1], wco[2]));
        }
        if self.cfg.report_overrides {
            parts.push(format!("Ov:{},{},{}", self.ov[0], self.ov[1], self.ov[2]));
        }
        format!("<{}>", parts.join("|"))
    }

    fn mpos(&self) -> [f64; 3] {
        [self.gstate.x().value(), self.gstate.y().value(), self.gstate.z().value()].map(f64_from_decimal)
    }

    // ------------------------------------------------------------------ execution

    fn execute_next(&mut self) {
        let Some(line) = self.rxq.pop_front() else { return };
        self.rx_used = self.rx_used.saturating_sub(line.len());
        let text = String::from_utf8_lossy(&line).trim().to_owned();
        let replies = self.execute(&text);
        for r in replies {
            self.emit(&r);
        }
    }

    fn data_reply(&self, data: Vec<String>) -> Vec<String> {
        let mut v = data;
        if self.cfg.ok_first {
            v.insert(0, "ok".into());
        } else {
            v.push("ok".into());
        }
        v
    }

    fn execute(&mut self, text: &str) -> Vec<String> {
        for (rx, code) in &self.cfg.error_rules {
            if rx.is_match(text) {
                return vec![format!("error:{code}")];
            }
        }
        if text.is_empty() {
            return vec!["ok".into()];
        }
        let up = text.to_uppercase();
        match up.as_str() {
            "$$" => {
                let data = self.cfg.settings.iter().map(|(k, v)| format!("${k}={v}")).collect();
                return self.data_reply(data);
            }
            "$I" => {
                let ver = self.cfg.ver_line.clone().unwrap_or_else(|| format!("[VER:{}.20170801:]", self.cfg.version));
                let data = vec![ver, self.cfg.opt_line.clone()];
                return self.data_reply(data);
            }
            "$X" => {
                self.alarm = None;
                self.state = "Idle";
                return vec!["[MSG:Caution: Unlocked]".into(), "ok".into()];
            }
            "$H" => {
                if self.cfg.settings.get(&22).is_none_or(|v| v.trim() == "0") {
                    return vec!["error:5".into()];
                }
                self.alarm = None;
                self.gstate.homing();
                self.state = "Idle";
                return vec!["ok".into()];
            }
            "$C" => {
                self.check_mode = !self.check_mode;
                self.state = if self.check_mode { "Check" } else { "Idle" };
                return vec![(if self.check_mode { "[MSG:Enabled]" } else { "[MSG:Disabled]" }).into(), "ok".into()];
            }
            "$G" => return vec!["[GC:G0 G54 G17 G21 G90 G94 M5 M9 T0 F0 S0]".into(), "ok".into()],
            _ => {}
        }
        if let Some((n, value)) = freekerf_gcode::command::parse_settings_line(text) {
            if let std::collections::btree_map::Entry::Occupied(mut e) = self.cfg.settings.entry(n) {
                e.insert(value.to_owned());
                return vec!["ok".into()];
            }
            return vec!["error:3".into()];
        }
        if up.starts_with("$J=") {
            if self.alarm.is_some() {
                return vec!["error:8".into()];
            }
            self.motion(&Command::jog(&up), true);
            return vec!["ok".into()];
        }
        if up.starts_with('$') {
            return vec!["error:3".into()];
        }
        if self.alarm.is_some() {
            return vec!["error:9".into()];
        }
        if !self.check_mode {
            let cmd = Command::new(text);
            if cmd.has_parse_error() {
                return vec!["error:2".into()];
            }
            self.motion(&cmd, false);
        }
        vec!["ok".into()]
    }

    fn motion(&mut self, cmd: &Command, jog: bool) {
        let timing = TimingConfig {
            max_rate_x_mm_min: self
                .cfg
                .settings
                .get(&110)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or_else(|| TimingConfig::default().max_rate_x_mm_min),
        };
        let ms = self.gstate.analyze(cmd, Some(&timing)).max(0);
        if let Some(f) = cmd.get('F') {
            self.feed = f64_from_decimal(f) as f32;
        }
        if let Some(s) = cmd.get('S') {
            self.spindle = f64_from_decimal(s) as f32;
        }
        if self.cfg.timed && ms > 0 {
            let start = self.planner_free_at.max(self.now);
            #[allow(clippy::cast_sign_loss)] // clamped to >= 0 above
            let end = start + ms as u64;
            self.planner_free_at = end;
            self.planner.push_back(end);
            self.state = if jog { "Jog" } else { "Run" };
            self.jogging = jog;
        }
    }

    // ------------------------------------------------------------------ observation (tests)

    /// Complete lines received, without the newline.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Clears the record of lines (tests).
    pub fn clear_lines(&mut self) {
        self.lines.clear();
    }

    /// Real-time bytes received.
    #[must_use]
    pub fn realtime(&self) -> &[u8] {
        &self.realtime
    }

    /// Clears the record of real-time bytes (tests).
    pub fn clear_realtime(&mut self) {
        self.realtime.clear();
    }

    /// Lines printed.
    #[must_use]
    pub fn responses(&self) -> &[String] {
        &self.responses
    }

    /// Bytes in the RX buffer.
    #[must_use]
    pub const fn rx_used(&self) -> usize {
        self.rx_used
    }

    /// Highest RX buffer use seen.
    #[must_use]
    pub const fn max_rx_used(&self) -> usize {
        self.max_rx_used
    }

    /// Resets the high-water mark (tests).
    pub fn reset_max_rx_used(&mut self) {
        self.max_rx_used = 0;
    }

    /// Bytes that did not fit.
    #[must_use]
    pub const fn overflows(&self) -> usize {
        self.overflows
    }

    /// Complete lines waiting to be executed.
    #[must_use]
    pub fn pending_lines(&self) -> usize {
        self.rxq.len()
    }

    /// Feed hold active.
    #[must_use]
    pub const fn hold(&self) -> bool {
        self.hold
    }

    /// Current overrides (feed, rapids, spindle).
    #[must_use]
    pub const fn overrides(&self) -> [i32; 3] {
        self.ov
    }

    /// Soft resets received.
    #[must_use]
    pub const fn resets(&self) -> usize {
        self.resets
    }

    /// Current `$$` values.
    #[must_use]
    pub const fn settings(&self) -> &BTreeMap<u32, String> {
        &self.cfg.settings
    }

    /// State name.
    #[must_use]
    pub const fn state(&self) -> &'static str {
        self.state
    }

    /// Machine position.
    #[must_use]
    pub fn position(&self) -> [f64; 3] {
        self.mpos()
    }

    /// Mutable configuration (error rules and so on).
    pub fn config_mut(&mut self) -> &mut DeviceConfig {
        &mut self.cfg
    }
}

/// Real-time bytes of Grbl 1.1.
#[must_use]
pub const fn is_realtime(b: u8) -> bool {
    matches!(b, RT_STATUS | RT_HOLD | RT_RESUME | RT_RESET) || b >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(d: &mut Device) -> String {
        String::from_utf8(d.take_output()).unwrap()
    }

    #[test]
    fn banner_status_and_settings() {
        let mut d = Device::new(DeviceConfig::default());
        d.connected();
        assert_eq!(out(&mut d), "Grbl 1.1f ['$' for help]\r\n");
        d.receive(b"?", 0);
        assert_eq!(out(&mut d), "<Idle|MPos:0.000,0.000,0.000|Bf:15,128|FS:0,0|Ov:100,100,100>\r\n");
        d.receive(b"$$\n", 0);
        let o = out(&mut d);
        assert!(o.starts_with("$0=10\r\n") && o.ends_with("$132=200.000\r\nok\r\n"));
        d.receive(b"$I\n", 0);
        assert_eq!(out(&mut d), "[VER:1.1f.20170801:]\r\n[OPT:V,15,128]\r\nok\r\n");
        d.receive(b"$110=4000\n$999=1\n", 0);
        assert_eq!(out(&mut d), "ok\r\nerror:3\r\n");
        assert_eq!(d.settings()[&110], "4000");
    }

    #[test]
    fn manual_ack_and_buffer_accounting() {
        let mut d = Device::new(DeviceConfig { auto_ack: false, rx_size: 10, ..DeviceConfig::default() });
        d.receive(b"G1X1\nG1X2\nG1X3\n", 0);
        assert_eq!(d.rx_used(), 15);
        assert_eq!(d.overflows(), 5);
        assert_eq!(d.pending_lines(), 3);
        assert!(d.status_report().starts_with("<Run|"));
        assert_eq!(d.release(2), 2);
        assert_eq!(d.rx_used(), 5);
        assert_eq!(out(&mut d), "ok\r\nok\r\n");
        d.set_auto_ack(true);
        assert_eq!(d.pending_lines(), 0);
        assert_eq!(d.position()[0], 3.0);
        assert_eq!(d.max_rx_used(), 15);
    }

    #[test]
    fn hold_resume_reset_and_alarm() {
        let mut d = Device::new(DeviceConfig::default());
        d.receive(b"!", 0);
        assert!(d.hold());
        d.receive(b"G1X1\n", 0);
        assert_eq!(d.pending_lines(), 1);
        assert!(d.status_report().starts_with("<Hold:0|"));
        d.receive(b"~", 0);
        assert_eq!(d.pending_lines(), 0);
        d.alarm_now(1);
        assert_eq!(d.state(), "Alarm");
        let _ = out(&mut d);
        d.receive(b"G1X2\n$J=G91X1F100\n", 0);
        assert_eq!(out(&mut d), "error:9\r\nerror:8\r\n");
        d.receive(&[RT_RESET], 0);
        assert_eq!(d.resets(), 1);
        assert_eq!(d.state(), "Alarm");
        assert_eq!(out(&mut d), "\r\nGrbl 1.1f ['$' for help]\r\n");
        d.receive(b"$X\n", 0);
        assert_eq!(out(&mut d), "[MSG:Caution: Unlocked]\r\nok\r\n");
        assert_eq!(d.state(), "Idle");
        d.receive(&[RT_DOOR], 0);
        assert_eq!(d.state(), "Door");
    }

    #[test]
    fn overrides_and_system_commands() {
        let mut d = Device::new(DeviceConfig::default());
        d.receive(&[0x91, 0x91, 0x97, 0x9B, 0x9D], 0);
        assert_eq!(d.overrides(), [120, 25, 89]);
        d.receive(&[0x90, 0x95, 0x99, 0x92, 0x94, 0x96, 0x9A, 0x9C, 0x93], 0);
        assert_eq!(d.overrides(), [90, 50, 111]);
        d.receive(b"$H\n", 0);
        assert_eq!(out(&mut d), "error:5\r\n");
        d.receive(b"$22=1\n$H\n$C\nG1X5\n$C\n$G\n$Q\nG1 X1..5\n", 0);
        let o = out(&mut d);
        assert_eq!(
            o,
            "ok\r\nok\r\n[MSG:Enabled]\r\nok\r\nok\r\n[MSG:Disabled]\r\nok\r\n[GC:G0 G54 G17 G21 G90 G94 M5 M9 T0 F0 S0]\r\nok\r\nerror:3\r\nerror:2\r\n"
        );
        assert_eq!(d.position()[0], 0.0); // check mode did not move
        assert_eq!(d.realtime().len(), 14);
        d.clear_realtime();
        d.clear_lines();
        assert!(d.realtime().is_empty() && d.lines().is_empty());
    }

    #[test]
    fn timed_execution_and_jog_cancel() {
        let mut d = Device::new(DeviceConfig { timed: true, ..DeviceConfig::default() });
        d.receive(b"G1 X10 F600\n", 0); // 1 s
        assert_eq!(d.state(), "Run");
        assert_eq!(d.next_deadline(), Some(1000));
        d.tick(999);
        assert_eq!(d.state(), "Run");
        d.tick(1000);
        assert_eq!(d.state(), "Idle");
        d.receive(b"$J=G91X10F600\n", 2000);
        assert_eq!(d.state(), "Jog");
        d.receive(&[RT_JOG_CANCEL], 2100);
        assert_eq!(d.state(), "Idle");
        // a real-time byte acts before the line received with it is executed
        d.receive(b"G92X0\n?", 2200);
        let r = d.responses();
        assert!(r[r.len() - 2].starts_with("<Run|MPos:20.000"));
        assert_eq!(r[r.len() - 1], "ok");
        d.receive(b"?", 2300);
        assert!(d.status_report().contains("WCO:20.000,0.000,0.000"));
        assert!(d.responses().iter().any(|r| r.starts_with("<Idle|MPos:20.000")));
    }

    #[test]
    fn error_rules_ok_first_and_custom_banner() {
        let cfg = DeviceConfig {
            error_rules: vec![(Regex::new("X5$").unwrap(), 33)],
            ok_first: true,
            welcome: Some("Grbl 1.1h ['$' for help]".into()),
            send_welcome_on_open: false,
            report_buffer: false,
            report_overrides: false,
            ..DeviceConfig::default()
        };
        let mut d = Device::new(cfg);
        d.connected();
        assert_eq!(out(&mut d), "");
        d.announce();
        assert_eq!(out(&mut d), "Grbl 1.1h ['$' for help]\r\n");
        d.receive(b"G1X5\n\n$I\n", 0);
        assert_eq!(out(&mut d), "error:33\r\nok\r\nok\r\n[VER:1.1f.20170801:]\r\n[OPT:V,15,128]\r\n");
        assert_eq!(d.status_report(), "<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        d.push("[MSG:hi]");
        assert_eq!(out(&mut d), "[MSG:hi]\r\n");
        d.config_mut().error_rules.clear();
        d.reset_max_rx_used();
        assert_eq!(d.max_rx_used(), 0);
        assert!(is_realtime(0x85) && !is_realtime(b'G'));
    }
}
