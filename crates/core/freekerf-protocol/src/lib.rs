//! The machine protocol of FreeKerf, sans-IO: a state machine that takes bytes,
//! lines and time, and returns bytes to send and events.
//!
//! [`Host`] is a port of LaserGRBL's `GrblCore`: streaming (buffered,
//! synchronous, repeat-on-error) with RX buffer accounting, responses, status
//! reports, firmware detection, overrides, jog, `$$`/`$I`, settings writes, job
//! lifecycle with passes and resume, custom code with `[expr]`, laser life
//! counters. Firmware variants (Grbl, Smoothie, Marlin, VigoWork) implement
//! [`Firmware`]. The caller owns the transport and the clock, so everything is
//! tested deterministically (see `freekerf-machine` for the I/O driver).
//!
//! Intentional differences from LaserGRBL are listed in `doc/divergences.md`.

pub mod console;
pub mod expr;
pub mod firmware;
pub mod grbl_settings;
pub mod host;
pub mod jog;
pub mod life;
pub mod projection;
pub mod status;
pub mod version;

pub use console::{CommandStatus, ConsoleRow, MessageKind};
pub use firmware::{Firmware, FirmwareKind};
pub use grbl_settings::GrblSettings;
pub use host::{
    AutoCooling, Event, Host, HostError, HostOptions, HostSnapshot, Output, ResumeSuggestion, StreamingMode,
    SystemOpKind,
};
pub use jog::JogDirection;
pub use life::{LaserLife, LaserLifeCounter};
pub use projection::{Issue, TimeProjection};
pub use status::{MachineStatus, Point3};
pub use version::FirmwareVersion;

/// A monotonic clock in milliseconds, injected wherever time is needed (ADR 0003).
pub trait Clock: Send + Sync {
    /// Milliseconds since an arbitrary origin; never goes back.
    fn now_ms(&self) -> u64;
}
