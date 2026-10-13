//! Paths: a start point followed by lines, circular arcs and cubic Bézier curves.
//!
//! Arcs keep their centre and direction so they can become `G2`/`G3`. The
//! approximations (`to_polyline`, `bounds`) follow LaserGRBL's `VectorPath`
//! (`SvgConverter/VectorDrawing.cs`) so fillings computed from them match.

use std::f64::consts::PI;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::bezier;
use crate::{Matrix, Point, Rect};

/// One piece of a path, from the end of the previous one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Segment {
    /// A straight line.
    Line {
        /// End point.
        end: Point,
    },
    /// A circular arc (never a full turn unless start and end coincide).
    Arc {
        /// End point.
        end: Point,
        /// Centre.
        center: Point,
        /// Counterclockwise (`G3`); clockwise (`G2`) otherwise. Y points up.
        ccw: bool,
    },
    /// A cubic Bézier curve.
    Cubic {
        /// First control point.
        c1: Point,
        /// Second control point.
        c2: Point,
        /// End point.
        end: Point,
    },
}

impl Segment {
    /// End point.
    #[must_use]
    pub const fn end(&self) -> Point {
        match *self {
            Self::Line { end } | Self::Arc { end, .. } | Self::Cubic { end, .. } => end,
        }
    }

    /// A line to `end`.
    #[must_use]
    pub const fn line(end: Point) -> Self {
        Self::Line { end }
    }

    /// An arc to `end` around `center`.
    #[must_use]
    pub const fn arc(end: Point, center: Point, ccw: bool) -> Self {
        Self::Arc { end, center, ccw }
    }
}

/// Signed angle travelled by an arc from `start` (positive = counterclockwise),
/// never a full turn (`VectorSegment.Sweep`).
#[must_use]
pub fn arc_sweep(start: Point, end: Point, center: Point, ccw: bool) -> f64 {
    let a1 = (start.y - center.y).atan2(start.x - center.x);
    let a2 = (end.y - center.y).atan2(end.x - center.x);
    let mut sweep = a2 - a1;
    if ccw && sweep <= 1e-12 {
        sweep += 2.0 * PI;
    }
    if !ccw && sweep >= -1e-12 {
        sweep -= 2.0 * PI;
    }
    sweep
}

/// A connected sequence of segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Path {
    /// First point.
    pub start: Point,
    /// Segments, each from the end of the previous one.
    pub segments: Vec<Segment>,
    /// The last segment ends on `start` and the shape is meant to be closed.
    #[serde(default)]
    pub closed: bool,
    /// Drawn from where the previous path ended, without a travel move (SVG path
    /// data that does not begin with a moveto).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub from_current: bool,
}

impl Path {
    /// A path with only a start point.
    #[must_use]
    pub const fn new(start: Point) -> Self {
        Self {
            start,
            segments: Vec::new(),
            closed: false,
            from_current: false,
        }
    }

    /// A polyline through `points` (empty input gives a path at the origin).
    #[must_use]
    pub fn polyline(points: &[Point], closed: bool) -> Self {
        let mut p = Self::new(points.first().copied().unwrap_or_default());
        p.segments = points.iter().skip(1).map(|&e| Segment::line(e)).collect();
        if closed && points.len() > 1 && points[points.len() - 1] != points[0] {
            p.segments.push(Segment::line(points[0]));
        }
        p.closed = closed;
        p
    }

    /// Last point.
    #[must_use]
    pub fn end(&self) -> Point {
        self.segments.last().map_or(self.start, Segment::end)
    }

    /// Approximation with straight lines within `tolerance` of every arc
    /// (`VectorPath.ToPolyline`); curves are flattened with [`bezier::flatten`].
    #[must_use]
    pub fn to_polyline(&self, tolerance: f64) -> Vec<Point> {
        let mut rv = vec![self.start];
        let mut current = self.start;
        for s in &self.segments {
            match *s {
                Segment::Arc { end, center, ccw } => {
                    let r = end.distance(center);
                    let sweep = arc_sweep(current, end, center, ccw);
                    let a1 = (current.y - center.y).atan2(current.x - center.x);
                    let step = if r > tolerance {
                        2.0 * (1.0 - tolerance / r).acos()
                    } else {
                        PI / 4.0
                    };
                    let n = arc_steps(sweep, step);
                    for i in 1..n {
                        let a = a1 + sweep * f64::from(i) / f64::from(n);
                        rv.push(Point::new(center.x + r * a.cos(), center.y + r * a.sin()));
                    }
                    rv.push(end);
                }
                Segment::Cubic { c1, c2, end } => {
                    let pts = bezier::flatten(&[current, c1, c2, end], tolerance, bezier::MAX_SUBDIVISIONS);
                    rv.extend(pts.into_iter().skip(1));
                }
                Segment::Line { end } => rv.push(end),
            }
            current = s.end();
        }
        rv
    }

    /// Bounds: segment ends, arc quadrant points inside their sweep, and curve
    /// control polygons (`VectorPath.ExtendBounds` for lines and arcs).
    #[must_use]
    pub fn bounds(&self) -> Rect {
        let mut r = Rect::EMPTY;
        r.add(self.start);
        let mut current = self.start;
        for s in &self.segments {
            r.add(s.end());
            match *s {
                Segment::Arc { end, center, ccw } => {
                    let radius = end.distance(center);
                    let start = (current.y - center.y).atan2(current.x - center.x);
                    let stop = start + arc_sweep(current, end, center, ccw);
                    let (from, to) = (start.min(stop), start.max(stop));
                    let mut q = (from / (PI / 2.0)).ceil() * (PI / 2.0);
                    while q <= to {
                        r.add(Point::new(
                            center.x + radius * q.cos(),
                            center.y + radius * q.sin(),
                        ));
                        q += PI / 2.0;
                    }
                }
                Segment::Cubic { c1, c2, .. } => {
                    for p in bezier::flatten(&[current, c1, c2, s.end()], 0.01, bezier::MAX_SUBDIVISIONS) {
                        r.add(p);
                    }
                }
                Segment::Line { .. } => {}
            }
            current = s.end();
        }
        r
    }

    /// Moves the path.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        let d = Point::new(dx, dy);
        self.start = self.start + d;
        for s in &mut self.segments {
            match s {
                Segment::Line { end } => *end = *end + d,
                Segment::Arc { end, center, .. } => {
                    *end = *end + d;
                    *center = *center + d;
                }
                Segment::Cubic { c1, c2, end } => {
                    *c1 = *c1 + d;
                    *c2 = *c2 + d;
                    *end = *end + d;
                }
            }
        }
    }

    /// Applies a transform. Arcs stay arcs under similarities (their direction flips
    /// with a mirror); under other transforms they are replaced by lines within
    /// `tolerance`.
    #[must_use]
    pub fn transformed(&self, m: &Matrix, tolerance: f64) -> Self {
        let mirror = m.determinant() < 0.0;
        let similar = m.is_similarity();
        let mut out = Self::new(m.transform(self.start));
        out.closed = self.closed;
        out.from_current = self.from_current;
        let mut current = self.start;
        for s in &self.segments {
            match *s {
                Segment::Line { end } => out.segments.push(Segment::line(m.transform(end))),
                Segment::Cubic { c1, c2, end } => out.segments.push(Segment::Cubic {
                    c1: m.transform(c1),
                    c2: m.transform(c2),
                    end: m.transform(end),
                }),
                Segment::Arc { end, center, ccw } => {
                    if similar {
                        out.segments
                            .push(Segment::arc(m.transform(end), m.transform(center), ccw != mirror));
                    } else {
                        let mut piece = Self::new(current);
                        piece.segments.push(*s);
                        let pts = piece.to_polyline(tolerance);
                        out.segments
                            .extend(pts.into_iter().skip(1).map(|p| Segment::line(m.transform(p))));
                    }
                }
            }
            current = s.end();
        }
        out
    }

    /// Length of the path (arcs exactly, curves flattened within 0.01).
    #[must_use]
    pub fn length(&self) -> f64 {
        let mut total = 0.0;
        let mut current = self.start;
        for s in &self.segments {
            total += match *s {
                Segment::Line { end } => current.distance(end),
                Segment::Arc { end, center, ccw } => {
                    arc_sweep(current, end, center, ccw).abs() * end.distance(center)
                }
                Segment::Cubic { c1, c2, end } => {
                    bezier::flatten(&[current, c1, c2, end], 0.01, bezier::MAX_SUBDIVISIONS)
                        .windows(2)
                        .map(|w| w[0].distance(w[1]))
                        .sum()
                }
            };
            current = s.end();
        }
        total
    }
}

/// `max(1, min(10000, ceil(|sweep| / min(step, PI/4))))` as an integer.
#[allow(clippy::cast_possible_truncation)]
fn arc_steps(sweep: f64, step: f64) -> i32 {
    let n = (sweep.abs() / step.min(PI / 4.0)).ceil();
    if n.is_nan() {
        return 1;
    }
    (n.min(10000.0) as i32).max(1)
}

/// Distance under which path ends are taken as touching ([`join_paths`]), mm.
pub const JOIN_TOLERANCE_MM: f64 = 0.01;

impl Path {
    /// The same path drawn the other way (lines, arcs and curves reversed).
    #[must_use]
    pub fn reversed(&self) -> Self {
        // the start of each segment, then the path's end becomes the new start
        let mut starts = Vec::with_capacity(self.segments.len());
        let mut at = self.start;
        for s in &self.segments {
            starts.push(at);
            at = s.end();
        }
        let mut out = Self::new(at);
        for (s, from) in self.segments.iter().zip(starts).rev() {
            out.segments.push(match *s {
                Segment::Line { .. } => Segment::Line { end: from },
                Segment::Arc { center, ccw, .. } => Segment::Arc {
                    end: from,
                    center,
                    ccw: !ccw,
                },
                Segment::Cubic { c1, c2, .. } => Segment::Cubic {
                    c1: c2,
                    c2: c1,
                    end: from,
                },
            });
        }
        out.closed = self.closed;
        out
    }
}

/// Joins open paths whose ends touch (within `tolerance` mm) into continuous
/// paths, reversing some when needed; a joined path whose ends meet is closed.
/// Closed paths and paths drawn from the current point are kept as they are.
/// Deterministic: paths are taken in order, each one extended at its end, then
/// at its start, by the first path that fits.
#[must_use]
pub fn join_paths(paths: Vec<Path>, tolerance: f64) -> Vec<Path> {
    let near = |a: Point, b: Point| a.distance(b) <= tolerance;
    let mut open: Vec<Option<Path>> = Vec::new();
    let mut out = Vec::new();
    for p in paths {
        if p.closed || p.from_current || p.segments.is_empty() {
            out.push(p);
        } else {
            open.push(Some(p));
        }
    }
    for i in 0..open.len() {
        let Some(mut chain) = open[i].take() else {
            continue;
        };
        loop {
            let (start, end) = (chain.start, chain.end());
            if near(start, end) && chain.segments.len() > 1 {
                break;
            }
            let next = open.iter().position(|o| {
                o.as_ref().is_some_and(|o| {
                    [o.start, o.end()]
                        .into_iter()
                        .any(|q| near(q, end) || near(q, start))
                })
            });
            let Some(j) = next else {
                break;
            };
            let Some(other) = open[j].take() else {
                break;
            };
            if near(other.start, end) {
                chain.segments.extend(other.segments);
            } else if near(other.end(), end) {
                chain.segments.extend(other.reversed().segments);
            } else {
                // it touches the chain's start: prepend it, drawn towards the start
                let head = if near(other.end(), start) {
                    other
                } else {
                    other.reversed()
                };
                let mut segments = head.segments;
                segments.extend(chain.segments);
                chain = Path {
                    start: head.start,
                    segments,
                    closed: false,
                    from_current: false,
                };
            }
        }
        if chain.segments.len() > 1 && near(chain.start, chain.end()) {
            chain.closed = true;
        }
        out.push(chain);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversing_and_joining_paths() {
        let pt = Point::new;
        // a square drawn as four loose lines, two of them backwards
        let line = |a: Point, b: Point| Path::polyline(&[a, b], false);
        let sides = vec![
            line(pt(0.0, 0.0), pt(10.0, 0.0)),
            line(pt(10.0, 10.0), pt(10.0, 0.0)),
            line(pt(0.0, 10.0), pt(10.0, 10.0)),
            line(pt(0.0, 0.0), pt(0.0, 10.0)),
        ];
        let joined = join_paths(sides, JOIN_TOLERANCE_MM);
        assert_eq!(joined.len(), 1);
        let sq = &joined[0];
        assert!(sq.closed);
        assert_eq!(sq.segments.len(), 4);
        assert!(sq.end().distance(sq.start) < 1e-9);
        assert!((sq.length() - 40.0).abs() < 1e-9);
        // ends within the tolerance join; farther ones do not
        let near = join_paths(
            vec![
                line(pt(0.0, 0.0), pt(5.0, 0.0)),
                line(pt(5.005, 0.0), pt(9.0, 0.0)),
            ],
            JOIN_TOLERANCE_MM,
        );
        assert_eq!(near.len(), 1);
        assert!(!near[0].closed);
        let apart = join_paths(
            vec![line(pt(0.0, 0.0), pt(5.0, 0.0)), line(pt(6.0, 0.0), pt(9.0, 0.0))],
            JOIN_TOLERANCE_MM,
        );
        assert_eq!(apart.len(), 2);
        // a path touching the chain's start is put before it
        let before = join_paths(
            vec![line(pt(5.0, 0.0), pt(9.0, 0.0)), line(pt(0.0, 0.0), pt(5.0, 0.0))],
            JOIN_TOLERANCE_MM,
        );
        assert_eq!(before.len(), 1);
        assert_eq!((before[0].start, before[0].end()), (pt(0.0, 0.0), pt(9.0, 0.0)));
        // closed paths are kept
        let closed = Path::polyline(&[pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0)], true);
        assert_eq!(join_paths(vec![closed.clone()], JOIN_TOLERANCE_MM), vec![closed]);
        // reversing keeps arcs and curves on the same points
        let mut p = Path::new(pt(0.0, 0.0));
        p.segments.push(Segment::Line { end: pt(10.0, 0.0) });
        p.segments.push(Segment::Arc {
            end: pt(10.0, 10.0),
            center: pt(10.0, 5.0),
            ccw: true,
        });
        p.segments.push(Segment::Cubic {
            c1: pt(8.0, 12.0),
            c2: pt(2.0, 12.0),
            end: pt(0.0, 10.0),
        });
        let r = p.reversed();
        assert_eq!((r.start, r.end()), (pt(0.0, 10.0), pt(0.0, 0.0)));
        assert!((r.length() - p.length()).abs() < 1e-6);
        assert_eq!(r.reversed(), p);
        let (a, b) = (p.to_polyline(0.001), r.to_polyline(0.001));
        assert!(
            (p.bounds().max_x - r.bounds().max_x).abs() < 1e-6,
            "{:?} {:?}",
            a.len(),
            b.len()
        );
    }

    #[test]
    fn sweep_never_a_full_turn_unless_closed() {
        let c = Point::ZERO;
        let s = Point::new(1.0, 0.0);
        assert!((arc_sweep(s, Point::new(0.0, 1.0), c, true) - PI / 2.0).abs() < 1e-12);
        assert!((arc_sweep(s, Point::new(0.0, 1.0), c, false) + 3.0 * PI / 2.0).abs() < 1e-12);
        assert!((arc_sweep(s, s, c, true) - 2.0 * PI).abs() < 1e-12);
        assert!((arc_sweep(s, s, c, false) + 2.0 * PI).abs() < 1e-12);
    }

    #[test]
    fn polyline_bounds_and_length_of_a_circle() {
        let mut p = Path::new(Point::new(1.0, 0.0));
        p.segments
            .push(Segment::arc(Point::new(1.0, 0.0), Point::ZERO, true));
        let poly = p.to_polyline(0.01);
        assert!(poly.len() > 8);
        assert!(poly.iter().all(|q| (q.distance(Point::ZERO) - 1.0).abs() < 1e-9));
        let b = p.bounds();
        assert!((b.min_x + 1.0).abs() < 1e-9 && (b.max_y - 1.0).abs() < 1e-9 && (b.min_y + 1.0).abs() < 1e-9);
        assert!((p.length() - 2.0 * PI).abs() < 1e-9);
        // a radius smaller than the tolerance uses 45° steps
        let mut tiny = Path::new(Point::new(0.001, 0.0));
        tiny.segments
            .push(Segment::arc(Point::new(-0.001, 0.0), Point::ZERO, true));
        assert_eq!(tiny.to_polyline(0.01).len(), 5);
        assert_eq!(arc_steps(f64::NAN, 1.0), 1);
    }

    #[test]
    fn polylines_curves_and_transforms() {
        let sq = Path::polyline(
            &[
                Point::ZERO,
                Point::new(2.0, 0.0),
                Point::new(2.0, 2.0),
                Point::new(0.0, 2.0),
            ],
            true,
        );
        assert_eq!(sq.segments.len(), 4);
        assert_eq!(sq.end(), Point::ZERO);
        assert!((sq.length() - 8.0).abs() < 1e-12);
        let mut moved = sq.clone();
        moved.translate(1.0, 1.0);
        assert_eq!(
            moved.bounds(),
            Rect::from_points(Point::new(1.0, 1.0), Point::new(3.0, 3.0))
        );

        let mut c = Path::new(Point::ZERO);
        c.segments.push(Segment::Cubic {
            c1: Point::new(0.0, 1.0),
            c2: Point::new(1.0, 1.0),
            end: Point::new(1.0, 0.0),
        });
        assert!(c.to_polyline(0.01).len() > 2);
        assert!(c.bounds().max_y > 0.5 && c.bounds().max_y <= 1.0);
        assert!(c.length() > 1.0);
        c.translate(1.0, 0.0);
        assert_eq!(c.start, Point::new(1.0, 0.0));

        let mut arc = Path::new(Point::new(1.0, 0.0));
        arc.segments
            .push(Segment::arc(Point::new(0.0, 1.0), Point::ZERO, true));
        arc.translate(0.0, 0.0);
        let mirrored = arc.transformed(&Matrix::scaling(1.0, -1.0), 0.01);
        assert!(matches!(mirrored.segments[0], Segment::Arc { ccw: false, .. }));
        let squashed = arc.transformed(&Matrix::scaling(1.0, 0.5), 0.01);
        assert!(squashed.segments.len() > 1);
        assert!(
            squashed
                .segments
                .iter()
                .all(|s| matches!(s, Segment::Line { .. }))
        );
        let lines = sq.transformed(&Matrix::translation(1.0, 0.0), 0.01);
        assert_eq!(lines.start, Point::new(1.0, 0.0));
        let curve = c.transformed(&Matrix::IDENTITY, 0.01);
        assert_eq!(curve, c);
        assert_eq!(Path::polyline(&[], false).start, Point::ZERO);
    }
}
