//! Every conformance golden file (minus `stream`) is reproduced by the analysis.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use freekerf_gcode::report::Report;
use freekerf_gcode::{AnalysisOptions, Decimal, Program, TimingConfig};

/// `golden.py` CONFIG: $110 = 6000 mm/min.
fn options() -> AnalysisOptions {
    AnalysisOptions {
        timing: TimingConfig {
            max_rate_x_mm_min: Decimal::from(6000),
        },
        support_pwm: true,
    }
}

#[test]
fn every_golden_file_matches() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir.join("gcode")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let program = Program::parse_bytes(&std::fs::read(&path).unwrap(), &options());
        let got = serde_json::to_value(Report::new(&name, &program)).unwrap();
        let golden: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(format!("golden/{stem}.json"))).unwrap())
                .unwrap();
        for (key, expected) in golden.as_object().unwrap() {
            if key == "stream" {
                continue;
            }
            assert_eq!(&got[key], expected, "{name}: '{key}' differs");
        }
        checked += 1;
    }
    assert_eq!(checked, 10);
}
