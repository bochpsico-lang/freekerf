//! Offline analysis of a G-code file: the report of `freekerf analyze --json`.
//!
//! The `stream` (what a device receives for a full job with the default header
//! and footer) is produced by the real protocol host talking to the emulator in
//! memory, with a simulated clock: no threads, no I/O, deterministic.

use freekerf_emulator::{Device, DeviceConfig};
use freekerf_gcode::report::Report;
use freekerf_gcode::{AnalysisOptions, Decimal, Program, TimingConfig};
use freekerf_protocol::{FirmwareKind, Host, HostOptions, Output};

/// The maximum X rate `conformance`'s `golden.py` configures ($110 = 6000 mm/min).
pub const GOLDEN_MAX_RATE: i64 = 6000;

/// Options of [`analyze`].
#[derive(Debug, Clone)]
pub struct AnalyzeOptions {
    /// `$110`, mm/min.
    pub max_rate_x_mm_min: Decimal,
    /// Hardware PWM (S0 does not burn).
    pub support_pwm: bool,
    /// Host options for the simulated job (header, footer...).
    pub host: HostOptions,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self { max_rate_x_mm_min: Decimal::from(GOLDEN_MAX_RATE), support_pwm: true, host: HostOptions::default() }
    }
}

/// Analyzes G-code bytes and simulates a job run to fill `stream`.
#[must_use]
pub fn analyze(name: &str, bytes: &[u8], options: &AnalyzeOptions) -> Report {
    let analysis = AnalysisOptions {
        timing: TimingConfig { max_rate_x_mm_min: options.max_rate_x_mm_min },
        support_pwm: options.support_pwm,
    };
    let program = Program::parse_bytes(bytes, &analysis);
    let mut report = Report::new(name, &program);
    report.stream = simulate_stream(program, &options.host);
    report
}

/// Lines a Grbl 1.1 device receives when `program` runs as a job.
#[must_use]
pub fn simulate_stream(program: Program, options: &HostOptions) -> Vec<String> {
    let mut sim = Sim::new(options.clone());
    sim.connect();
    sim.device.clear_lines();
    if program.is_empty() || sim.host.set_program(program).is_err() {
        return Vec::new();
    }
    if sim.host.run_program(false, sim.now).is_err() {
        return Vec::new();
    }
    sim.run_while(|h| h.in_program() || !h.ready());
    sim.device.lines().to_vec()
}

struct Sim {
    host: Host,
    device: Device,
    now: u64,
}

impl Sim {
    fn new(options: HostOptions) -> Self {
        Self { host: Host::for_kind(FirmwareKind::Grbl, options), device: Device::new(DeviceConfig::default()), now: 0 }
    }

    fn exchange(&mut self) {
        for _ in 0..100_000 {
            let mut moved = false;
            while let Some(o) = self.host.poll_output() {
                if let Output::Write(b) = o {
                    self.device.receive(&b, self.now);
                    moved = true;
                }
            }
            let out = self.device.take_output();
            if !out.is_empty() {
                self.host.receive(&out, self.now);
                moved = true;
            }
            while self.host.poll_event().is_some() {}
            if !moved {
                return;
            }
        }
    }

    fn connect(&mut self) {
        self.host.open(self.now);
        self.device.connected();
        self.exchange();
        self.run_while(|h| !h.ready());
    }

    /// Advances the simulated clock (bounded: one simulated hour).
    fn run_while(&mut self, cond: impl Fn(&Host) -> bool) {
        let end = self.now + 3_600_000;
        self.host.tick(self.now);
        self.exchange();
        while cond(&self.host) && self.now < end {
            self.now = self.host.next_deadline().map_or(self.now + 10, |d| d.max(self.now + 1));
            self.device.tick(self.now);
            self.host.tick(self.now);
            self.exchange();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_of_a_small_program() {
        let r = analyze("t.nc", b"; c\nG1 X1 F100\n", &AnalyzeOptions::default());
        assert_eq!(r.stream, vec!["G90", "", "G1X1F100", "G0X0Y0Z0"]);
        assert_eq!(r.input, "t.nc");
        let empty = analyze("e.nc", b"\n\n", &AnalyzeOptions::default());
        assert!(empty.stream.is_empty());
    }
}
