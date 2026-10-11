//! Importer golden cases copied from conformance (`fixtures/importers`): every case
//! must give exactly LaserGRBL's result (the full set runs in the `conformance` CI job).

use std::path::Path;

use freekerf_service::import_case;
use serde_json::Value;

#[test]
fn importer_cases_match_laser_grbl() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/importers");
    let inputs = dir.join("inputs");
    let read = |name: &str| std::fs::read(inputs.join(name)).map_err(|e| e.to_string());
    let strip = |v: &Value| -> Value {
        // svg_text: compared without the converter's comments (DIV-106)
        let lines: Vec<Value> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                let mut s = String::new();
                let mut depth = false;
                for c in l.as_str().unwrap().chars() {
                    match c {
                        '(' if !depth => depth = true,
                        ')' if depth => depth = false,
                        _ if !depth => s.push(c),
                        _ => {}
                    }
                }
                Value::from(s)
            })
            .collect();
        Value::Array(lines)
    };
    let mut cases: Vec<_> = std::fs::read_dir(dir.join("cases"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    cases.sort();
    assert!(cases.len() >= 18);
    for path in cases {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let case: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let golden: Value =
            serde_json::from_slice(&std::fs::read(dir.join("golden").join(format!("{name}.json"))).unwrap())
                .unwrap();
        let got = import_case::run(&case, &read).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (key, expected) in golden.as_object().unwrap() {
            if key == "converter_output" {
                assert_eq!(strip(&got[key]), strip(expected), "{name}: {key}");
            } else {
                assert_eq!(&got[key], expected, "{name}: {key}");
            }
        }
        assert_eq!(
            got.as_object().unwrap().len(),
            golden.as_object().unwrap().len(),
            "{name}: keys"
        );
    }
}
