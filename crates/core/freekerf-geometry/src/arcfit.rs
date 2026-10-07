//! Replacing finely sampled curves (splines) with lines and circular arcs that stay
//! within a tolerance of every sample: far fewer `G1` moves and smoother motion.
//! Port of LaserGRBL's `SvgConverter/ArcFitter.cs`.

use std::f64::consts::PI;

use crate::{Point, Segment};

/// Default tolerance in mm (`ArcFitter.DefaultTolerance`).
pub const DEFAULT_TOLERANCE: f64 = 0.01;
/// Arcs with a larger radius (mm) are left as lines for the controller.
const MAX_RADIUS: f64 = 10000.0;

/// The segments that go from `points[0]` to the last point, each one ending on
/// one of the points.
#[must_use]
pub fn fit(points: &[Point], tolerance: f64) -> Vec<Segment> {
    let mut rv = Vec::new();
    let n = points.len();
    let mut i = 0;
    while i + 1 < n {
        // the longest range starting from i that fits: grow it doubling, then binary search the limit
        let mut good = i + 1;
        let mut good_segment = Segment::line(points[good]);
        let mut bad: Option<usize> = None;
        let mut probe = (n - 1).min(i + 2);
        while probe > good {
            match fit_range(points, i, probe, tolerance) {
                None => {
                    bad = Some(probe);
                    break;
                }
                Some(s) => {
                    good = probe;
                    good_segment = s;
                    probe = (n - 1).min(i + (probe - i) * 2);
                }
            }
        }
        if let Some(mut bad) = bad {
            while bad > good + 1 {
                let mid = usize::midpoint(good, bad);
                match fit_range(points, i, mid, tolerance) {
                    None => bad = mid,
                    Some(s) => {
                        good = mid;
                        good_segment = s;
                    }
                }
            }
        }
        rv.push(good_segment);
        i = good;
    }
    rv
}

/// A line or an arc from `p[i]` to `p[j]` passing within `tol` of every point between them.
fn fit_range(p: &[Point], i: usize, j: usize, tol: f64) -> Option<Segment> {
    if line_fits(p, i, j, tol) {
        return Some(Segment::line(p[j]));
    }

    // circle through the first, the middle and the last point
    let (a, b, c) = (p[i], p[usize::midpoint(i, j)], p[j]);
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let center = Point::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    );
    let r = a.distance(center);
    if r > MAX_RADIUS {
        return None;
    }

    // direction from the turn of the three points, then every point must be on the circle and in order along the arc
    let ccw = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x) > 0.0;
    let start = (a.y - center.y).atan2(a.x - center.x);
    let sweep = progress(start, (c.y - center.y).atan2(c.x - center.x), ccw);
    if !(1e-9..=2.0 * PI - 1e-6).contains(&sweep) {
        return None;
    }

    let mut last = 0.0;
    for q in &p[i + 1..j] {
        if (q.distance(center) - r).abs() > tol {
            return None;
        }
        let pr = progress(start, (q.y - center.y).atan2(q.x - center.x), ccw);
        if pr < last - 1e-9 || pr > sweep + 1e-9 {
            return None;
        }
        last = pr;
    }
    Some(Segment::arc(c, center, ccw))
}

/// Angle travelled from `start` to `angle` in the given direction, in `[0, 2π)`.
fn progress(start: f64, angle: f64, ccw: bool) -> f64 {
    let mut rv = if ccw { angle - start } else { start - angle };
    while rv < 0.0 {
        rv += 2.0 * PI;
    }
    while rv >= 2.0 * PI {
        rv -= 2.0 * PI;
    }
    rv
}

fn line_fits(p: &[Point], i: usize, j: usize, tol: f64) -> bool {
    let (a, b) = (p[i], p[j]);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    for q in &p[i + 1..j] {
        let t = if len2 > 0.0 {
            ((q.x - a.x) * dx + (q.y - a.y) * dy) / len2
        } else {
            0.0
        };
        if !(0.0..=1.0).contains(&t) {
            return false;
        }
        let (x, y) = (a.x + t * dx - q.x, a.y + t * dy - q.y);
        if x * x + y * y > tol * tol {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle_points(n: u32, r: f64, ccw: bool) -> Vec<Point> {
        (0..=n)
            .map(|i| {
                let a = PI * f64::from(i) / f64::from(n) * if ccw { 1.0 } else { -1.0 };
                Point::new(r * a.cos(), r * a.sin())
            })
            .collect()
    }

    #[test]
    fn samples_of_a_half_circle_become_one_arc() {
        let pts = circle_points(64, 10.0, true);
        let s = fit(&pts, 0.01);
        assert_eq!(s.len(), 1);
        match s[0] {
            Segment::Arc { end, center, ccw } => {
                assert!(ccw);
                assert!(center.distance(Point::ZERO) < 1e-6);
                assert_eq!(end, pts[64]);
            }
            Segment::Line { .. } | Segment::Cubic { .. } => panic!("expected an arc"),
        }
        let cw = fit(&circle_points(64, 10.0, false), 0.01);
        assert!(matches!(cw[0], Segment::Arc { ccw: false, .. }));
    }

    #[test]
    fn straight_samples_become_one_line_and_corners_split() {
        let pts: Vec<Point> = (0..10).map(|i| Point::new(f64::from(i), 0.0)).collect();
        assert_eq!(fit(&pts, 0.01), vec![Segment::line(Point::new(9.0, 0.0))]);
        let corner = vec![
            Point::ZERO,
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(2.0, 1.0),
            Point::new(2.0, 2.0),
        ];
        let s = fit(&corner, 0.01);
        assert_eq!(s.len(), 2);
        assert!(fit(&[Point::ZERO], 0.01).is_empty());
        // a back-and-forth line does not fit (t outside 0..1) and the points are collinear
        let back = vec![Point::ZERO, Point::new(2.0, 0.0), Point::new(1.0, 0.0)];
        assert_eq!(fit(&back, 0.01).len(), 2);
        // a huge radius stays lines
        let flat = vec![Point::ZERO, Point::new(1.0, 1e-5), Point::new(2.0, 0.0)];
        assert_eq!(fit(&flat, 1e-9).len(), 2);
        assert!((progress(0.0, -0.5, true) - (2.0 * PI - 0.5)).abs() < 1e-12);
        assert!((progress(0.0, 7.0, true) - (7.0 - 2.0 * PI)).abs() < 1e-12);
    }
}
