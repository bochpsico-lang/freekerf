//! The Clipper port against LaserGRBL's Clipper 6.4.2: every case of
//! `fixtures/clipper_cases.jsonl` (seeded random inputs and the C# output, made by
//! `fixtures/ClipperCases.cs` under Mono) must give exactly the same paths, in the
//! same order, with the same vertices.

#![allow(clippy::unwrap_used)] // test helpers parsing a trusted fixture

use freekerf_geometry::clipper::{
    ClipType, Clipper, ClipperOffset, EndType, IntPoint, JoinType, Paths, PolyFillType, PolyType,
};
use serde_json::Value;

fn paths(v: &Value) -> Paths {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            p.as_array()
                .unwrap()
                .iter()
                .map(|q| IntPoint::new(q[0].as_i64().unwrap(), q[1].as_i64().unwrap()))
                .collect()
        })
        .collect()
}

fn clip_type(v: &Value) -> ClipType {
    [
        ClipType::Intersection,
        ClipType::Union,
        ClipType::Difference,
        ClipType::Xor,
    ][v.as_u64().unwrap() as usize]
}

fn fill_type(v: &Value) -> PolyFillType {
    [
        PolyFillType::EvenOdd,
        PolyFillType::NonZero,
        PolyFillType::Positive,
        PolyFillType::Negative,
    ][v.as_u64().unwrap() as usize]
}

#[test]
fn clipper_matches_laser_grbl() {
    let text = include_str!("fixtures/clipper_cases.jsonl");
    let mut count = 0;
    for (n, line) in text.lines().enumerate() {
        let case: Value = serde_json::from_str(line).unwrap();
        let subj = paths(&case["subj"]);
        match case["kind"].as_str().unwrap() {
            "open" => {
                let mut c = Clipper::new();
                c.add_paths(&subj, PolyType::Subject, false).unwrap();
                c.add_paths(&paths(&case["clip"]), PolyType::Clip, true).unwrap();
                let tree = c
                    .execute_tree(
                        ClipType::Intersection,
                        PolyFillType::EvenOdd,
                        PolyFillType::EvenOdd,
                    )
                    .unwrap();
                assert_eq!(tree.is_some(), case["ok"].as_bool().unwrap(), "case {n}");
                let tree = tree.unwrap_or_default();
                assert_eq!(tree.open_paths(), paths(&case["open"]), "case {n} open");
                assert_eq!(tree.closed_paths(), paths(&case["closed"]), "case {n} closed");
            }
            kind @ ("paths" | "tree") => {
                let mut c = Clipper::new();
                c.strictly_simple = case["simple"].as_bool().unwrap();
                c.reverse_solution = case["reverse"].as_bool().unwrap();
                c.add_paths(&subj, PolyType::Subject, true).unwrap();
                c.add_paths(&paths(&case["clip"]), PolyType::Clip, true).unwrap();
                let (ct, sf, cf) = (
                    clip_type(&case["ct"]),
                    fill_type(&case["sf"]),
                    fill_type(&case["cf"]),
                );
                let got = if kind == "tree" {
                    c.execute_tree(ct, sf, cf).unwrap().map(|t| t.closed_paths())
                } else {
                    c.execute(ct, sf, cf).unwrap()
                };
                assert_eq!(got.is_some(), case["ok"].as_bool().unwrap(), "case {n}");
                assert_eq!(
                    got.unwrap_or_default(),
                    paths(&case["closed"]),
                    "case {n} ({kind})"
                );
            }
            "offset" => {
                let delta = case["delta"].as_f64().unwrap();
                let mut o = ClipperOffset::default();
                o.add_paths(&subj, JoinType::Round, EndType::ClosedPolygon);
                assert_eq!(
                    o.execute(delta).unwrap(),
                    paths(&case["closed"]),
                    "case {n} offset"
                );
                assert_eq!(
                    o.execute(delta * 2.0).unwrap(),
                    paths(&case["closed2"]),
                    "case {n} offset x2"
                );
            }
            other => panic!("unknown kind {other}"),
        }
        count += 1;
    }
    assert_eq!(count, 440);
}
