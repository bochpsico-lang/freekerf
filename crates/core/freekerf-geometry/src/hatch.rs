//! Hatch fillings: the line patterns LaserGRBL engraves inside closed shapes
//! (`CsPotrace/PotraceClipper.cs`). The pattern is generated over a `width × height`
//! box starting at the origin and clipped (even-odd) by the shapes with [`crate::clipper`].
//!
//! Coordinates are scaled by 1000 and truncated to integers exactly as LaserGRBL
//! does, and the shapes go through `f32` first (they are `PointF` in a GDI+
//! `GraphicsPath` there), so the lines match LaserGRBL vertex for vertex.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Point;
use crate::clipper::{
    ClipType, Clipper, ClipperError, ClipperOffset, EndType, IntPoint, JoinType, Path, Paths, PolyFillType,
    PolyType, int_point,
};

/// Scale between units and the integer grid of the clipper.
const RESOLUTION: f64 = 1000.0;

/// The filling patterns (LaserGRBL's `Direction.New*` values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FillPattern {
    /// Horizontal lines.
    Horizontal,
    /// Vertical lines.
    Vertical,
    /// Lines at 45° (up to the left).
    Diagonal,
    /// Lines at -45°.
    ReverseDiagonal,
    /// Horizontal and vertical lines.
    Grid,
    /// Both diagonals.
    DiagonalGrid,
    /// Small `+` marks.
    Cross,
    /// Small `×` marks.
    DiagonalCross,
    /// Small diamonds.
    Squares,
    /// Zig-zag lines.
    ZigZag,
    /// Hilbert curve tiles.
    Hilbert,
    /// Concentric insets of the shape outline.
    Inset,
}

/// A shape point as LaserGRBL feeds it to the clipper: `(cInt)((float)x * 1000.0)`.
#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
fn shape_point(p: Point) -> IntPoint {
    let (x, y) = (p.x as f32, p.y as f32);
    int_point(f64::from(x) * RESOLUTION, f64::from(y) * RESOLUTION)
}

/// The shapes as closed clipper paths: consecutive identical points are merged
/// like `GraphicsPath.AddLine` does with the start of each line.
fn shape_paths(shapes: &[Vec<Point>]) -> Paths {
    shapes
        .iter()
        .map(|s| {
            let mut out: Vec<(f32, f32)> = Vec::with_capacity(s.len() + 1);
            // AddLine(a, b) for every edge of the closed shape: `a` is dropped when it
            // equals the previous point, `b` is always added
            let n = s.len();
            #[allow(clippy::cast_possible_truncation)]
            let f = |p: Point| (p.x as f32, p.y as f32);
            for i in 0..n {
                let (a, b) = (f(s[i]), f(s[(i + 1) % n]));
                if out.last() != Some(&a) {
                    out.push(a);
                }
                out.push(b);
            }
            out.into_iter()
                .map(|(x, y)| int_point(f64::from(x) * RESOLUTION, f64::from(y) * RESOLUTION))
                .collect()
        })
        .collect()
}

/// Fills closed `shapes` (polylines, implicitly closed; even-odd: nested shapes are
/// holes) with `pattern` at `step` between lines, over the box `(0,0)-(width,height)`.
/// Returns polylines in the same units (on the 1/1000 grid).
///
/// # Errors
/// Coordinates out of the clipper's range.
pub fn fill(
    shapes: &[Vec<Point>],
    width: f64,
    height: f64,
    pattern: FillPattern,
    step: f64,
) -> Result<Vec<Vec<Point>>, ClipperError> {
    if pattern == FillPattern::Inset {
        return inset(shapes, step);
    }
    let mut c = Clipper::new();
    c.add_paths(&grid(width, height, pattern, step), PolyType::Subject, false)?;
    c.add_paths(&shape_paths(shapes), PolyType::Clip, true)?;
    let Some(tree) = c.execute_tree(
        ClipType::Intersection,
        PolyFillType::EvenOdd,
        PolyFillType::EvenOdd,
    )?
    else {
        return Ok(Vec::new());
    };
    Ok(to_polylines(&tree.open_paths(), false))
}

fn inset(shapes: &[Vec<Point>], spacing: f64) -> Result<Vec<Vec<Point>>, ClipperError> {
    let mut out = Vec::new();
    let mut c = ClipperOffset::default();
    c.add_paths(&shape_paths(shapes), JoinType::Round, EndType::ClosedPolygon);
    let mut i = 1.0;
    loop {
        let solution = c.execute(-spacing * i * RESOLUTION)?;
        if solution.is_empty() {
            break;
        }
        out.extend(to_polylines(&solution, true));
        i += 1.0;
    }
    Ok(out)
}

fn to_polylines(paths: &Paths, close: bool) -> Vec<Vec<Point>> {
    #[allow(clippy::cast_precision_loss)]
    let p = |ip: &IntPoint| Point::new(ip.x as f64 / RESOLUTION, ip.y as f64 / RESOLUTION);
    paths
        .iter()
        .map(|pg| {
            let mut v: Vec<Point> = pg.iter().map(p).collect();
            if close && pg.len() > 2 {
                v.push(p(&pg[0]));
            }
            v
        })
        .collect()
}

fn segment(target: &mut Paths, x1: f64, y1: f64, x2: f64, y2: f64, invert: bool) {
    let a = int_point(x1 * RESOLUTION, y1 * RESOLUTION);
    let b = int_point(x2 * RESOLUTION, y2 * RESOLUTION);
    target.push(if invert { vec![b, a] } else { vec![a, b] });
}

fn pt(x: f64, y: f64) -> IntPoint {
    int_point(x * RESOLUTION, y * RESOLUTION)
}

/// The pattern lines over the box (`AddGridSubject`).
#[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
fn grid(w: f64, h: f64, dir: FillPattern, step: f64) -> Paths {
    use FillPattern as D;
    let dstep = step * 2.0_f64.sqrt(); // step for diagonal (1.414)
    let rdstep = step * (1.0 / 2.0_f64.sqrt());
    let mut paths: Paths = Vec::new();

    if matches!(dir, D::Horizontal | D::Grid) {
        let mut y = 1;
        while f64::from(y) < h / step {
            let yy = f64::from(y) * step;
            segment(&mut paths, 0.0, yy, w, yy, y % 2 == 1);
            y += 1;
        }
    }
    if matches!(dir, D::Vertical | D::Grid) {
        let mut x = 1;
        while f64::from(x) < w / step {
            let xx = f64::from(x) * step;
            segment(&mut paths, xx, 0.0, xx, h, x % 2 == 0);
            x += 1;
        }
    }
    if matches!(dir, D::Diagonal | D::DiagonalGrid) {
        let mut i = 0;
        while f64::from(i) < (w + h) / dstep {
            let d = f64::from(i) * dstep;
            segment(&mut paths, 0.0, d, d, 0.0, i % 2 == 0);
            i += 1;
        }
    }
    if matches!(dir, D::ReverseDiagonal | D::DiagonalGrid) {
        let mut i = 0;
        while f64::from(i) < (w + h) / dstep {
            let d = f64::from(i) * dstep;
            segment(&mut paths, 0.0, h - d, d, h, i % 2 == 1);
            i += 1;
        }
    }
    if dir == D::Cross {
        let cl = step / 3.0; // cross len
        let mut y = 1;
        while f64::from(y) < h / step {
            let mut x = 1;
            while f64::from(x) < w / step {
                let (xs, ys) = (f64::from(x) * step, f64::from(y) * step);
                segment(&mut paths, xs - cl, ys, xs + cl, ys, y % 2 == 1);
                segment(&mut paths, xs, ys - cl, xs, ys + cl, x % 2 == 0);
                x += 1;
            }
            y += 1;
        }
    }
    if dir == D::DiagonalCross {
        let cl = rdstep / 3.0;
        let mut y = 0;
        while f64::from(y) < (h / step) + 1.0 {
            let mut x = 0;
            while f64::from(x) < (w / step) + 1.0 {
                let (xs, ys) = (f64::from(x) * step, f64::from(y) * step);
                segment(&mut paths, xs - cl, ys - cl, xs + cl, ys + cl, x % 2 == 1);
                segment(&mut paths, xs + cl, ys - cl, xs - cl, ys + cl, x % 2 == 1);
                x += 1;
            }
            y += 1;
        }
    }
    if dir == D::Squares {
        let cl = step / 3.0;
        let mut y = 0;
        while f64::from(y) < (h / step) + 1.0 {
            let mut x = 0;
            while f64::from(x) < (w / step) + 1.0 {
                let (xs, ys) = (f64::from(x) * step, f64::from(y) * step);
                paths.push(vec![
                    pt(xs - cl, ys),
                    pt(xs, ys + cl),
                    pt(xs + cl, ys),
                    pt(xs, ys - cl),
                    pt(xs - cl, ys),
                ]);
                x += 1;
            }
            y += 1;
        }
    }
    if dir == D::ZigZag {
        let hs = dstep / 2.0; // halfstep
        let mut i = 1;
        while f64::from(i) < (w + h) / dstep {
            let mut list: Path = Vec::new();
            if i % 2 == 1 {
                let mut my = 0.0;
                let mut mx = f64::from(i) * dstep - hs;
                list.push(pt(mx, my));
                while mx > 0.0 {
                    my += hs;
                    list.push(pt(mx, my));
                    mx -= hs;
                    list.push(pt(mx, my));
                }
            } else {
                let mut mx = 0.0;
                let mut my = f64::from(i) * dstep - hs;
                // LaserGRBL: "do not remove this +/-1, it seems to trick the clipper
                // algorithm into keeping my direction"
                list.push(pt(mx - 1.0, my + 1.0));
                list.push(pt(mx, my));
                while my > 0.0 {
                    mx += hs;
                    list.push(pt(mx, my));
                    my -= hs;
                    list.push(pt(mx, my));
                }
            }
            paths.push(list);
            i += 1;
        }
    }
    if dir == D::Hilbert {
        let n = 6;
        #[allow(clippy::cast_possible_truncation)]
        let ts = (2.0_f64.powi(n) * step) as f32;
        // a tile from n recursions on step: its size is 2^n * step
        #[allow(clippy::cast_possible_truncation)]
        let texel = hilbert(n, step as f32);
        let mut y = 0_i32;
        while f64::from(y) < h / f64::from(ts) {
            let mut x = 0_i32;
            while f64::from(x) < w / f64::from(ts) {
                #[allow(clippy::cast_precision_loss)]
                let list: Path = texel
                    .iter()
                    .map(|&(tx, ty)| {
                        let px = tx + x as f32 * ts;
                        let py = ty + y as f32 * ts;
                        pt(f64::from(px), f64::from(py))
                    })
                    .collect();
                paths.push(list);
                x += 1;
            }
            y += 1;
        }
    }
    paths
}

/// The points of a Hilbert curve of `depth` with steps of `delta` (single precision,
/// as LaserGRBL's `PointF` arithmetic).
fn hilbert(depth: i32, delta: f32) -> Vec<(f32, f32)> {
    fn rec(list: &mut Vec<(f32, f32)>, depth: i32, dx: f32, dy: f32) {
        let last = |l: &Vec<(f32, f32)>| *l.last().unwrap_or(&(0.0, 0.0));
        if depth > 1 {
            rec(list, depth - 1, dy, dx);
        }
        let (lx, ly) = last(list);
        list.push((lx + dx, ly + dy));
        if depth > 1 {
            rec(list, depth - 1, dx, dy);
        }
        let (lx, ly) = last(list);
        list.push((lx + dy, ly + dx));
        if depth > 1 {
            rec(list, depth - 1, dx, dy);
        }
        let (lx, ly) = last(list);
        list.push((lx - dx, ly - dy));
        if depth > 1 {
            rec(list, depth - 1, -dy, -dx);
        }
    }
    let mut rv = vec![(0.0, 0.0)];
    rec(&mut rv, depth, delta, 0.0);
    rv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    }

    #[test]
    fn horizontal_lines_inside_a_square_alternate_direction() {
        let lines = fill(
            &[square(1.0, 1.0, 9.0, 9.0)],
            10.0,
            10.0,
            FillPattern::Horizontal,
            1.0,
        )
        .unwrap();
        assert_eq!(lines.len(), 7); // y = 2..8 strictly inside, 1 and 9 on the border
        for l in &lines {
            assert_eq!(l.len(), 2);
            assert!(l.iter().all(|p| (1.0..=9.0).contains(&p.x)));
        }
    }

    #[test]
    fn holes_split_lines() {
        let lines = fill(
            &[square(0.0, 0.0, 10.0, 10.0), square(3.0, 3.0, 7.0, 7.0)],
            10.0,
            10.0,
            FillPattern::Vertical,
            1.0,
        )
        .unwrap();
        // x = 1..9; x = 4..6 are split by the hole (3 and 7 run along its border)
        assert!(lines.len() > 9);
    }

    #[test]
    fn every_pattern_produces_lines() {
        for p in [
            FillPattern::Horizontal,
            FillPattern::Vertical,
            FillPattern::Diagonal,
            FillPattern::ReverseDiagonal,
            FillPattern::Grid,
            FillPattern::DiagonalGrid,
            FillPattern::Cross,
            FillPattern::DiagonalCross,
            FillPattern::Squares,
            FillPattern::ZigZag,
            FillPattern::Hilbert,
            FillPattern::Inset,
        ] {
            let lines = fill(&[square(0.5, 0.5, 9.5, 9.5)], 10.0, 10.0, p, 0.5).unwrap();
            assert!(!lines.is_empty(), "{p:?}");
        }
        let insets = fill(
            &[square(0.0, 0.0, 10.0, 10.0)],
            10.0,
            10.0,
            FillPattern::Inset,
            1.0,
        )
        .unwrap();
        assert_eq!(insets.len(), 4);
        assert!(insets.iter().all(|l| l.first() == l.last()));
    }

    #[test]
    fn hilbert_curve_has_four_to_the_depth_points() {
        assert_eq!(hilbert(2, 1.0).len(), 16);
        assert_eq!(hilbert(6, 0.5).len(), 4096);
        assert_eq!(shape_point(Point::new(0.0015, 1.0)), IntPoint::new(1, 1000));
    }
}
