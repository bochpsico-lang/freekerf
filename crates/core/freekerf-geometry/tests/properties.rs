//! Geometry invariants.

use freekerf_geometry::boolean::{self, Op};
use freekerf_geometry::hatch::{self, FillPattern};
use freekerf_geometry::{Matrix, Path, Point, Segment, arcfit, bezier};
use proptest::prelude::*;

fn coord() -> impl Strategy<Value = f64> {
    -100.0..100.0f64
}

fn point() -> impl Strategy<Value = Point> {
    (coord(), coord()).prop_map(|(x, y)| Point::new(x, y))
}

fn rect() -> impl Strategy<Value = Vec<Point>> {
    (coord(), coord(), 1.0..50.0f64, 1.0..50.0f64).prop_map(|(x, y, w, h)| {
        vec![
            Point::new(x, y),
            Point::new(x + w, y),
            Point::new(x + w, y + h),
            Point::new(x, y + h),
        ]
    })
}

fn area(polys: &[Vec<Point>]) -> f64 {
    polys.iter().map(|p| boolean::signed_area(p)).sum::<f64>().abs()
}

proptest! {
    #[test]
    fn append_composes_transforms(a in point(), sx in 0.1..10.0f64, angle in -360.0..360.0f64, d in point()) {
        let mut m = Matrix::IDENTITY;
        m.scale(sx, sx);
        m.rotate(angle);
        m.translate(d.x, d.y);
        let mut step = Matrix::scaling(sx, sx).transform(a);
        let mut r = Matrix::IDENTITY;
        r.rotate(angle);
        step = r.transform(step) + d;
        let direct = m.transform(a);
        prop_assert!(direct.distance(step) < 1e-9);
        prop_assert!(m.is_similarity());
    }

    #[test]
    fn inclusion_exclusion_holds_for_rectangles(a in rect(), b in rect()) {
        let s = boolean::DEFAULT_SCALE;
        let i = area(&boolean::boolean(&[a.clone()], &[b.clone()], Op::Intersection, s).unwrap());
        let u = area(&boolean::boolean(&[a.clone()], &[b.clone()], Op::Union, s).unwrap());
        let d = area(&boolean::boolean(&[a.clone()], &[b.clone()], Op::Difference, s).unwrap());
        let (aa, ab) = (area(&[a]), area(&[b]));
        let tol = 1e-3 * (aa + ab) + 1e-2;
        prop_assert!((u - (aa + ab - i)).abs() < tol);
        prop_assert!((d - (aa - i)).abs() < tol);
    }

    #[test]
    fn shrinking_never_grows(a in rect(), delta in 0.01..5.0f64) {
        let inner = boolean::offset(&[a.clone()], -delta, boolean::Join::Square, boolean::DEFAULT_SCALE).unwrap();
        prop_assert!(area(&inner) <= area(&[a]) + 1e-6);
    }

    #[test]
    fn hatch_lines_stay_inside_the_shape(w in 2.0..40.0f64, h in 2.0..40.0f64, step in 0.2..3.0f64,
                                         pattern in prop::sample::select(vec![FillPattern::Horizontal, FillPattern::Vertical,
                                             FillPattern::Diagonal, FillPattern::ReverseDiagonal, FillPattern::Grid, FillPattern::Cross])) {
        let shape = vec![Point::new(1.0, 1.0), Point::new(w - 1.0, 1.0), Point::new(w - 1.0, h - 1.0), Point::new(1.0, h - 1.0)];
        let lines = hatch::fill(&[shape], w, h, pattern, step).unwrap();
        for l in &lines {
            for p in l {
                prop_assert!(p.x >= 1.0 - 0.002 && p.x <= w - 1.0 + 0.002, "{p:?}");
                prop_assert!(p.y >= 1.0 - 0.002 && p.y <= h - 1.0 + 0.002, "{p:?}");
            }
        }
    }

    #[test]
    fn flattening_keeps_the_end_points(p0 in point(), p1 in point(), p2 in point(), p3 in point(), err in 0.001..2.0f64) {
        let f = bezier::flatten(&[p0, p1, p2, p3], err, bezier::MAX_SUBDIVISIONS);
        prop_assert_eq!(f[0], p0);
        prop_assert_eq!(*f.last().unwrap(), p3);
        // each flat sub-curve emits 3 points after the start
        prop_assert_eq!((f.len() - 1) % 3, 0);
    }

    #[test]
    fn arc_fitting_stays_within_tolerance(r in 1.0..100.0f64, n in 3u32..200, span in 0.1..3.0f64) {
        let pts: Vec<Point> = (0..=n).map(|i| {
            let a = span * f64::from(i) / f64::from(n);
            Point::new(r * a.cos(), r * a.sin())
        }).collect();
        let segs = arcfit::fit(&pts, 0.01);
        prop_assert!(!segs.is_empty());
        prop_assert_eq!(segs.last().unwrap().end(), *pts.last().unwrap());
        // the fitted path stays near the samples
        let mut path = Path::new(pts[0]);
        path.segments = segs;
        for q in path.to_polyline(0.001) {
            prop_assert!((q.distance(Point::ZERO) - r).abs() < 0.02 + r * 1e-9);
        }
    }

    #[test]
    fn json_round_trip(p in point(), q in point(), c in point(), ccw in any::<bool>()) {
        let mut path = Path::new(p);
        path.segments.push(Segment::line(q));
        path.segments.push(Segment::arc(p, c, ccw));
        let json = serde_json::to_string(&path).unwrap();
        let back: Path = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(back, path);
    }
}
