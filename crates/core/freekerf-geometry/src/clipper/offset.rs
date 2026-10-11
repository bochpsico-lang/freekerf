//! Polygon and polyline offsetting: a port of `ClipperOffset` of Clipper 6.4.2.

use super::core::{get_bounds, near_zero, orientation, round, trunc};
use super::{ClipType, Clipper, ClipperError, IntPoint, Path, Paths, PolyFillType, PolyType};

/// How corners are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    /// Squared off at distance `delta`.
    Square,
    /// Rounded (arc approximated within `arc_tolerance`).
    Round,
    /// Mitered up to `miter_limit`.
    Miter,
}

/// How path ends are treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndType {
    /// A closed polygon (offset inside or outside).
    ClosedPolygon,
    /// A closed line (both sides are offset).
    ClosedLine,
    /// An open path with butt ends.
    OpenButt,
    /// An open path with square ends.
    OpenSquare,
    /// An open path with round ends.
    OpenRound,
}

const TWO_PI: f64 = std::f64::consts::PI * 2.0;
const DEF_ARC_TOLERANCE: f64 = 0.25;

#[derive(Debug, Clone, Copy, Default)]
struct DoublePoint {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone)]
struct Node {
    polygon: Path,
    join_type: JoinType,
    end_type: EndType,
}

/// Offsets polygons and polylines (`ClipperLib.ClipperOffset`).
#[derive(Debug, Clone)]
pub struct ClipperOffset {
    nodes: Vec<Node>,
    lowest: Option<(usize, usize)>,
    dest_polys: Paths,
    src_poly: Path,
    dest_poly: Path,
    normals: Vec<DoublePoint>,
    delta: f64,
    sin_a: f64,
    sin: f64,
    cos: f64,
    miter_lim: f64,
    steps_per_rad: f64,
    /// Maximum distance of a rounded join from the true arc (in coordinate units).
    pub arc_tolerance: f64,
    /// Maximum miter length as a multiple of `delta`.
    pub miter_limit: f64,
}

impl Default for ClipperOffset {
    fn default() -> Self {
        Self::new(2.0, DEF_ARC_TOLERANCE)
    }
}

#[allow(clippy::cast_precision_loss)]
fn f(v: i64) -> f64 {
    v as f64
}

impl ClipperOffset {
    /// A new offsetter (`ClipperOffset(miterLimit, arcTolerance)`; defaults 2 and 0.25).
    #[must_use]
    pub const fn new(miter_limit: f64, arc_tolerance: f64) -> Self {
        Self {
            nodes: Vec::new(),
            lowest: None,
            dest_polys: Vec::new(),
            src_poly: Vec::new(),
            dest_poly: Vec::new(),
            normals: Vec::new(),
            delta: 0.0,
            sin_a: 0.0,
            sin: 0.0,
            cos: 0.0,
            miter_lim: 0.0,
            steps_per_rad: 0.0,
            arc_tolerance,
            miter_limit,
        }
    }

    /// Removes every path.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.lowest = None;
    }

    /// Adds a path to offset.
    pub fn add_path(&mut self, path: &[IntPoint], join_type: JoinType, end_type: EndType) {
        if path.is_empty() {
            return;
        }
        let mut high_i = path.len() - 1;
        // strip duplicate points from path and also get index to the lowest point ...
        if matches!(end_type, EndType::ClosedLine | EndType::ClosedPolygon) {
            while high_i > 0 && path[0] == path[high_i] {
                high_i -= 1;
            }
        }
        let mut polygon = Vec::with_capacity(high_i + 1);
        polygon.push(path[0]);
        let mut j = 0;
        let mut k = 0;
        for p in &path[1..=high_i] {
            if polygon[j] != *p {
                j += 1;
                polygon.push(*p);
                if p.y > polygon[k].y || (p.y == polygon[k].y && p.x < polygon[k].x) {
                    k = j;
                }
            }
        }
        if end_type == EndType::ClosedPolygon && j < 2 {
            return;
        }
        let lowest_pt = polygon[k];
        self.nodes.push(Node {
            polygon,
            join_type,
            end_type,
        });

        // if this path's lowest pt is lower than all the others then update m_lowest
        if end_type != EndType::ClosedPolygon {
            return;
        }
        let n = self.nodes.len() - 1;
        match self.lowest {
            None => self.lowest = Some((n, k)),
            Some((li, lk)) => {
                let ip = self.nodes[li].polygon[lk];
                if lowest_pt.y > ip.y || (lowest_pt.y == ip.y && lowest_pt.x < ip.x) {
                    self.lowest = Some((n, k));
                }
            }
        }
    }

    /// Adds several paths.
    pub fn add_paths(&mut self, paths: &[Path], join_type: JoinType, end_type: EndType) {
        for p in paths {
            self.add_path(p, join_type, end_type);
        }
    }

    fn fix_orientations(&mut self) {
        // fixup orientations of all closed paths if the orientation of the
        // closed path with the lowermost vertex is wrong ...
        if let Some((li, _)) = self.lowest
            && !orientation(&self.nodes[li].polygon)
        {
            for node in &mut self.nodes {
                if node.end_type == EndType::ClosedPolygon
                    || (node.end_type == EndType::ClosedLine && orientation(&node.polygon))
                {
                    node.polygon.reverse();
                }
            }
        } else {
            for node in &mut self.nodes {
                if node.end_type == EndType::ClosedLine && !orientation(&node.polygon) {
                    node.polygon.reverse();
                }
            }
        }
    }

    fn unit_normal(pt1: IntPoint, pt2: IntPoint) -> DoublePoint {
        let mut dx = f(pt2.x - pt1.x);
        let mut dy = f(pt2.y - pt1.y);
        if dx == 0.0 && dy == 0.0 {
            return DoublePoint::default();
        }
        let fct = 1.0 / (dx * dx + dy * dy).sqrt();
        dx *= fct;
        dy *= fct;
        DoublePoint { x: dy, y: -dx }
    }

    fn pt(&self, j: usize, n: DoublePoint, scale: f64) -> IntPoint {
        IntPoint::new(
            round(f(self.src_poly[j].x) + n.x * scale),
            round(f(self.src_poly[j].y) + n.y * scale),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn do_offset(&mut self, delta: f64) {
        self.dest_polys = Vec::new();
        self.delta = delta;

        // if Zero offset, just copy any CLOSED polygons to m_p and return ...
        if near_zero(delta) {
            for node in &self.nodes {
                if node.end_type == EndType::ClosedPolygon {
                    self.dest_polys.push(node.polygon.clone());
                }
            }
            return;
        }

        // see offset_triginometry3.svg in the documentation folder ...
        self.miter_lim = if self.miter_limit > 2.0 {
            2.0 / (self.miter_limit * self.miter_limit)
        } else {
            0.5
        };

        let y = if self.arc_tolerance <= 0.0 {
            DEF_ARC_TOLERANCE
        } else if self.arc_tolerance > delta.abs() * DEF_ARC_TOLERANCE {
            delta.abs() * DEF_ARC_TOLERANCE
        } else {
            self.arc_tolerance
        };
        // see offset_triginometry2.svg in the documentation folder ...
        let steps = std::f64::consts::PI / (1.0 - y / delta.abs()).acos();
        self.sin = (TWO_PI / steps).sin();
        self.cos = (TWO_PI / steps).cos();
        self.steps_per_rad = steps / TWO_PI;
        if delta < 0.0 {
            self.sin = -self.sin;
        }

        for ni in 0..self.nodes.len() {
            let node = self.nodes[ni].clone();
            self.src_poly = node.polygon;
            let len = self.src_poly.len();

            if len == 0 || (delta <= 0.0 && (len < 3 || node.end_type != EndType::ClosedPolygon)) {
                continue;
            }

            self.dest_poly = Vec::new();

            if len == 1 {
                if node.join_type == JoinType::Round {
                    let (mut x, mut y) = (1.0, 0.0);
                    let mut j = 1.0;
                    while j <= steps {
                        self.dest_poly.push(IntPoint::new(
                            round(f(self.src_poly[0].x) + x * delta),
                            round(f(self.src_poly[0].y) + y * delta),
                        ));
                        let x2 = x;
                        x = x * self.cos - self.sin * y;
                        y = x2 * self.sin + y * self.cos;
                        j += 1.0;
                    }
                } else {
                    let (mut x, mut y) = (-1.0, -1.0);
                    for _ in 0..4 {
                        self.dest_poly.push(IntPoint::new(
                            round(f(self.src_poly[0].x) + x * delta),
                            round(f(self.src_poly[0].y) + y * delta),
                        ));
                        if x < 0.0 {
                            x = 1.0;
                        } else if y < 0.0 {
                            y = 1.0;
                        } else {
                            x = -1.0;
                        }
                    }
                }
                self.dest_polys.push(std::mem::take(&mut self.dest_poly));
                continue;
            }

            // build m_normals ...
            self.normals.clear();
            for j in 0..len - 1 {
                self.normals
                    .push(Self::unit_normal(self.src_poly[j], self.src_poly[j + 1]));
            }
            if matches!(node.end_type, EndType::ClosedLine | EndType::ClosedPolygon) {
                self.normals
                    .push(Self::unit_normal(self.src_poly[len - 1], self.src_poly[0]));
            } else {
                self.normals.push(self.normals[len - 2]);
            }

            if node.end_type == EndType::ClosedPolygon {
                let mut k = len - 1;
                for j in 0..len {
                    self.offset_point(j, &mut k, node.join_type);
                }
                self.dest_polys.push(std::mem::take(&mut self.dest_poly));
            } else if node.end_type == EndType::ClosedLine {
                let mut k = len - 1;
                for j in 0..len {
                    self.offset_point(j, &mut k, node.join_type);
                }
                self.dest_polys.push(std::mem::take(&mut self.dest_poly));
                // re-build m_normals ...
                let n = self.normals[len - 1];
                for j in (1..len).rev() {
                    self.normals[j] = DoublePoint {
                        x: -self.normals[j - 1].x,
                        y: -self.normals[j - 1].y,
                    };
                }
                self.normals[0] = DoublePoint { x: -n.x, y: -n.y };
                k = 0;
                for j in (0..len).rev() {
                    self.offset_point(j, &mut k, node.join_type);
                }
                self.dest_polys.push(std::mem::take(&mut self.dest_poly));
            } else {
                let mut k = 0;
                for j in 1..len - 1 {
                    self.offset_point(j, &mut k, node.join_type);
                }

                if node.end_type == EndType::OpenButt {
                    let j = len - 1;
                    let n = self.normals[j];
                    self.dest_poly.push(self.pt(j, n, delta));
                    self.dest_poly.push(self.pt(j, n, -delta));
                } else {
                    let j = len - 1;
                    self.sin_a = 0.0;
                    self.normals[j] = DoublePoint {
                        x: -self.normals[j].x,
                        y: -self.normals[j].y,
                    };
                    if node.end_type == EndType::OpenSquare {
                        self.do_square(j, len - 2);
                    } else {
                        self.do_round(j, len - 2);
                    }
                }

                // re-build m_normals ...
                for j in (1..len).rev() {
                    self.normals[j] = DoublePoint {
                        x: -self.normals[j - 1].x,
                        y: -self.normals[j - 1].y,
                    };
                }
                self.normals[0] = DoublePoint {
                    x: -self.normals[1].x,
                    y: -self.normals[1].y,
                };

                k = len - 1;
                for j in (1..k).rev() {
                    self.offset_point(j, &mut k, node.join_type);
                }

                if node.end_type == EndType::OpenButt {
                    let n = self.normals[0];
                    self.dest_poly.push(self.pt(0, n, -delta));
                    self.dest_poly.push(self.pt(0, n, delta));
                } else {
                    self.sin_a = 0.0;
                    if node.end_type == EndType::OpenSquare {
                        self.do_square(0, 1);
                    } else {
                        self.do_round(0, 1);
                    }
                }
                self.dest_polys.push(std::mem::take(&mut self.dest_poly));
            }
        }
    }

    /// Offsets every path by `delta` (negative shrinks) into closed paths.
    ///
    /// # Errors
    /// Coordinates out of range.
    pub fn execute(&mut self, delta: f64) -> Result<Paths, ClipperError> {
        self.fix_orientations();
        self.do_offset(delta);
        // now clean up 'corners' ...
        let mut clpr = Clipper::new();
        clpr.add_paths(&self.dest_polys, PolyType::Subject, true)?;
        if delta > 0.0 {
            Ok(clpr
                .execute(ClipType::Union, PolyFillType::Positive, PolyFillType::Positive)?
                .unwrap_or_default())
        } else {
            let (left, top, right, bottom) = get_bounds(&self.dest_polys);
            let outer = vec![
                IntPoint::new(left - 10, bottom + 10),
                IntPoint::new(right + 10, bottom + 10),
                IntPoint::new(right + 10, top - 10),
                IntPoint::new(left - 10, top - 10),
            ];
            clpr.add_path(&outer, PolyType::Subject, true)?;
            clpr.reverse_solution = true;
            let mut solution = clpr
                .execute(ClipType::Union, PolyFillType::Negative, PolyFillType::Negative)?
                .unwrap_or_default();
            if !solution.is_empty() {
                solution.remove(0);
            }
            Ok(solution)
        }
    }

    fn offset_point(&mut self, j: usize, k: &mut usize, join_type: JoinType) {
        let (nk, nj) = (self.normals[*k], self.normals[j]);
        // cross product ...
        self.sin_a = nk.x * nj.y - nj.x * nk.y;

        if (self.sin_a * self.delta).abs() < 1.0 {
            // dot product ...
            let cos_a = nk.x * nj.x + nj.y * nk.y;
            if cos_a > 0.0 {
                // angle ==> 0 degrees
                self.dest_poly.push(self.pt(j, nk, self.delta));
                return;
            }
            // else angle ==> 180 degrees
        } else {
            self.sin_a = self.sin_a.clamp(-1.0, 1.0);
        }

        if self.sin_a * self.delta < 0.0 {
            self.dest_poly.push(self.pt(j, nk, self.delta));
            self.dest_poly.push(self.src_poly[j]);
            self.dest_poly.push(self.pt(j, nj, self.delta));
        } else {
            match join_type {
                JoinType::Miter => {
                    let r = 1.0 + (nj.x * nk.x + nj.y * nk.y);
                    if r >= self.miter_lim {
                        self.do_miter(j, *k, r);
                    } else {
                        self.do_square(j, *k);
                    }
                }
                JoinType::Square => self.do_square(j, *k),
                JoinType::Round => self.do_round(j, *k),
            }
        }
        *k = j;
    }

    fn do_square(&mut self, j: usize, k: usize) {
        let (nk, nj) = (self.normals[k], self.normals[j]);
        let dx = (self.sin_a.atan2(nk.x * nj.x + nk.y * nj.y) / 4.0).tan();
        let s = self.src_poly[j];
        self.dest_poly.push(IntPoint::new(
            round(f(s.x) + self.delta * (nk.x - nk.y * dx)),
            round(f(s.y) + self.delta * (nk.y + nk.x * dx)),
        ));
        self.dest_poly.push(IntPoint::new(
            round(f(s.x) + self.delta * (nj.x + nj.y * dx)),
            round(f(s.y) + self.delta * (nj.y - nj.x * dx)),
        ));
    }

    fn do_miter(&mut self, j: usize, k: usize, r: f64) {
        let (nk, nj) = (self.normals[k], self.normals[j]);
        let q = self.delta / r;
        let s = self.src_poly[j];
        self.dest_poly.push(IntPoint::new(
            round(f(s.x) + (nk.x + nj.x) * q),
            round(f(s.y) + (nk.y + nj.y) * q),
        ));
    }

    fn do_round(&mut self, j: usize, k: usize) {
        let (nk, nj) = (self.normals[k], self.normals[j]);
        let a = self.sin_a.atan2(nk.x * nj.x + nk.y * nj.y);
        let steps = round(self.steps_per_rad * a.abs()).max(1);
        let (mut x, mut y) = (nk.x, nk.y);
        let s = self.src_poly[j];
        for _ in 0..steps {
            self.dest_poly.push(IntPoint::new(
                round(f(s.x) + x * self.delta),
                round(f(s.y) + y * self.delta),
            ));
            let x2 = x;
            x = x * self.cos - self.sin * y;
            y = x2 * self.sin + y * self.cos;
        }
        self.dest_poly.push(IntPoint::new(
            round(f(s.x) + nj.x * self.delta),
            round(f(s.y) + nj.y * self.delta),
        ));
    }
}

/// `(cInt)` truncation of a value, re-exported for the callers that build points
/// from doubles the way `new IntPoint(double, double)` does.
#[must_use]
pub fn int_point(x: f64, y: f64) -> IntPoint {
    IntPoint::new(trunc(x), trunc(y))
}
