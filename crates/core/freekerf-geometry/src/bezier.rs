//! Cubic Bézier flattening.
//!
//! [`flatten`] ports LaserGRBL's `SvgConverter/BezierTools.cs` (Darren Starr, after
//! Graphics Gems): recursive halving until both "triangles" of the control polygon
//! are flat enough. Note the test is `sqrt(triangle area) < error`, not a distance,
//! and every flat sub-curve emits its four control points, so the polyline passes
//! through off-curve points (within the tolerance). [`uniform`] is the "legacy"
//! fixed-step approximation (`GetBezierApproximationOld`).

use crate::Point;

/// Default recursion limit of `BezierTools.FlattenTo`.
pub const MAX_SUBDIVISIONS: u32 = 20;

fn split_at(cp: &[Point; 4], t: f64) -> ([Point; 4], [Point; 4]) {
    // De Casteljau triangle, as SplitCurveAtT computes it
    let mut v = [[Point::ZERO; 4]; 4];
    v[0] = *cp;
    for i in 1..=3 {
        for j in 0..=3 - i {
            v[i][j] = Point::new(
                (1.0 - t) * v[i - 1][j].x + t * v[i - 1][j + 1].x,
                (1.0 - t) * v[i - 1][j].y + t * v[i - 1][j + 1].y,
            );
        }
    }
    let mut a = [Point::ZERO; 4];
    let mut b = [Point::ZERO; 4];
    for i in 0..=3 {
        a[i] = v[i][0];
        b[i] = v[3 - i][i];
    }
    (a, b)
}

fn triangle_area(a: Point, b: Point, c: Point) -> f64 {
    (a.x * b.y + b.x * c.y + c.x * a.y - a.y * b.x - b.y * c.x - c.y * a.x).abs() / 2.0
}

/// Curves with a control point beyond this (mm, a kilometre) are not subdivided.
pub const MAX_COORDINATE: f64 = 1.0e6;

fn flatten_segment(
    seg: &[Point; 4],
    error: f64,
    subdivisions: u32,
    max: u32,
    out: &mut Vec<Point>,
    skip_first: bool,
) {
    // a curve with a non-finite point, or one reaching beyond any machine, never
    // looks flat: LaserGRBL subdivides it to the limit (2^20 pieces); it is drawn
    // as it is instead (DIV-109)
    if subdivisions == 0
        && !seg
            .iter()
            .all(|p| p.x.abs() <= MAX_COORDINATE && p.y.abs() <= MAX_COORDINATE)
    {
        out.extend(seg.iter().skip(usize::from(skip_first)));
        return;
    }
    // a negative tolerance (a drawing scaled by a negative size) is taken as
    // positive, and a straight piece is flat whatever the tolerance: LaserGRBL
    // subdivides those to the limit (DIV-109); with a positive tolerance and a
    // curve piece this is LaserGRBL's test
    let error = error.abs();
    let (a1, a2) = (
        triangle_area(seg[0], seg[1], seg[2]),
        triangle_area(seg[1], seg[2], seg[3]),
    );
    if subdivisions >= max || (a1.sqrt() < error && a2.sqrt() < error) || (a1 == 0.0 && a2 == 0.0) {
        out.extend(seg.iter().skip(usize::from(skip_first)));
        return;
    }
    let (a, b) = split_at(seg, 0.5);
    flatten_segment(&a, error, subdivisions + 1, max, out, skip_first);
    flatten_segment(&b, error, subdivisions + 1, max, out, true);
}

/// `BezierTools.FlattenTo` for one curve: the start point, then the points of every
/// flat sub-curve (its control points 1..3).
#[must_use]
pub fn flatten(cp: &[Point; 4], error: f64, max_subdivisions: u32) -> Vec<Point> {
    let mut out = vec![cp[0]];
    flatten_segment(cp, error, 0, max_subdivisions, &mut out, true);
    out
}

/// `BezierTools.FlattenTo` for contiguous curves (`3n + 1` control points).
#[must_use]
pub fn flatten_chain(points: &[Point], error: f64, max_subdivisions: u32) -> Vec<Point> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let mut out = vec![first];
    let mut i = 0;
    while i + 4 <= points.len() {
        let seg = [points[i], points[i + 1], points[i + 2], points[i + 3]];
        flatten_segment(&seg, error, 0, max_subdivisions, &mut out, true);
        i += 3;
    }
    out
}

/// Point at `t` by recursive linear interpolation (`GetBezierPoint`).
fn point_at(t: f64, cp: &[Point]) -> Point {
    if cp.len() == 1 {
        return cp[0];
    }
    let p0 = point_at(t, &cp[..cp.len() - 1]);
    let p1 = point_at(t, &cp[1..]);
    let x = (1.0 - t) * p0.x + t * p1.x;
    Point::new(x, (1.0 - t) * p0.y + t * p1.y)
}

/// `segments + 1` points at equal steps of `t` (`GetBezierApproximationOld`).
#[must_use]
pub fn uniform(cp: &[Point; 4], segments: u32) -> Vec<Point> {
    (0..=segments)
        .map(|i| point_at(f64::from(i) / f64::from(segments.max(1)), cp))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_curves_are_not_subdivided() {
        let cp = [
            Point::new(0.0, 0.0),
            Point::new(1.0, f64::NEG_INFINITY),
            Point::new(2.0, 1.0),
            Point::new(3.0, 0.0),
        ];
        assert_eq!(flatten(&cp, 0.2, 20).len(), 4);
        assert_eq!(flatten_chain(&cp, 0.2, 20).len(), 4);
        let far = [cp[0], Point::new(1.0, 1e26), cp[2], cp[3]];
        assert_eq!(flatten(&far, 0.2, 20).len(), 4);
        // negative or zero tolerances do not subdivide to the limit
        let round = [
            Point::new(0.0, 0.0),
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        ];
        assert_eq!(flatten(&round, -0.2, 20), flatten(&round, 0.2, 20));
        let flat = [Point::new(0.0, 0.0); 4];
        assert_eq!(flatten(&flat, 0.0, 20).len(), 4);
    }

    const CURVE: [Point; 4] = [
        Point::new(0.0, 0.0),
        Point::new(0.0, 10.0),
        Point::new(10.0, 10.0),
        Point::new(10.0, 0.0),
    ];

    #[test]
    fn flat_curves_emit_their_control_points() {
        let line = [
            Point::ZERO,
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(3.0, 0.0),
        ];
        assert_eq!(flatten(&line, 0.2, MAX_SUBDIVISIONS), line.to_vec());
    }

    #[test]
    fn subdivision_reaches_the_tolerance_and_respects_the_limit() {
        let fine = flatten(&CURVE, 0.01, MAX_SUBDIVISIONS);
        let coarse = flatten(&CURVE, 1.0, MAX_SUBDIVISIONS);
        assert!(fine.len() > coarse.len());
        assert_eq!(fine[0], CURVE[0]);
        assert_eq!(*fine.last().unwrap(), CURVE[3]);
        // max 0: the control points as they are
        assert_eq!(flatten(&CURVE, 1e-9, 0), CURVE.to_vec());
        let chain = flatten_chain(
            &[
                CURVE[0],
                CURVE[1],
                CURVE[2],
                CURVE[3],
                Point::new(10.0, -10.0),
                Point::new(0.0, -10.0),
                Point::ZERO,
            ],
            1.0,
            MAX_SUBDIVISIONS,
        );
        assert_eq!(chain[..coarse.len()], coarse[..]);
        assert_eq!(*chain.last().unwrap(), Point::ZERO);
        assert!(flatten_chain(&[], 1.0, 1).is_empty());
    }

    #[test]
    fn uniform_steps() {
        let pts = uniform(&CURVE, 12);
        assert_eq!(pts.len(), 13);
        assert_eq!(pts[0], CURVE[0]);
        assert_eq!(pts[12], CURVE[3]);
        assert!((pts[6].x - 5.0).abs() < 1e-12 && (pts[6].y - 7.5).abs() < 1e-12);
    }
}
