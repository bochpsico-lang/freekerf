//! G-code for FreeKerf: commands, modal state, arcs, program analysis (ranges,
//! quadrant) and the time estimate.
//!
//! This crate is part of the **core** layer: it does no I/O and builds for
//! `wasm32-unknown-unknown` (disable the `parallel` feature there). The behaviour
//! follows LaserGRBL (`GrblCommand`, `StateBuilder`, `GrblFile`) so that the
//! `conformance` golden files match; intentional differences are listed in
//! `doc/divergences.md`.
//!
//! ```
//! use freekerf_gcode::{AnalysisOptions, Program};
//!
//! let p = Program::parse("G0 X10\nM3 S500\nG1 X20 F600\nM5\n", &AnalysisOptions::default());
//! assert_eq!(p.len(), 4);
//! assert_eq!(p.estimated_ms(), 1150);
//! ```

pub mod arc;
pub mod command;
pub mod number;
pub mod program;
pub mod range;
pub mod report;
pub mod state;

pub use command::{Command, Word};
pub use program::{AnalysisOptions, Program, ProgramLine};
pub use range::{ProgramRange, Quadrant};
pub use state::{MachineState, TimingConfig};

pub use rust_decimal::Decimal;
