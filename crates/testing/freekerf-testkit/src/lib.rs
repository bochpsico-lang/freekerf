//! Test support for FreeKerf (dev-dependency only, never shipped).
//!
//! * [`SimClock`]: a manual clock implementing [`freekerf_protocol::Clock`].
//! * [`Bench`]: a [`Host`] wired to an emulated [`Device`] in memory, with a
//!   simulated clock: tests drive both state machines step by step, no threads.
//! * [`pty`]: a pseudo terminal pair, to test serial code without hardware.
//! * [`fixtures`]: paths of shared test files.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use freekerf_emulator::{Device, DeviceConfig};
use freekerf_protocol::{Clock, Event, Host, HostOptions, Output};

pub mod fixtures;
#[cfg(unix)]
pub mod pty;

/// A clock that only moves when told to.
#[derive(Debug, Clone, Default)]
pub struct SimClock(Arc<AtomicU64>);

impl SimClock {
    /// A clock at `start` ms.
    #[must_use]
    pub fn new(start: u64) -> Self {
        Self(Arc::new(AtomicU64::new(start)))
    }

    /// Moves forward by `ms`.
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }

    /// Sets the time.
    pub fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl Clock for SimClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// A host and an emulated device connected in memory.
#[derive(Debug)]
pub struct Bench {
    /// The host under test.
    pub host: Host,
    /// The device.
    pub device: Device,
    /// Simulated time, ms.
    pub now: u64,
    /// Every event the host produced.
    pub events: Vec<Event>,
    /// The link is open.
    pub open: bool,
}

impl Bench {
    /// A Grbl host with default options and a default device.
    #[must_use]
    pub fn new() -> Self {
        Self::with(HostOptions::default(), DeviceConfig::default())
    }

    /// A bench with the given host options and device configuration.
    #[must_use]
    pub fn with(options: HostOptions, device: DeviceConfig) -> Self {
        Self::with_host(Host::for_kind(freekerf_protocol::FirmwareKind::Grbl, options), device)
    }

    /// A bench around an existing host.
    #[must_use]
    pub fn with_host(host: Host, device: DeviceConfig) -> Self {
        Self { host, device: Device::new(device), now: 1000, events: Vec::new(), open: false }
    }

    /// Opens the link (the device prints its banner) and exchanges traffic.
    pub fn open(&mut self) {
        self.open = true;
        self.host.open(self.now);
        self.device.connected();
        self.exchange();
    }

    /// Opens and waits until the connect-time queries are done and the host is ready.
    pub fn connect(&mut self) {
        self.open();
        assert!(self.run_until(|b| b.host.ready() && b.host.status() == freekerf_protocol::MachineStatus::Idle, 30_000));
        self.device.clear_lines();
        self.device.clear_realtime();
        self.device.reset_max_rx_used();
        self.events.clear();
    }

    /// Moves bytes both ways until nothing moves (no time passes).
    pub fn exchange(&mut self) {
        for _ in 0..10_000 {
            let mut moved = false;
            while let Some(o) = self.host.poll_output() {
                match o {
                    Output::Write(bytes) if self.open => {
                        self.device.receive(&bytes, self.now);
                        moved = true;
                    }
                    Output::Write(_) => {}
                    Output::Close => {
                        self.open = false;
                        self.host.close(false, self.now);
                        moved = true;
                    }
                }
            }
            let out = self.device.take_output();
            if !out.is_empty() && self.open {
                self.host.receive(&out, self.now);
                moved = true;
            }
            while let Some(e) = self.host.poll_event() {
                self.events.push(e);
            }
            if !moved {
                return;
            }
        }
        panic!("traffic did not settle");
    }

    /// Advances time by `ms` in steps of at most 10 ms, ticking host and device.
    pub fn advance(&mut self, ms: u64) {
        let end = self.now + ms;
        while self.now < end {
            self.now = (self.now + 10).min(end);
            self.device.tick(self.now);
            self.host.tick(self.now);
            self.exchange();
        }
    }

    /// One tick at the current time.
    pub fn tick(&mut self) {
        self.device.tick(self.now);
        self.host.tick(self.now);
        self.exchange();
    }

    /// Advances time until `pred` holds or `max_ms` passed; true if it held.
    pub fn run_until(&mut self, mut pred: impl FnMut(&Self) -> bool, max_ms: u64) -> bool {
        let end = self.now + max_ms;
        self.tick();
        while !pred(self) {
            if self.now >= end {
                return false;
            }
            self.advance(10);
        }
        true
    }

    /// Lines the device received.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.device.lines().to_vec()
    }

    /// Issues reported by the host.
    #[must_use]
    pub fn issues(&self) -> Vec<freekerf_protocol::Issue> {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::Issue(i) => Some(*i),
                _ => None,
            })
            .collect()
    }

    /// Loads a program from G-code text.
    pub fn load(&mut self, text: &str) {
        let p = freekerf_gcode::Program::parse(text, &freekerf_gcode::AnalysisOptions::default());
        self.host.set_program(p).unwrap_or_else(|e| panic!("cannot load: {e}"));
    }
}

impl Default for Bench {
    fn default() -> Self {
        Self::new()
    }
}

/// `n` job lines of about 20 bytes, as in the conformance protocol tests.
#[must_use]
pub fn job_lines(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("G1 X{}.{} Y{} F{}", i % 50, i % 10, (i * 7) % 40, 1000 + i)).collect()
}

/// A line as it goes on the wire (spaces removed).
#[must_use]
pub fn wire(line: &str) -> String {
    line.replace(' ', "")
}
