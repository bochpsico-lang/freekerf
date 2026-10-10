//! Import of LaserGRBL's data into FreeKerf (PLAN §10, M2), **read only**: the
//! LaserGRBL files are never written.
//!
//! - `LaserGRBL.Settings.bin` (a `BinaryFormatter` dictionary) → [`settings`]: the
//!   values FreeKerf has an equivalent for, with a report of the rest;
//! - `LaserLifeCounter.bin` → [`laser_life`]: the laser module counters;
//! - the `BinaryFormatter` format itself → [`nrbf`], a reader that only builds a
//!   tree of values (nothing is instantiated).
//!
//! `UsageStats.bin` is LaserGRBL's anonymous usage telemetry: FreeKerf has none
//! (DIV-027), so it is not imported. Materials go through `freekerf-materials`,
//! custom buttons (`.zbn`) through the script migration.

pub mod laser_life;
pub mod nrbf;
pub mod settings;

/// Errors of the import.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MigrateError {
    /// Not a readable `BinaryFormatter` stream.
    #[error("cannot read the file: {0}")]
    Format(#[from] nrbf::NrbfError),
    /// The file holds something else than expected.
    #[error("unexpected content: {0}")]
    Content(String),
}
