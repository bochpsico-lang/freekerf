//! The part of a GDI+ `GraphicsPath` LaserGRBL relies on to turn curves into
//! polygons: `AddLine` and `AddBezier` (single precision; the start point is
//! dropped when it repeats the previous point) and `Flatten` as libgdiplus does it
//! (Sodipodi's `nr_curve_flatten`, flatness 0.25, at most 10 levels of
//! subdivision).

/// A point in single precision (`PointF`).
pub type PointF = (f32, f32);

/// `GraphicsPath.Flatten()`'s default flatness.
pub const DEFAULT_FLATNESS: f32 = 0.25;

/// libgdiplus' `FLATTEN_RECURSION_LIMIT`.
const RECURSION_LIMIT: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Line,
    Bezier,
}

/// One figure of a `GraphicsPath`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GdiPath {
    points: Vec<(PointF, Kind)>,
}

impl GdiPath {
    /// An empty path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn append(&mut self, p: PointF, kind: Kind, compress: bool) {
        if compress && self.points.last().is_some_and(|(q, _)| *q == p) {
            return;
        }
        self.points.push((p, kind));
    }

    /// `AddLine(a, b)`.
    pub fn add_line(&mut self, a: PointF, b: PointF) {
        self.append(a, Kind::Line, true);
        self.append(b, Kind::Line, false);
    }

    /// `AddBezier(a, c1, c2, b)`.
    pub fn add_bezier(&mut self, a: PointF, c1: PointF, c2: PointF, b: PointF) {
        self.append(a, Kind::Line, true);
        self.append(c1, Kind::Bezier, false);
        self.append(c2, Kind::Bezier, false);
        self.append(b, Kind::Bezier, false);
    }

    /// The points (`PathPoints`) after `Flatten(flatness)`. A curve that needs
    /// more than 10 levels of subdivision turns the whole path into four points at
    /// the origin, as libgdiplus does.
    #[must_use]
    pub fn flatten(&self, flatness: f32) -> Vec<PointF> {
        let mut out = Vec::with_capacity(self.points.len());
        let mut i = 0;
        while i < self.points.len() {
            let (p, kind) = self.points[i];
            if kind == Kind::Bezier {
                if i == 0 || i + 2 >= self.points.len() {
                    // bad path data: libgdiplus gives up on the whole path
                    return vec![(0.0, 0.0); 4];
                }
                let start = self.points[i - 1].0;
                let (c1, c2, end) = (p, self.points[i + 1].0, self.points[i + 2].0);
                if !curve(start, c1, c2, end, flatness.abs(), 0, &mut out) {
                    return vec![(0.0, 0.0); 4];
                }
                i += 3;
            } else {
                out.push(p);
                i += 1;
            }
        }
        out
    }
}

/// `nr_curve_flatten`: appends the end points of the flat enough pieces.
#[allow(clippy::many_single_char_names, clippy::similar_names)]
fn curve(
    p0: PointF,
    p1: PointF,
    p2: PointF,
    p3: PointF,
    flatness: f32,
    level: u32,
    out: &mut Vec<PointF>,
) -> bool {
    let ((x0, y0), (x1, y1), (x2, y2), (x3, y3)) = (p0, p1, p2, p3);
    let (dx1_0, dy1_0) = (x1 - x0, y1 - y0);
    let (dx2_0, dy2_0) = (x2 - x0, y2 - y0);
    let (dx3_0, dy3_0) = (x3 - x0, y3 - y0);
    let (dx2_3, dy2_3) = (x3 - x2, y3 - y2);
    let f2 = flatness;
    let d3_0_2 = dx3_0 * dx3_0 + dy3_0 * dy3_0;
    let subdivide = if d3_0_2 < f2 {
        let d1_0_2 = dx1_0 * dx1_0 + dy1_0 * dy1_0;
        let d2_0_2 = dx2_0 * dx2_0 + dy2_0 * dy2_0;
        !(d1_0_2 < f2 && d2_0_2 < f2)
    } else {
        let f2_q = f2 * d3_0_2;
        let s1_q = dx1_0 * dx3_0 + dy1_0 * dy3_0;
        let t1_q = dy1_0 * dx3_0 - dx1_0 * dy3_0;
        let s2_q = dx2_0 * dx3_0 + dy2_0 * dy3_0;
        let t2_q = dy2_0 * dx3_0 - dx2_0 * dy3_0;
        let v2_q = dx2_3 * dx3_0 + dy2_3 * dy3_0;
        t1_q * t1_q > f2_q
            || t2_q * t2_q > f2_q
            || (s1_q < 0.0 && s1_q * s1_q > f2_q)
            || (v2_q < 0.0 && v2_q * v2_q > f2_q)
            || s1_q >= s2_q
    };
    if !subdivide {
        out.push(p3);
        return true;
    }
    if level >= RECURSION_LIMIT {
        return false;
    }
    let x00t = (x0 + x1) * 0.5;
    let y00t = (y0 + y1) * 0.5;
    let x0tt = (x0 + 2.0 * x1 + x2) * 0.25;
    let y0tt = (y0 + 2.0 * y1 + y2) * 0.25;
    let x1tt = (x1 + 2.0 * x2 + x3) * 0.25;
    let y1tt = (y1 + 2.0 * y2 + y3) * 0.25;
    let x11t = (x2 + x3) * 0.5;
    let y11t = (y2 + y3) * 0.5;
    let xttt = (x0tt + x1tt) * 0.5;
    let yttt = (y0tt + y1tt) * 0.5;
    curve(
        p0,
        (x00t, y00t),
        (x0tt, y0tt),
        (xttt, yttt),
        flatness,
        level + 1,
        out,
    ) && curve(
        (xttt, yttt),
        (x1tt, y1tt),
        (x11t, y11t),
        p3,
        flatness,
        level + 1,
        out,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_compressed_and_curves_flattened() {
        let mut p = GdiPath::new();
        p.add_line((0.0, 0.0), (10.0, 0.0));
        p.add_line((10.0, 0.0), (10.0, 10.0));
        p.add_bezier((10.0, 10.0), (10.0, 20.0), (0.0, 20.0), (0.0, 10.0));
        let flat = p.flatten(DEFAULT_FLATNESS);
        assert_eq!(flat[..3], [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        assert!(flat.len() > 6);
        assert_eq!(*flat.last().unwrap(), (0.0, 10.0));
        // a degenerate (tiny) curve is a single point
        let mut t = GdiPath::new();
        t.add_bezier((0.0, 0.0), (0.1, 0.0), (0.2, 0.0), (0.3, 0.0));
        assert_eq!(t.flatten(DEFAULT_FLATNESS), [(0.0, 0.0), (0.3, 0.0)]);
        // a curve that cannot be flattened in 10 levels
        let mut h = GdiPath::new();
        h.add_bezier((0.0, 0.0), (1e9, 1e9), (-1e9, 1e9), (0.0, 0.0));
        assert_eq!(h.flatten(1e-9), [(0.0, 0.0); 4]);
        // a curve point without its start is bad data
        let bad = GdiPath {
            points: vec![((0.0, 0.0), Kind::Bezier)],
        };
        assert_eq!(bad.flatten(DEFAULT_FLATNESS), [(0.0, 0.0); 4]);
    }
}
