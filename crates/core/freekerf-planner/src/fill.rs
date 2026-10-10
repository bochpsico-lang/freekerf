//! Filling of closed shapes for colour layers (`SvgConverter/SvgFilling.cs`): open
//! paths whose ends touch are joined, the hatch pattern is generated around the
//! shapes with one line of margin, clipped (even-odd: nested shapes are holes) and
//! ordered nearest-neighbour.

use std::collections::HashMap;

use freekerf_geometry::hatch::{self, FillPattern};
use freekerf_geometry::{Point, order};

use crate::PlanError;

/// Ends closer than this (mm, on each axis) are the same point.
const JOIN_TOLERANCE: f64 = 0.05;

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() <= JOIN_TOLERANCE && (a.y - b.y).abs() <= JOIN_TOLERANCE
}

#[allow(clippy::cast_possible_truncation)]
fn cell(p: Point) -> (i64, i64) {
    (
        (p.x / JOIN_TOLERANCE).floor() as i64,
        (p.y / JOIN_TOLERANCE).floor() as i64,
    )
}

/// `JoinOpenPaths`: closed shapes as they are, open ones chained when their ends
/// touch (CAD files often store each side of a shape as its own path).
#[must_use]
pub fn join_open_paths(shapes: &[Vec<Point>]) -> Vec<Vec<Point>> {
    let mut rv = Vec::new();
    let mut open: Vec<&Vec<Point>> = Vec::new();
    for s in shapes {
        if s.len() < 2 {
            continue;
        }
        if near(s[0], s[s.len() - 1]) {
            rv.push(s.clone());
        } else {
            open.push(s);
        }
    }

    // index the endpoints of open paths on a grid, to find touching ends quickly
    let mut index: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, s) in open.iter().enumerate() {
        index.entry(cell(s[0])).or_default().push(i);
        index.entry(cell(s[s.len() - 1])).or_default().push(i);
    }
    let find = |used: &[bool], p: Point| -> Option<usize> {
        let (cx, cy) = cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = index.get(&(cx.saturating_add(dx), cy.saturating_add(dy))) {
                    for &i in list {
                        if !used[i] && (near(open[i][0], p) || near(open[i][open[i].len() - 1], p)) {
                            return Some(i);
                        }
                    }
                }
            }
        }
        None
    };

    let mut used = vec![false; open.len()];
    for i in 0..open.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut chain = open[i].clone();
        // grow the chain at the end, then at the start (by reversing it)
        for _ in 0..2 {
            while !near(chain[0], chain[chain.len() - 1]) {
                let Some(next) = find(&used, chain[chain.len() - 1]) else {
                    break;
                };
                used[next] = true;
                let mut segment = open[next].clone();
                if !near(segment[0], chain[chain.len() - 1]) {
                    segment.reverse();
                }
                chain.extend(segment.into_iter().skip(1));
            }
            chain.reverse();
        }
        rv.push(chain);
    }
    rv
}

/// `SvgFilling.Build`: the hatch polylines filling `shapes` (mm) with `pattern` at
/// `lines_per_mm`.
///
/// # Errors
/// Coordinates out of the clipper's range.
pub fn build(
    shapes: &[Vec<Point>],
    pattern: FillPattern,
    lines_per_mm: f64,
) -> Result<Vec<Vec<Point>>, PlanError> {
    let shapes: Vec<Vec<Point>> = join_open_paths(shapes)
        .into_iter()
        .filter(|s| s.len() >= 3)
        .collect();
    if shapes.is_empty() || lines_per_mm <= 0.0 {
        return Ok(Vec::new());
    }
    // the pattern is generated in the rectangle (0,0)-(w,h): move the shapes inside it, with one line of margin
    let step = 1.0 / lines_per_mm;
    let fold = |f: fn(f64, f64) -> f64, init: f64, get: fn(&Point) -> f64| {
        shapes
            .iter()
            .map(|s| s.iter().map(get).fold(init, f))
            .fold(init, f)
    };
    let min_x = fold(f64::min, f64::MAX, |p| p.x) - step;
    let min_y = fold(f64::min, f64::MAX, |p| p.y) - step;
    let w = fold(f64::max, f64::MIN, |p| p.x) - min_x + step;
    let h = fold(f64::max, f64::MIN, |p| p.y) - min_y + step;

    // every shape is closed (an edge from the last point back to the first)
    let moved: Vec<Vec<Point>> = shapes
        .iter()
        .map(|s| s.iter().map(|p| Point::new(p.x - min_x, p.y - min_y)).collect())
        .collect();
    let lines = hatch::fill(&moved, w, h, pattern, 1.0 / lines_per_mm)?;
    let lines: Vec<Vec<Point>> = lines.into_iter().filter(|l| !l.is_empty()).collect();
    let ends: Vec<(Point, Point)> = lines.iter().map(|l| (l[0], l[l.len() - 1])).collect();
    let ordered = order::apply(&lines, &order::nearest_neighbour_blocks(&ends));
    Ok(ordered
        .into_iter()
        .map(|l| {
            l.into_iter()
                .map(|p| Point::new(p.x + min_x, p.y + min_y))
                .collect()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Found by fuzzing: coordinates near `i64::MAX` cells overflowed the lookup.
    #[test]
    fn huge_coordinates_do_not_overflow() {
        let far = vec![Point::new(1e300, 2.0), Point::new(3.0, 1e300)];
        let other = vec![Point::new(-1e300, 0.0), Point::new(0.0, 1.0)];
        assert_eq!(join_open_paths(&[far, other]).len(), 2);
    }

    #[test]
    fn open_sides_are_joined_into_a_closed_shape() {
        let sides = vec![
            vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)],
            vec![Point::new(10.0, 10.0), Point::new(10.0, 0.01)],
            vec![Point::new(0.0, 10.0), Point::new(10.0, 10.0)],
            vec![Point::new(0.0, 10.0), Point::new(0.0, 0.0)],
            vec![Point::new(50.0, 50.0)],
        ];
        let joined = join_open_paths(&sides);
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].len(), 5);
        assert!(near(joined[0][0], joined[0][4]));
        let closed = vec![vec![
            Point::ZERO,
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::ZERO,
        ]];
        assert_eq!(join_open_paths(&closed), closed);
    }

    #[test]
    fn filling_a_square() {
        let sq = vec![vec![
            Point::ZERO,
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]];
        let lines = build(&sq, FillPattern::Horizontal, 1.0).unwrap();
        assert_eq!(lines.len(), 9);
        assert!(build(&sq, FillPattern::Horizontal, 0.0).unwrap().is_empty());
        assert!(
            build(
                &[vec![Point::ZERO, Point::new(1.0, 1.0)]],
                FillPattern::Horizontal,
                1.0
            )
            .unwrap()
            .is_empty()
        );
    }
}
