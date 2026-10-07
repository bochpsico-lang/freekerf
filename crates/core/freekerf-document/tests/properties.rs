//! Any document survives the project file unchanged.

use freekerf_document::{Color, Document, FillSettings, LayerMode, ObjectKind, UnsupportedReason};
use freekerf_geometry::hatch::FillPattern;
use freekerf_geometry::{Path, Point, Segment};
use proptest::prelude::*;

fn point() -> impl Strategy<Value = Point> {
    (-1e4..1e4f64, -1e4..1e4f64).prop_map(|(x, y)| Point::new(x, y))
}

fn segment() -> impl Strategy<Value = Segment> {
    prop_oneof![
        point().prop_map(Segment::line),
        (point(), point(), any::<bool>()).prop_map(|(e, c, ccw)| Segment::arc(e, c, ccw)),
        (point(), point(), point()).prop_map(|(c1, c2, end)| Segment::Cubic { c1, c2, end }),
    ]
}

fn path() -> impl Strategy<Value = Path> {
    (point(), prop::collection::vec(segment(), 0..6), any::<bool>()).prop_map(|(start, segments, closed)| {
        Path {
            start,
            segments,
            closed,
        }
    })
}

fn mode() -> impl Strategy<Value = LayerMode> {
    prop::sample::select(vec![
        LayerMode::Line,
        LayerMode::Fill,
        LayerMode::FillAndLine,
        LayerMode::Cut,
        LayerMode::Ignore,
    ])
}

proptest! {
    #[test]
    fn documents_round_trip(layers in prop::collection::vec((any::<[u8; 3]>(), mode(), 1u32..20000, 0u32..1000, 1u32..5, 0.1..20.0f64), 1..5),
                            objects in prop::collection::vec((0usize..5, prop::collection::vec(path(), 0..4), any::<bool>()), 0..8)) {
        let mut d = Document::new();
        let mut ids = Vec::new();
        for (c, m, speed, power, passes, lpm) in layers {
            let id = d.layer_for_color(Color::rgb(c[0], c[1], c[2]), speed, power);
            let l = d.layer_mut(id).unwrap();
            l.mode = m;
            l.passes = passes;
            l.fill = FillSettings { pattern: FillPattern::Grid, lines_per_mm: lpm };
            ids.push(id);
        }
        for (li, paths, unsupported) in objects {
            let layer = ids[li % ids.len()];
            let kind = if unsupported {
                ObjectKind::Unsupported { element: "text".into(), reason: UnsupportedReason::NotConverted }
            } else {
                ObjectKind::Shape { paths }
            };
            d.add_object(layer, kind).unwrap();
        }
        let back = Document::from_fkproj(d.to_fkproj().as_bytes()).unwrap();
        prop_assert_eq!(back, d);
    }
}
