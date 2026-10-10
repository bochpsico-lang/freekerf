//! Any bytes as a G-code file: parse, analyze (time, ranges), report.
#![no_main]

use freekerf_gcode::report::Report;
use freekerf_gcode::{AnalysisOptions, Program};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let program = Program::parse_bytes(data, &AnalysisOptions::default());
    let _ = Report::new("fuzz.nc", &program);
});
