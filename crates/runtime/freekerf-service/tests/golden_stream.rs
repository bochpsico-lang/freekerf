//! The full conformance golden files, `stream` included, from `analysis::analyze`.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use freekerf_service::analysis::{AnalyzeOptions, analyze};
use freekerf_testkit::fixtures::gcode_fixture;

#[test]
fn every_golden_file_matches_including_the_stream() {
    let mut n = 0;
    for entry in std::fs::read_dir(gcode_fixture("gcode")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let report = analyze(&name, &std::fs::read(&path).unwrap(), &AnalyzeOptions::default());
        let got = serde_json::to_value(&report).unwrap();
        let golden: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(gcode_fixture(&format!("golden/{stem}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(got, golden, "{name}");
        n += 1;
    }
    assert_eq!(n, 10);
}
