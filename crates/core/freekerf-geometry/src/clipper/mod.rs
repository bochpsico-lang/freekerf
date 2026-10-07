//! Integer polygon clipping and offsetting: a port of **Clipper 6.4.2** (C# version)
//! by Angus Johnson, © 2010-2017, under the Boost Software License 1.0 (see
//! `NOTICE`). LaserGRBL embeds the same version for its hatch fillings
//! (`PotraceClipper`); FreeKerf keeps the algorithm unchanged so that fillings
//! match LaserGRBL vertex for vertex ([ADR 0007](../../../../doc/adr/0007-clipping-library.md)).
//!
//! Coordinates are integers (`i64`): callers scale their millimetres (LaserGRBL
//! uses a factor of 1000). The friendlier `f64` API is [`crate::boolean`].

mod core;
mod offset;
mod sort;

pub use self::core::{Clipper, ClipperError, area, get_bounds, orientation};
pub use self::offset::{ClipperOffset, EndType, JoinType, int_point};

/// A point with integer coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct IntPoint {
    /// X.
    pub x: i64,
    /// Y.
    pub y: i64,
}

impl IntPoint {
    /// A point.
    #[must_use]
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// A path (open or closed).
pub type Path = Vec<IntPoint>;
/// Several paths.
pub type Paths = Vec<Path>;

/// The boolean operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipType {
    /// Subject AND clip.
    Intersection,
    /// Subject OR clip.
    Union,
    /// Subject AND NOT clip.
    Difference,
    /// Subject XOR clip.
    Xor,
}

/// Which side of the operation a path belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolyType {
    /// Subject (may be open).
    Subject,
    /// Clip (always closed).
    Clip,
}

/// Winding rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolyFillType {
    /// Odd winding numbers are inside.
    EvenOdd,
    /// Non-zero winding numbers are inside.
    NonZero,
    /// Positive winding numbers are inside.
    Positive,
    /// Negative winding numbers are inside.
    Negative,
}

/// A node of a [`PolyTree`]: a contour and the contours nested in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolyNode {
    parent: Option<usize>,
    /// The contour.
    pub contour: Path,
    /// Child nodes (indices in the tree).
    pub children: Vec<usize>,
    /// True for an open path (always a direct child of the root).
    pub is_open: bool,
}

/// The nested result of a clipping operation (`PolyTree`). Node 0 is the root,
/// which has no contour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolyTree {
    nodes: Vec<PolyNode>,
}

impl Default for PolyTree {
    fn default() -> Self {
        Self::new()
    }
}

impl PolyTree {
    /// Index of the root node.
    pub const ROOT: usize = 0;

    /// A tree with only the root.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: vec![PolyNode::default()],
        }
    }

    pub(crate) fn new_node(&mut self, contour: Path) -> usize {
        self.nodes.push(PolyNode {
            contour,
            ..PolyNode::default()
        });
        self.nodes.len() - 1
    }

    pub(crate) fn set_open(&mut self, node: usize) {
        self.nodes[node].is_open = true;
    }

    pub(crate) fn add_child(&mut self, parent: usize, child: usize) {
        self.nodes[parent].children.push(child);
        self.nodes[child].parent = Some(parent);
    }

    /// A node.
    #[must_use]
    pub fn node(&self, index: usize) -> &PolyNode {
        &self.nodes[index]
    }

    /// True for holes (an odd depth of closed ancestors).
    #[must_use]
    pub fn is_hole(&self, index: usize) -> bool {
        let mut result = true;
        let mut node = self.nodes[index].parent;
        while let Some(n) = node {
            result = !result;
            node = self.nodes[n].parent;
        }
        result
    }

    /// `Clipper.OpenPathsFromPolyTree`: the open paths, in order.
    #[must_use]
    pub fn open_paths(&self) -> Paths {
        self.nodes[Self::ROOT]
            .children
            .iter()
            .filter(|&&c| self.nodes[c].is_open)
            .map(|&c| self.nodes[c].contour.clone())
            .collect()
    }

    /// `Clipper.ClosedPathsFromPolyTree`: every closed contour, depth first.
    #[must_use]
    pub fn closed_paths(&self) -> Paths {
        let mut out = Vec::new();
        self.collect(Self::ROOT, &mut out);
        out
    }

    fn collect(&self, index: usize, out: &mut Paths) {
        let n = &self.nodes[index];
        if !n.contour.is_empty() && !n.is_open {
            out.push(n.contour.clone());
        }
        for &c in &n.children {
            self.collect(c, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: i64, y0: i64, x1: i64, y1: i64) -> Path {
        vec![
            IntPoint::new(x0, y0),
            IntPoint::new(x1, y0),
            IntPoint::new(x1, y1),
            IntPoint::new(x0, y1),
        ]
    }

    fn clip(op: ClipType, subj: &[Path], clp: &[Path]) -> Paths {
        let mut c = Clipper::new();
        c.add_paths(subj, PolyType::Subject, true).unwrap();
        c.add_paths(clp, PolyType::Clip, true).unwrap();
        c.execute(op, PolyFillType::EvenOdd, PolyFillType::EvenOdd)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn boolean_operations_on_two_squares() {
        let a = vec![rect(0, 0, 10, 10)];
        let b = vec![rect(5, 5, 15, 15)];
        let i = clip(ClipType::Intersection, &a, &b);
        assert_eq!(i.len(), 1);
        assert!((area(&i[0]).abs() - 25.0).abs() < 1e-9);
        let u = clip(ClipType::Union, &a, &b);
        assert_eq!(u.len(), 1);
        assert!((area(&u[0]).abs() - 175.0).abs() < 1e-9);
        let d = clip(ClipType::Difference, &a, &b);
        assert!((d.iter().map(|p| area(p)).sum::<f64>().abs() - 75.0).abs() < 1e-9);
        let x = clip(ClipType::Xor, &a, &b);
        assert!((x.iter().map(|p| area(p).abs()).sum::<f64>() - 150.0).abs() < 1e-9);
    }

    #[test]
    fn open_lines_are_clipped_by_a_square_with_a_hole() {
        let mut c = Clipper::new();
        let lines: Paths = (1..10)
            .map(|y| vec![IntPoint::new(-5, y * 10), IntPoint::new(105, y * 10)])
            .collect();
        c.add_paths(&lines, PolyType::Subject, false).unwrap();
        c.add_path(&rect(0, 0, 100, 100), PolyType::Clip, true).unwrap();
        c.add_path(&rect(30, 30, 70, 70), PolyType::Clip, true).unwrap();
        let tree = c
            .execute_tree(
                ClipType::Intersection,
                PolyFillType::EvenOdd,
                PolyFillType::EvenOdd,
            )
            .unwrap()
            .unwrap();
        let open = tree.open_paths();
        // lines at y = 40..60 are split in two by the hole (30 and 70 run along its border)
        assert!(open.len() >= 9 + 3);
        for p in &open {
            assert_eq!(p.len(), 2);
            assert!(p.iter().all(|pt| (0..=100).contains(&pt.x)));
        }
        assert!(tree.closed_paths().is_empty());
        assert!(
            c.execute(
                ClipType::Intersection,
                PolyFillType::EvenOdd,
                PolyFillType::EvenOdd
            )
            .is_err()
        );
    }

    #[test]
    fn polytree_nests_holes() {
        let mut c = Clipper::new();
        c.add_path(&rect(0, 0, 100, 100), PolyType::Subject, true)
            .unwrap();
        c.add_path(&rect(20, 20, 80, 80), PolyType::Subject, true)
            .unwrap();
        let tree = c
            .execute_tree(ClipType::Union, PolyFillType::EvenOdd, PolyFillType::EvenOdd)
            .unwrap()
            .unwrap();
        let root = tree.node(PolyTree::ROOT);
        assert_eq!(root.children.len(), 1);
        let outer = root.children[0];
        assert!(!tree.is_hole(outer));
        assert_eq!(tree.node(outer).children.len(), 1);
        assert!(tree.is_hole(tree.node(outer).children[0]));
        assert_eq!(tree.closed_paths().len(), 2);
    }

    #[test]
    fn offsets_shrink_and_grow() {
        let mut o = ClipperOffset::default();
        o.add_path(&rect(0, 0, 1000, 1000), JoinType::Round, EndType::ClosedPolygon);
        let inner = o.execute(-100.0).unwrap();
        assert_eq!(inner.len(), 1);
        assert!((area(&inner[0]).abs() - 800.0 * 800.0).abs() < 1.0);
        let outer = o.execute(100.0).unwrap();
        assert_eq!(outer.len(), 1);
        assert!(area(&outer[0]).abs() > 1000.0 * 1000.0);
        assert!(o.execute(-600.0).unwrap().is_empty());
        assert_eq!(o.execute(0.0).unwrap().len(), 1);
    }

    #[test]
    fn open_path_offsets_have_ends() {
        for (end, join) in [
            (EndType::OpenButt, JoinType::Miter),
            (EndType::OpenSquare, JoinType::Square),
            (EndType::OpenRound, JoinType::Round),
            (EndType::ClosedLine, JoinType::Round),
        ] {
            let mut o = ClipperOffset::new(3.0, 0.0);
            o.add_path(
                &[
                    IntPoint::new(0, 0),
                    IntPoint::new(1000, 0),
                    IntPoint::new(1000, 1000),
                ],
                join,
                end,
            );
            let r = o.execute(50.0).unwrap();
            assert!(!r.is_empty(), "{end:?}");
        }
        let mut o = ClipperOffset::default();
        o.add_path(&[IntPoint::new(5, 5)], JoinType::Round, EndType::OpenRound);
        o.add_path(&[IntPoint::new(500, 5)], JoinType::Square, EndType::OpenSquare);
        assert_eq!(o.execute(10.0).unwrap().len(), 2);
        o.clear();
        assert!(o.execute(10.0).unwrap().is_empty());
    }

    #[test]
    fn degenerate_and_invalid_inputs() {
        let mut c = Clipper::new();
        assert!(!c.add_path(&[], PolyType::Subject, true).unwrap());
        assert!(
            !c.add_path(
                &[IntPoint::new(1, 1), IntPoint::new(1, 1)],
                PolyType::Subject,
                true
            )
            .unwrap()
        );
        // collinear closed path: nothing left
        assert!(
            !c.add_path(
                &[IntPoint::new(0, 0), IntPoint::new(1, 1), IntPoint::new(2, 2)],
                PolyType::Subject,
                true
            )
            .unwrap()
        );
        assert_eq!(
            c.add_path(&[IntPoint::new(0, 0), IntPoint::new(1, 1)], PolyType::Clip, false),
            Err(ClipperError::OpenClip)
        );
        assert_eq!(
            c.add_path(&rect(0, 0, i64::MAX - 1, 10), PolyType::Subject, true),
            Err(ClipperError::Range)
        );
        // nothing to do
        let mut empty = Clipper::new();
        assert!(
            empty
                .execute(ClipType::Union, PolyFillType::NonZero, PolyFillType::NonZero)
                .unwrap()
                .is_none()
        );
        assert_eq!(get_bounds(&[]), (0, 0, 0, 0));
        assert!(orientation(&rect(0, 0, 1, 1)));
    }

    #[test]
    fn large_coordinates_use_full_range() {
        let big = 1_i64 << 40;
        let a = vec![rect(0, 0, big, big)];
        let b = vec![rect(big / 2, big / 2, big + big / 2, big + big / 2)];
        let i = clip(ClipType::Intersection, &a, &b);
        assert_eq!(i.len(), 1);
    }

    #[test]
    fn strictly_simple_splits_touching_polygons() {
        // a figure-eight touching at one vertex
        let p = vec![
            IntPoint::new(0, 0),
            IntPoint::new(10, 0),
            IntPoint::new(10, 10),
            IntPoint::new(20, 10),
            IntPoint::new(20, 20),
            IntPoint::new(10, 20),
            IntPoint::new(10, 10),
            IntPoint::new(0, 10),
        ];
        let mut c = Clipper::new();
        c.strictly_simple = true;
        c.add_path(&p, PolyType::Subject, true).unwrap();
        let r = c
            .execute(ClipType::Union, PolyFillType::NonZero, PolyFillType::NonZero)
            .unwrap()
            .unwrap();
        assert_eq!(r.len(), 2);
    }
}
