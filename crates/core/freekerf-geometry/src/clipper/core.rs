//! The polygon clipper: a port of `Clipper` (`ClipperBase` + `Clipper`) of
//! Clipper 6.4.2 by Angus Johnson, with open paths enabled (`use_lines`) and
//! 64-bit coordinates.
//!
//! Objects of the C# version (edges, output points, output records, poly nodes)
//! live in arenas here and refer to each other by index; [`NIL`] is `null`. The
//! algorithm, the order of every operation and every rounding are the original's,
//! so the output (vertices, their order and the order of the paths) is the same.

use std::collections::BTreeSet;

use super::sort::introspective_sort;
use super::{ClipType, IntPoint, Path, Paths, PolyFillType, PolyTree, PolyType};

/// `null` for arena indices.
pub(crate) const NIL: usize = usize::MAX;

pub(crate) const HORIZONTAL: f64 = -3.4E+38;
const SKIP: i32 = -2;
const UNASSIGNED: i32 = -1;
const TOLERANCE: f64 = 1.0E-20;
const LO_RANGE: i64 = 0x3FFF_FFFF;
const HI_RANGE: i64 = 0x3FFF_FFFF_FFFF_FFFF;

pub(crate) fn near_zero(val: f64) -> bool {
    val > -TOLERANCE && val < TOLERANCE
}

/// Error of a clipping operation (the `ClipperException` of the original).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipperError {
    /// A coordinate is larger than ±0x3FFFFFFFFFFFFFFF.
    #[error("coordinate outside allowed range")]
    Range,
    /// Open paths can only be subjects.
    #[error("open paths must be subject")]
    OpenClip,
    /// Open paths need a [`PolyTree`] solution.
    #[error("a PolyTree is needed for open path clipping")]
    OpenNeedsTree,
    /// Internal inconsistency of the sweep (`DoMaxima`, `UpdateEdgeIntoAEL`).
    #[error("clipping failed: {0}")]
    Internal(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    RightToLeft,
    LeftToRight,
}

#[derive(Debug, Clone)]
struct TEdge {
    bot: IntPoint,
    curr: IntPoint,
    top: IntPoint,
    delta: IntPoint,
    dx: f64,
    poly_typ: PolyType,
    side: EdgeSide,
    wind_delta: i32,
    wind_cnt: i32,
    wind_cnt2: i32,
    out_idx: i32,
    next: usize,
    prev: usize,
    next_in_lml: usize,
    next_in_ael: usize,
    prev_in_ael: usize,
    next_in_sel: usize,
    prev_in_sel: usize,
}

impl Default for TEdge {
    fn default() -> Self {
        Self {
            bot: IntPoint::default(),
            curr: IntPoint::default(),
            top: IntPoint::default(),
            delta: IntPoint::default(),
            dx: 0.0,
            poly_typ: PolyType::Subject,
            side: EdgeSide::Left,
            wind_delta: 0,
            wind_cnt: 0,
            wind_cnt2: 0,
            out_idx: 0,
            next: NIL,
            prev: NIL,
            next_in_lml: NIL,
            next_in_ael: NIL,
            prev_in_ael: NIL,
            next_in_sel: NIL,
            prev_in_sel: NIL,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct IntersectNode {
    edge1: usize,
    edge2: usize,
    pt: IntPoint,
}

#[derive(Debug, Clone, Copy)]
struct LocalMinima {
    y: i64,
    left_bound: usize,
    right_bound: usize,
    next: usize,
}

#[derive(Debug, Clone)]
struct OutRec {
    idx: i32,
    is_hole: bool,
    is_open: bool,
    first_left: usize,
    pts: usize,
    bottom_pt: usize,
    poly_node: usize,
}

#[derive(Debug, Clone, Copy)]
struct OutPt {
    idx: i32,
    pt: IntPoint,
    next: usize,
    prev: usize,
}

#[derive(Debug, Clone, Copy)]
struct Join {
    out_pt1: usize,
    out_pt2: usize,
    off_pt: IntPoint,
}

fn i128_mul(a: i64, b: i64) -> i128 {
    i128::from(a) * i128::from(b)
}

/// `(cInt)` of a C# `double`: truncation toward zero.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn trunc(v: f64) -> i64 {
    v as i64
}

/// `Clipper.Round`: half away from zero, then truncation.
pub(crate) fn round(value: f64) -> i64 {
    if value < 0.0 {
        trunc(value - 0.5)
    } else {
        trunc(value + 0.5)
    }
}

#[allow(clippy::cast_precision_loss)]
fn f(v: i64) -> f64 {
    v as f64
}

fn slopes_equal3(pt1: IntPoint, pt2: IntPoint, pt3: IntPoint, full: bool) -> bool {
    if full {
        i128_mul(pt1.y - pt2.y, pt2.x - pt3.x) == i128_mul(pt1.x - pt2.x, pt2.y - pt3.y)
    } else {
        (pt1.y - pt2.y).wrapping_mul(pt2.x - pt3.x) - (pt1.x - pt2.x).wrapping_mul(pt2.y - pt3.y) == 0
    }
}

fn slopes_equal4(pt1: IntPoint, pt2: IntPoint, pt3: IntPoint, pt4: IntPoint, full: bool) -> bool {
    if full {
        i128_mul(pt1.y - pt2.y, pt3.x - pt4.x) == i128_mul(pt1.x - pt2.x, pt3.y - pt4.y)
    } else {
        (pt1.y - pt2.y).wrapping_mul(pt3.x - pt4.x) - (pt1.x - pt2.x).wrapping_mul(pt3.y - pt4.y) == 0
    }
}

fn pt2_is_between_pt1_and_pt3(pt1: IntPoint, pt2: IntPoint, pt3: IntPoint) -> bool {
    if pt1 == pt3 || pt1 == pt2 || pt3 == pt2 {
        false
    } else if pt1.x != pt3.x {
        (pt2.x > pt1.x) == (pt2.x < pt3.x)
    } else {
        (pt2.y > pt1.y) == (pt2.y < pt3.y)
    }
}

fn horz_segments_overlap(mut seg1a: i64, mut seg1b: i64, mut seg2a: i64, mut seg2b: i64) -> bool {
    if seg1a > seg1b {
        std::mem::swap(&mut seg1a, &mut seg1b);
    }
    if seg2a > seg2b {
        std::mem::swap(&mut seg2a, &mut seg2b);
    }
    seg1a < seg2b && seg2a < seg1b
}

fn get_dx(pt1: IntPoint, pt2: IntPoint) -> f64 {
    if pt1.y == pt2.y {
        HORIZONTAL
    } else {
        f(pt2.x - pt1.x) / f(pt2.y - pt1.y)
    }
}

/// Signed area of a path (positive for the orientation Clipper calls "true").
#[must_use]
pub fn area(poly: &[IntPoint]) -> f64 {
    let cnt = poly.len();
    if cnt < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    let mut j = cnt - 1;
    for i in 0..cnt {
        a += (f(poly[j].x) + f(poly[i].x)) * (f(poly[j].y) - f(poly[i].y));
        j = i;
    }
    -a * 0.5
}

/// `Clipper.Orientation`: true when the area is not negative.
#[must_use]
pub fn orientation(poly: &[IntPoint]) -> bool {
    area(poly) >= 0.0
}

/// The polygon clipper (`ClipperLib.Clipper`).
#[derive(Debug, Default)]
pub struct Clipper {
    // ClipperBase
    edges: Vec<TEdge>,
    minima: Vec<LocalMinima>,
    minima_list: usize,
    current_lm: usize,
    scanbeam: BTreeSet<i64>,
    poly_outs: Vec<OutRec>,
    out_pts: Vec<OutPt>,
    active_edges: usize,
    use_full_range: bool,
    has_open_paths: bool,
    /// Keep collinear vertices of closed paths (only spikes are removed).
    pub preserve_collinear: bool,
    // Clipper
    clip_type: Option<ClipType>,
    maxima: Vec<i64>,
    sorted_edges: usize,
    intersect_list: Vec<IntersectNode>,
    intersect_capacity: usize,
    clip_fill_type: Option<PolyFillType>,
    subj_fill_type: Option<PolyFillType>,
    joins: Vec<Join>,
    ghost_joins: Vec<Join>,
    using_poly_tree: bool,
    /// Reverse the orientation of the output polygons.
    pub reverse_solution: bool,
    /// Split polygons at touching vertices.
    pub strictly_simple: bool,
}

impl Clipper {
    /// A new, empty clipper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            minima_list: NIL,
            current_lm: NIL,
            active_edges: NIL,
            sorted_edges: NIL,
            ..Self::default()
        }
    }

    fn subj_fill(&self) -> PolyFillType {
        self.subj_fill_type.unwrap_or(PolyFillType::EvenOdd)
    }

    fn clip_fill(&self) -> PolyFillType {
        self.clip_fill_type.unwrap_or(PolyFillType::EvenOdd)
    }

    fn ctype(&self) -> ClipType {
        self.clip_type.unwrap_or(ClipType::Intersection)
    }

    // ------------------------------------------------------------------ ClipperBase

    fn is_horizontal(&self, e: usize) -> bool {
        self.edges[e].delta.y == 0
    }

    fn slopes_equal_e(&self, e1: usize, e2: usize) -> bool {
        let (a, b) = (&self.edges[e1], &self.edges[e2]);
        if self.use_full_range {
            i128_mul(a.delta.y, b.delta.x) == i128_mul(a.delta.x, b.delta.y)
        } else {
            a.delta.y.wrapping_mul(b.delta.x) == a.delta.x.wrapping_mul(b.delta.y)
        }
    }

    fn range_test(pt: IntPoint, use_full_range: &mut bool) -> Result<(), ClipperError> {
        if *use_full_range {
            if pt.x > HI_RANGE || pt.y > HI_RANGE || -pt.x > HI_RANGE || -pt.y > HI_RANGE {
                return Err(ClipperError::Range);
            }
        } else if pt.x > LO_RANGE || pt.y > LO_RANGE || -pt.x > LO_RANGE || -pt.y > LO_RANGE {
            *use_full_range = true;
            Self::range_test(pt, use_full_range)?;
        }
        Ok(())
    }

    fn init_edge(&mut self, e: usize, e_next: usize, e_prev: usize, pt: IntPoint) {
        let ed = &mut self.edges[e];
        ed.next = e_next;
        ed.prev = e_prev;
        ed.curr = pt;
        ed.out_idx = UNASSIGNED;
    }

    fn init_edge2(&mut self, e: usize, poly_type: PolyType) {
        let next_curr = self.edges[self.edges[e].next].curr;
        let ed = &mut self.edges[e];
        if ed.curr.y >= next_curr.y {
            ed.bot = ed.curr;
            ed.top = next_curr;
        } else {
            ed.top = ed.curr;
            ed.bot = next_curr;
        }
        self.set_dx(e);
        self.edges[e].poly_typ = poly_type;
    }

    fn set_dx(&mut self, e: usize) {
        let ed = &mut self.edges[e];
        ed.delta.x = ed.top.x - ed.bot.x;
        ed.delta.y = ed.top.y - ed.bot.y;
        if ed.delta.y == 0 {
            ed.dx = HORIZONTAL;
        } else {
            ed.dx = f(ed.delta.x) / f(ed.delta.y);
        }
    }

    fn find_next_loc_min(&self, mut e: usize) -> usize {
        let ed = &self.edges;
        loop {
            while ed[e].bot != ed[ed[e].prev].bot || ed[e].curr == ed[e].top {
                e = ed[e].next;
            }
            if ed[e].dx != HORIZONTAL && ed[ed[e].prev].dx != HORIZONTAL {
                break;
            }
            while ed[ed[e].prev].dx == HORIZONTAL {
                e = ed[e].prev;
            }
            let e2 = e;
            while ed[e].dx == HORIZONTAL {
                e = ed[e].next;
            }
            if ed[e].top.y == ed[ed[e].prev].bot.y {
                continue; // ie just an intermediate horz.
            }
            if ed[ed[e2].prev].bot.x < ed[e].bot.x {
                e = e2;
            }
            break;
        }
        e
    }

    fn reverse_horizontal(&mut self, e: usize) {
        let ed = &mut self.edges[e];
        std::mem::swap(&mut ed.top.x, &mut ed.bot.x);
    }

    #[allow(clippy::too_many_lines)]
    fn process_bound(&mut self, mut e: usize, left_bound_is_forward: bool) -> usize {
        let mut result = e;
        if self.edges[result].out_idx == SKIP {
            // check if there are edges beyond the skip edge in the bound and if so
            // create another LocMin and calling ProcessBound once more ...
            e = result;
            if left_bound_is_forward {
                while self.edges[e].top.y == self.edges[self.edges[e].next].bot.y {
                    e = self.edges[e].next;
                }
                while e != result && self.edges[e].dx == HORIZONTAL {
                    e = self.edges[e].prev;
                }
            } else {
                while self.edges[e].top.y == self.edges[self.edges[e].prev].bot.y {
                    e = self.edges[e].prev;
                }
                while e != result && self.edges[e].dx == HORIZONTAL {
                    e = self.edges[e].next;
                }
            }
            if e == result {
                result = if left_bound_is_forward {
                    self.edges[e].next
                } else {
                    self.edges[e].prev
                };
            } else {
                // there are more edges in the bound beyond result starting with E
                e = if left_bound_is_forward {
                    self.edges[result].next
                } else {
                    self.edges[result].prev
                };
                let lm = LocalMinima {
                    y: self.edges[e].bot.y,
                    left_bound: NIL,
                    right_bound: e,
                    next: NIL,
                };
                self.edges[e].wind_delta = 0;
                result = self.process_bound(e, left_bound_is_forward);
                self.insert_local_minima(lm);
            }
            return result;
        }

        if self.edges[e].dx == HORIZONTAL {
            // We need to be careful with open paths because this may not be a
            // true local minima (ie E may be following a skip edge).
            // Also, consecutive horz. edges may start heading left before going right.
            let e_start = if left_bound_is_forward {
                self.edges[e].prev
            } else {
                self.edges[e].next
            };
            if self.edges[e_start].dx == HORIZONTAL {
                // ie an adjoining horizontal skip edge
                if self.edges[e_start].bot.x != self.edges[e].bot.x
                    && self.edges[e_start].top.x != self.edges[e].bot.x
                {
                    self.reverse_horizontal(e);
                }
            } else if self.edges[e_start].bot.x != self.edges[e].bot.x {
                self.reverse_horizontal(e);
            }
        }

        let e_start = e;
        if left_bound_is_forward {
            while self.edges[result].top.y == self.edges[self.edges[result].next].bot.y
                && self.edges[self.edges[result].next].out_idx != SKIP
            {
                result = self.edges[result].next;
            }
            if self.edges[result].dx == HORIZONTAL && self.edges[self.edges[result].next].out_idx != SKIP {
                // nb: at the top of a bound, horizontals are added to the bound
                // only when the preceding edge attaches to the horizontal's left vertex
                // unless a Skip edge is encountered when that becomes the top divide
                let mut horz = result;
                while self.edges[self.edges[horz].prev].dx == HORIZONTAL {
                    horz = self.edges[horz].prev;
                }
                if self.edges[self.edges[horz].prev].top.x > self.edges[self.edges[result].next].top.x {
                    result = self.edges[horz].prev;
                }
            }
            while e != result {
                self.edges[e].next_in_lml = self.edges[e].next;
                if self.edges[e].dx == HORIZONTAL
                    && e != e_start
                    && self.edges[e].bot.x != self.edges[self.edges[e].prev].top.x
                {
                    self.reverse_horizontal(e);
                }
                e = self.edges[e].next;
            }
            if self.edges[e].dx == HORIZONTAL
                && e != e_start
                && self.edges[e].bot.x != self.edges[self.edges[e].prev].top.x
            {
                self.reverse_horizontal(e);
            }
            result = self.edges[result].next; // move to the edge just beyond current bound
        } else {
            while self.edges[result].top.y == self.edges[self.edges[result].prev].bot.y
                && self.edges[self.edges[result].prev].out_idx != SKIP
            {
                result = self.edges[result].prev;
            }
            if self.edges[result].dx == HORIZONTAL && self.edges[self.edges[result].prev].out_idx != SKIP {
                let mut horz = result;
                while self.edges[self.edges[horz].next].dx == HORIZONTAL {
                    horz = self.edges[horz].next;
                }
                let hn = self.edges[horz].next;
                let rp = self.edges[result].prev;
                if self.edges[hn].top.x >= self.edges[rp].top.x {
                    result = hn;
                }
            }
            while e != result {
                self.edges[e].next_in_lml = self.edges[e].prev;
                if self.edges[e].dx == HORIZONTAL
                    && e != e_start
                    && self.edges[e].bot.x != self.edges[self.edges[e].next].top.x
                {
                    self.reverse_horizontal(e);
                }
                e = self.edges[e].prev;
            }
            if self.edges[e].dx == HORIZONTAL
                && e != e_start
                && self.edges[e].bot.x != self.edges[self.edges[e].next].top.x
            {
                self.reverse_horizontal(e);
            }
            result = self.edges[result].prev; // move to the edge just beyond current bound
        }
        result
    }

    /// Adds a path (`AddPath`). Returns false when it is degenerate (and ignored).
    ///
    /// # Errors
    /// Open clip paths and coordinates out of range.
    #[allow(clippy::too_many_lines)]
    pub fn add_path(
        &mut self,
        pg: &[IntPoint],
        poly_type: PolyType,
        closed: bool,
    ) -> Result<bool, ClipperError> {
        if !closed && poly_type == PolyType::Clip {
            return Err(ClipperError::OpenClip);
        }
        if pg.is_empty() {
            return Ok(false);
        }
        let mut high_i = pg.len() - 1;
        if closed {
            while high_i > 0 && pg[high_i] == pg[0] {
                high_i -= 1;
            }
        }
        while high_i > 0 && pg[high_i] == pg[high_i - 1] {
            high_i -= 1;
        }
        if (closed && high_i < 2) || (!closed && high_i < 1) {
            return Ok(false);
        }

        // create a new edge array ...
        let base = self.edges.len();
        self.edges.extend((0..=high_i).map(|_| TEdge::default()));
        let mut is_flat = true;

        // 1. Basic (first) edge initialization ...
        self.edges[base + 1].curr = pg[1];
        let mut full = self.use_full_range;
        Self::range_test(pg[0], &mut full)?;
        Self::range_test(pg[high_i], &mut full)?;
        self.use_full_range = full;
        self.init_edge(base, base + 1, base + high_i, pg[0]);
        self.init_edge(base + high_i, base, base + high_i - 1, pg[high_i]);
        for i in (1..high_i).rev() {
            let mut full = self.use_full_range;
            Self::range_test(pg[i], &mut full)?;
            self.use_full_range = full;
            self.init_edge(base + i, base + i + 1, base + i - 1, pg[i]);
        }
        let mut e_start = base;

        // 2. Remove duplicate vertices, and (when closed) collinear edges ...
        let mut e = e_start;
        let mut e_loop_stop = e_start;
        loop {
            // nb: allows matching start and end points when not Closed ...
            let en = self.edges[e].next;
            if self.edges[e].curr == self.edges[en].curr && (closed || en != e_start) {
                if e == en {
                    break;
                }
                if e == e_start {
                    e_start = en;
                }
                e = self.remove_edge(e);
                e_loop_stop = e;
                continue;
            }
            let ep = self.edges[e].prev;
            if ep == en {
                break; // only two vertices
            } else if closed
                && slopes_equal3(
                    self.edges[ep].curr,
                    self.edges[e].curr,
                    self.edges[en].curr,
                    self.use_full_range,
                )
                && (!self.preserve_collinear
                    || !pt2_is_between_pt1_and_pt3(
                        self.edges[ep].curr,
                        self.edges[e].curr,
                        self.edges[en].curr,
                    ))
            {
                // Collinear edges are allowed for open paths but in closed paths
                // the default is to merge adjacent collinear edges into a single edge.
                if e == e_start {
                    e_start = en;
                }
                e = self.remove_edge(e);
                e = self.edges[e].prev;
                e_loop_stop = e;
                continue;
            }
            e = self.edges[e].next;
            if e == e_loop_stop || (!closed && self.edges[e].next == e_start) {
                break;
            }
        }

        if (!closed && e == self.edges[e].next) || (closed && self.edges[e].prev == self.edges[e].next) {
            return Ok(false);
        }

        if !closed {
            self.has_open_paths = true;
            let p = self.edges[e_start].prev;
            self.edges[p].out_idx = SKIP;
        }

        // 3. Do second stage of edge initialization ...
        e = e_start;
        loop {
            self.init_edge2(e, poly_type);
            e = self.edges[e].next;
            if is_flat && self.edges[e].curr.y != self.edges[e_start].curr.y {
                is_flat = false;
            }
            if e == e_start {
                break;
            }
        }

        // 4. Finally, add edge bounds to LocalMinima list ...
        // Totally flat paths must be handled differently when adding them
        // to LocalMinima list to avoid endless loops etc ...
        if is_flat {
            if closed {
                return Ok(false);
            }
            let p = self.edges[e].prev;
            self.edges[p].out_idx = SKIP;
            let lm = LocalMinima {
                y: self.edges[e].bot.y,
                left_bound: NIL,
                right_bound: e,
                next: NIL,
            };
            self.edges[e].side = EdgeSide::Right;
            self.edges[e].wind_delta = 0;
            loop {
                if self.edges[e].bot.x != self.edges[self.edges[e].prev].top.x {
                    self.reverse_horizontal(e);
                }
                if self.edges[self.edges[e].next].out_idx == SKIP {
                    break;
                }
                self.edges[e].next_in_lml = self.edges[e].next;
                e = self.edges[e].next;
            }
            self.insert_local_minima(lm);
            return Ok(true);
        }

        let mut e_min = NIL;

        // workaround to avoid an endless loop in the while loop below when
        // open paths have matching start and end points ...
        let ep = self.edges[e].prev;
        if self.edges[ep].bot == self.edges[ep].top {
            e = self.edges[e].next;
        }

        loop {
            e = self.find_next_loc_min(e);
            if e == e_min {
                break;
            } else if e_min == NIL {
                e_min = e;
            }

            // E and E.Prev now share a local minima (left aligned if horizontal).
            // Compare their slopes to find which starts which bound ...
            let ep = self.edges[e].prev;
            let (mut lb, mut rb, left_bound_is_forward) = if self.edges[e].dx < self.edges[ep].dx {
                (ep, e, false)
            } else {
                (e, ep, true)
            };
            let y = self.edges[e].bot.y;
            self.edges[lb].side = EdgeSide::Left;
            self.edges[rb].side = EdgeSide::Right;

            if !closed {
                self.edges[lb].wind_delta = 0;
            } else if self.edges[lb].next == rb {
                self.edges[lb].wind_delta = -1;
            } else {
                self.edges[lb].wind_delta = 1;
            }
            self.edges[rb].wind_delta = -self.edges[lb].wind_delta;

            e = self.process_bound(lb, left_bound_is_forward);
            if self.edges[e].out_idx == SKIP {
                e = self.process_bound(e, left_bound_is_forward);
            }

            let mut e2 = self.process_bound(rb, !left_bound_is_forward);
            if self.edges[e2].out_idx == SKIP {
                e2 = self.process_bound(e2, !left_bound_is_forward);
            }

            if self.edges[lb].out_idx == SKIP {
                lb = NIL;
            } else if self.edges[rb].out_idx == SKIP {
                rb = NIL;
            }
            self.insert_local_minima(LocalMinima {
                y,
                left_bound: lb,
                right_bound: rb,
                next: NIL,
            });
            if !left_bound_is_forward {
                e = e2;
            }
        }
        Ok(true)
    }

    /// Adds several paths (`AddPaths`). True when at least one was added.
    ///
    /// # Errors
    /// As [`Clipper::add_path`].
    pub fn add_paths(
        &mut self,
        ppg: &[Path],
        poly_type: PolyType,
        closed: bool,
    ) -> Result<bool, ClipperError> {
        let mut result = false;
        for p in ppg {
            if self.add_path(p, poly_type, closed)? {
                result = true;
            }
        }
        Ok(result)
    }

    fn remove_edge(&mut self, e: usize) -> usize {
        // removes e from double_linked_list (but without removing from memory)
        let (prev, next) = (self.edges[e].prev, self.edges[e].next);
        self.edges[prev].next = next;
        self.edges[next].prev = prev;
        self.edges[e].prev = NIL;
        next
    }

    fn insert_local_minima(&mut self, mut new_lm: LocalMinima) {
        let idx = self.minima.len();
        if self.minima_list == NIL {
            self.minima.push(new_lm);
            self.minima_list = idx;
        } else if new_lm.y >= self.minima[self.minima_list].y {
            new_lm.next = self.minima_list;
            self.minima.push(new_lm);
            self.minima_list = idx;
        } else {
            let mut tmp = self.minima_list;
            while self.minima[tmp].next != NIL && new_lm.y < self.minima[self.minima[tmp].next].y {
                tmp = self.minima[tmp].next;
            }
            new_lm.next = self.minima[tmp].next;
            self.minima.push(new_lm);
            self.minima[tmp].next = idx;
        }
    }

    fn pop_local_minima(&mut self, y: i64) -> Option<LocalMinima> {
        if self.current_lm != NIL && self.minima[self.current_lm].y == y {
            let lm = self.minima[self.current_lm];
            self.current_lm = lm.next;
            return Some(lm);
        }
        None
    }

    fn reset(&mut self) {
        self.current_lm = self.minima_list;
        if self.current_lm == NIL {
            return; // ie nothing to process
        }
        // reset all edges ...
        self.scanbeam.clear();
        let mut lm = self.minima_list;
        while lm != NIL {
            let m = self.minima[lm];
            self.insert_scanbeam(m.y);
            for e in [m.left_bound, m.right_bound] {
                if e != NIL {
                    self.edges[e].curr = self.edges[e].bot;
                    self.edges[e].out_idx = UNASSIGNED;
                }
            }
            lm = m.next;
        }
        self.active_edges = NIL;
    }

    fn insert_scanbeam(&mut self, y: i64) {
        self.scanbeam.insert(y);
    }

    fn pop_scanbeam(&mut self) -> Option<i64> {
        self.scanbeam.pop_last()
    }

    fn local_minima_pending(&self) -> bool {
        self.current_lm != NIL
    }

    fn create_out_rec(&mut self) -> usize {
        let idx = self.poly_outs.len();
        self.poly_outs.push(OutRec {
            idx: i32::try_from(idx).unwrap_or(i32::MAX),
            is_hole: false,
            is_open: false,
            first_left: NIL,
            pts: NIL,
            bottom_pt: NIL,
            poly_node: NIL,
        });
        idx
    }

    fn new_out_pt(&mut self, idx: i32, pt: IntPoint) -> usize {
        let i = self.out_pts.len();
        self.out_pts.push(OutPt {
            idx,
            pt,
            next: i,
            prev: i,
        });
        i
    }

    fn update_edge_into_ael(&mut self, e: usize) -> Result<usize, ClipperError> {
        let nl = self.edges[e].next_in_lml;
        if nl == NIL {
            return Err(ClipperError::Internal("UpdateEdgeIntoAEL: invalid call"));
        }
        let ael_prev = self.edges[e].prev_in_ael;
        let ael_next = self.edges[e].next_in_ael;
        self.edges[nl].out_idx = self.edges[e].out_idx;
        if ael_prev == NIL {
            self.active_edges = nl;
        } else {
            self.edges[ael_prev].next_in_ael = nl;
        }
        if ael_next != NIL {
            self.edges[ael_next].prev_in_ael = nl;
        }
        self.edges[nl].side = self.edges[e].side;
        self.edges[nl].wind_delta = self.edges[e].wind_delta;
        self.edges[nl].wind_cnt = self.edges[e].wind_cnt;
        self.edges[nl].wind_cnt2 = self.edges[e].wind_cnt2;
        let e = nl;
        self.edges[e].curr = self.edges[e].bot;
        self.edges[e].prev_in_ael = ael_prev;
        self.edges[e].next_in_ael = ael_next;
        if !self.is_horizontal(e) {
            self.insert_scanbeam(self.edges[e].top.y);
        }
        Ok(e)
    }

    fn swap_positions_in_ael(&mut self, edge1: usize, edge2: usize) {
        // check that one or other edge hasn't already been removed from AEL ...
        if self.edges[edge1].next_in_ael == self.edges[edge1].prev_in_ael
            || self.edges[edge2].next_in_ael == self.edges[edge2].prev_in_ael
        {
            return;
        }
        if self.edges[edge1].next_in_ael == edge2 {
            let next = self.edges[edge2].next_in_ael;
            if next != NIL {
                self.edges[next].prev_in_ael = edge1;
            }
            let prev = self.edges[edge1].prev_in_ael;
            if prev != NIL {
                self.edges[prev].next_in_ael = edge2;
            }
            self.edges[edge2].prev_in_ael = prev;
            self.edges[edge2].next_in_ael = edge1;
            self.edges[edge1].prev_in_ael = edge2;
            self.edges[edge1].next_in_ael = next;
        } else if self.edges[edge2].next_in_ael == edge1 {
            let next = self.edges[edge1].next_in_ael;
            if next != NIL {
                self.edges[next].prev_in_ael = edge2;
            }
            let prev = self.edges[edge2].prev_in_ael;
            if prev != NIL {
                self.edges[prev].next_in_ael = edge1;
            }
            self.edges[edge1].prev_in_ael = prev;
            self.edges[edge1].next_in_ael = edge2;
            self.edges[edge2].prev_in_ael = edge1;
            self.edges[edge2].next_in_ael = next;
        } else {
            let next = self.edges[edge1].next_in_ael;
            let prev = self.edges[edge1].prev_in_ael;
            self.edges[edge1].next_in_ael = self.edges[edge2].next_in_ael;
            let n1 = self.edges[edge1].next_in_ael;
            if n1 != NIL {
                self.edges[n1].prev_in_ael = edge1;
            }
            self.edges[edge1].prev_in_ael = self.edges[edge2].prev_in_ael;
            let p1 = self.edges[edge1].prev_in_ael;
            if p1 != NIL {
                self.edges[p1].next_in_ael = edge1;
            }
            self.edges[edge2].next_in_ael = next;
            if next != NIL {
                self.edges[next].prev_in_ael = edge2;
            }
            self.edges[edge2].prev_in_ael = prev;
            if prev != NIL {
                self.edges[prev].next_in_ael = edge2;
            }
        }
        if self.edges[edge1].prev_in_ael == NIL {
            self.active_edges = edge1;
        } else if self.edges[edge2].prev_in_ael == NIL {
            self.active_edges = edge2;
        }
    }

    fn delete_from_ael(&mut self, e: usize) {
        let ael_prev = self.edges[e].prev_in_ael;
        let ael_next = self.edges[e].next_in_ael;
        if ael_prev == NIL && ael_next == NIL && e != self.active_edges {
            return; // already deleted
        }
        if ael_prev == NIL {
            self.active_edges = ael_next;
        } else {
            self.edges[ael_prev].next_in_ael = ael_next;
        }
        if ael_next != NIL {
            self.edges[ael_next].prev_in_ael = ael_prev;
        }
        self.edges[e].next_in_ael = NIL;
        self.edges[e].prev_in_ael = NIL;
    }

    // ------------------------------------------------------------------ Clipper

    fn insert_maxima(&mut self, x: i64) {
        // sorted ascending, ignoring dups
        if let Err(pos) = self.maxima.binary_search(&x) {
            self.maxima.insert(pos, x);
        }
    }

    /// Clips into a list of closed paths (`Execute(ClipType, Paths, ...)`).
    ///
    /// # Errors
    /// Open paths were added (they need a [`PolyTree`]), or the sweep failed.
    pub fn execute(
        &mut self,
        clip_type: ClipType,
        subj_fill_type: PolyFillType,
        clip_fill_type: PolyFillType,
    ) -> Result<Option<Paths>, ClipperError> {
        if self.has_open_paths {
            return Err(ClipperError::OpenNeedsTree);
        }
        self.subj_fill_type = Some(subj_fill_type);
        self.clip_fill_type = Some(clip_fill_type);
        self.clip_type = Some(clip_type);
        self.using_poly_tree = false;
        let succeeded = self.execute_internal()?;
        let result = succeeded.then(|| self.build_result());
        self.dispose_all_poly_pts();
        Ok(result)
    }

    /// Clips into a [`PolyTree`] (`Execute(ClipType, PolyTree, ...)`), needed for open paths.
    ///
    /// # Errors
    /// The sweep failed.
    pub fn execute_tree(
        &mut self,
        clip_type: ClipType,
        subj_fill_type: PolyFillType,
        clip_fill_type: PolyFillType,
    ) -> Result<Option<PolyTree>, ClipperError> {
        self.subj_fill_type = Some(subj_fill_type);
        self.clip_fill_type = Some(clip_fill_type);
        self.clip_type = Some(clip_type);
        self.using_poly_tree = true;
        let succeeded = self.execute_internal()?;
        let result = succeeded.then(|| self.build_result2());
        self.dispose_all_poly_pts();
        Ok(result)
    }

    fn fix_hole_linkage(&mut self, rec: usize) {
        // skip if an outermost polygon or
        // already already points to the correct FirstLeft ...
        let fl = self.poly_outs[rec].first_left;
        if fl == NIL
            || (self.poly_outs[rec].is_hole != self.poly_outs[fl].is_hole && self.poly_outs[fl].pts != NIL)
        {
            return;
        }
        let mut orfl = fl;
        while orfl != NIL
            && (self.poly_outs[orfl].is_hole == self.poly_outs[rec].is_hole
                || self.poly_outs[orfl].pts == NIL)
        {
            orfl = self.poly_outs[orfl].first_left;
        }
        self.poly_outs[rec].first_left = orfl;
    }

    fn execute_internal(&mut self) -> Result<bool, ClipperError> {
        let r = self.execute_sweep();
        self.joins.clear();
        self.ghost_joins.clear();
        r
    }

    fn execute_sweep(&mut self) -> Result<bool, ClipperError> {
        self.reset();
        self.sorted_edges = NIL;
        self.maxima.clear();

        let Some(mut bot_y) = self.pop_scanbeam() else {
            return Ok(false);
        };
        self.insert_local_minima_into_ael(bot_y)?;
        loop {
            let top = self.pop_scanbeam();
            if top.is_none() && !self.local_minima_pending() {
                break;
            }
            // `PopScanbeam` leaves Y = 0 when empty (local minima still pending)
            let top_y = top.unwrap_or(0);
            self.process_horizontals()?;
            self.ghost_joins.clear();
            if !self.process_intersections(top_y)? {
                return Ok(false);
            }
            self.process_edges_at_top_of_scanbeam(top_y)?;
            bot_y = top_y;
            self.insert_local_minima_into_ael(bot_y)?;
        }

        // fix orientations ...
        for i in 0..self.poly_outs.len() {
            let r = &self.poly_outs[i];
            if r.pts == NIL || r.is_open {
                continue;
            }
            if (r.is_hole ^ self.reverse_solution) == (self.area_rec(i) > 0.0) {
                self.reverse_poly_pt_links(self.poly_outs[i].pts);
            }
        }

        self.join_common_edges();

        for i in 0..self.poly_outs.len() {
            if self.poly_outs[i].pts == NIL {
                continue;
            } else if self.poly_outs[i].is_open {
                self.fixup_out_polyline(i);
            } else {
                self.fixup_out_polygon(i);
            }
        }

        if self.strictly_simple {
            self.do_simple_polygons();
        }
        Ok(true)
    }

    fn dispose_all_poly_pts(&mut self) {
        self.poly_outs.clear();
        self.out_pts.clear();
    }

    fn add_join(&mut self, op1: usize, op2: usize, off_pt: IntPoint) {
        self.joins.push(Join {
            out_pt1: op1,
            out_pt2: op2,
            off_pt,
        });
    }

    fn add_ghost_join(&mut self, op: usize, off_pt: IntPoint) {
        self.ghost_joins.push(Join {
            out_pt1: op,
            out_pt2: NIL,
            off_pt,
        });
    }

    #[allow(clippy::too_many_lines)]
    fn insert_local_minima_into_ael(&mut self, bot_y: i64) -> Result<(), ClipperError> {
        while let Some(lm) = self.pop_local_minima(bot_y) {
            let lb = lm.left_bound;
            let rb = lm.right_bound;

            let mut op1 = NIL;
            if lb == NIL {
                self.insert_edge_into_ael(rb, NIL);
                self.set_winding_count(rb);
                if self.is_contributing(rb) {
                    op1 = self.add_out_pt(rb, self.edges[rb].bot);
                }
            } else if rb == NIL {
                self.insert_edge_into_ael(lb, NIL);
                self.set_winding_count(lb);
                if self.is_contributing(lb) {
                    op1 = self.add_out_pt(lb, self.edges[lb].bot);
                }
                self.insert_scanbeam(self.edges[lb].top.y);
            } else {
                self.insert_edge_into_ael(lb, NIL);
                self.insert_edge_into_ael(rb, lb);
                self.set_winding_count(lb);
                self.edges[rb].wind_cnt = self.edges[lb].wind_cnt;
                self.edges[rb].wind_cnt2 = self.edges[lb].wind_cnt2;
                if self.is_contributing(lb) {
                    op1 = self.add_local_min_poly(lb, rb, self.edges[lb].bot);
                }
                self.insert_scanbeam(self.edges[lb].top.y);
            }

            if rb != NIL {
                if self.is_horizontal(rb) {
                    let nl = self.edges[rb].next_in_lml;
                    if nl != NIL {
                        self.insert_scanbeam(self.edges[nl].top.y);
                    }
                    self.add_edge_to_sel(rb);
                } else {
                    self.insert_scanbeam(self.edges[rb].top.y);
                }
            }

            if lb == NIL || rb == NIL {
                continue;
            }

            // if output polygons share an Edge with a horizontal rb, they'll need joining later ...
            if op1 != NIL
                && self.is_horizontal(rb)
                && !self.ghost_joins.is_empty()
                && self.edges[rb].wind_delta != 0
            {
                for i in 0..self.ghost_joins.len() {
                    // if the horizontal Rb and a 'ghost' horizontal overlap, then convert
                    // the 'ghost' join to a real join ready for later ...
                    let j = self.ghost_joins[i];
                    if horz_segments_overlap(
                        self.out_pts[j.out_pt1].pt.x,
                        j.off_pt.x,
                        self.edges[rb].bot.x,
                        self.edges[rb].top.x,
                    ) {
                        self.add_join(j.out_pt1, op1, j.off_pt);
                    }
                }
            }

            let lbp = self.edges[lb].prev_in_ael;
            if self.edges[lb].out_idx >= 0
                && lbp != NIL
                && self.edges[lbp].curr.x == self.edges[lb].bot.x
                && self.edges[lbp].out_idx >= 0
                && slopes_equal4(
                    self.edges[lbp].curr,
                    self.edges[lbp].top,
                    self.edges[lb].curr,
                    self.edges[lb].top,
                    self.use_full_range,
                )
                && self.edges[lb].wind_delta != 0
                && self.edges[lbp].wind_delta != 0
            {
                let op2 = self.add_out_pt(lbp, self.edges[lb].bot);
                self.add_join(op1, op2, self.edges[lb].top);
            }

            if self.edges[lb].next_in_ael != rb {
                let rbp = self.edges[rb].prev_in_ael;
                if self.edges[rb].out_idx >= 0
                    && self.edges[rbp].out_idx >= 0
                    && slopes_equal4(
                        self.edges[rbp].curr,
                        self.edges[rbp].top,
                        self.edges[rb].curr,
                        self.edges[rb].top,
                        self.use_full_range,
                    )
                    && self.edges[rb].wind_delta != 0
                    && self.edges[rbp].wind_delta != 0
                {
                    let op2 = self.add_out_pt(rbp, self.edges[rb].bot);
                    self.add_join(op1, op2, self.edges[rb].top);
                }

                let mut e = self.edges[lb].next_in_ael;
                if e != NIL {
                    while e != rb {
                        // nb: For calculating winding counts etc, IntersectEdges() assumes
                        // that param1 will be to the right of param2 ABOVE the intersection ...
                        let pt = self.edges[lb].curr;
                        self.intersect_edges(rb, e, pt); // order important here
                        e = self.edges[e].next_in_ael;
                    }
                }
            }
        }
        Ok(())
    }

    fn insert_edge_into_ael(&mut self, edge: usize, mut start_edge: usize) {
        if self.active_edges == NIL {
            self.edges[edge].prev_in_ael = NIL;
            self.edges[edge].next_in_ael = NIL;
            self.active_edges = edge;
        } else if start_edge == NIL && self.e2_inserts_before_e1(self.active_edges, edge) {
            self.edges[edge].prev_in_ael = NIL;
            self.edges[edge].next_in_ael = self.active_edges;
            let ae = self.active_edges;
            self.edges[ae].prev_in_ael = edge;
            self.active_edges = edge;
        } else {
            if start_edge == NIL {
                start_edge = self.active_edges;
            }
            while self.edges[start_edge].next_in_ael != NIL
                && !self.e2_inserts_before_e1(self.edges[start_edge].next_in_ael, edge)
            {
                start_edge = self.edges[start_edge].next_in_ael;
            }
            self.edges[edge].next_in_ael = self.edges[start_edge].next_in_ael;
            let n = self.edges[start_edge].next_in_ael;
            if n != NIL {
                self.edges[n].prev_in_ael = edge;
            }
            self.edges[edge].prev_in_ael = start_edge;
            self.edges[start_edge].next_in_ael = edge;
        }
    }

    fn e2_inserts_before_e1(&self, e1: usize, e2: usize) -> bool {
        let (a, b) = (&self.edges[e1], &self.edges[e2]);
        if b.curr.x == a.curr.x {
            if b.top.y > a.top.y {
                b.top.x < top_x(a, b.top.y)
            } else {
                a.top.x > top_x(b, a.top.y)
            }
        } else {
            b.curr.x < a.curr.x
        }
    }

    fn is_even_odd_fill_type(&self, e: usize) -> bool {
        if self.edges[e].poly_typ == PolyType::Subject {
            self.subj_fill() == PolyFillType::EvenOdd
        } else {
            self.clip_fill() == PolyFillType::EvenOdd
        }
    }

    fn is_even_odd_alt_fill_type(&self, e: usize) -> bool {
        if self.edges[e].poly_typ == PolyType::Subject {
            self.clip_fill() == PolyFillType::EvenOdd
        } else {
            self.subj_fill() == PolyFillType::EvenOdd
        }
    }

    fn is_contributing(&self, e: usize) -> bool {
        let edge = &self.edges[e];
        let (pft, pft2) = if edge.poly_typ == PolyType::Subject {
            (self.subj_fill(), self.clip_fill())
        } else {
            (self.clip_fill(), self.subj_fill())
        };

        match pft {
            PolyFillType::EvenOdd => {
                // return false if a subj line has been flagged as inside a subj polygon
                if edge.wind_delta == 0 && edge.wind_cnt != 1 {
                    return false;
                }
            }
            PolyFillType::NonZero => {
                if edge.wind_cnt.abs() != 1 {
                    return false;
                }
            }
            PolyFillType::Positive => {
                if edge.wind_cnt != 1 {
                    return false;
                }
            }
            PolyFillType::Negative => {
                if edge.wind_cnt != -1 {
                    return false;
                }
            }
        }

        let w2 = edge.wind_cnt2;
        let not_zero = |pft2| match pft2 {
            PolyFillType::EvenOdd | PolyFillType::NonZero => w2 != 0,
            PolyFillType::Positive => w2 > 0,
            PolyFillType::Negative => w2 < 0,
        };
        let zero = |pft2| match pft2 {
            PolyFillType::EvenOdd | PolyFillType::NonZero => w2 == 0,
            PolyFillType::Positive => w2 <= 0,
            PolyFillType::Negative => w2 >= 0,
        };
        match self.ctype() {
            ClipType::Intersection => not_zero(pft2),
            ClipType::Union => zero(pft2),
            ClipType::Difference => {
                if edge.poly_typ == PolyType::Subject {
                    zero(pft2)
                } else {
                    not_zero(pft2)
                }
            }
            ClipType::Xor => {
                if edge.wind_delta == 0 {
                    // XOr always contributing unless open
                    zero(pft2)
                } else {
                    true
                }
            }
        }
    }

    fn set_winding_count(&mut self, edge: usize) {
        let mut e = self.edges[edge].prev_in_ael;
        // find the edge of the same polytype that immediately preceeds 'edge' in AEL
        while e != NIL
            && (self.edges[e].poly_typ != self.edges[edge].poly_typ || self.edges[e].wind_delta == 0)
        {
            e = self.edges[e].prev_in_ael;
        }
        let wd = self.edges[edge].wind_delta;
        if e == NIL {
            let pft = if self.edges[edge].poly_typ == PolyType::Subject {
                self.subj_fill()
            } else {
                self.clip_fill()
            };
            self.edges[edge].wind_cnt = if wd == 0 {
                if pft == PolyFillType::Negative { -1 } else { 1 }
            } else {
                wd
            };
            self.edges[edge].wind_cnt2 = 0;
            e = self.active_edges; // ie get ready to calc WindCnt2
        } else if wd == 0 && self.ctype() != ClipType::Union {
            self.edges[edge].wind_cnt = 1;
            self.edges[edge].wind_cnt2 = self.edges[e].wind_cnt2;
            e = self.edges[e].next_in_ael; // ie get ready to calc WindCnt2
        } else if self.is_even_odd_fill_type(edge) {
            // EvenOdd filling ...
            if wd == 0 {
                // are we inside a subj polygon ...
                let mut inside = true;
                let mut e2 = self.edges[e].prev_in_ael;
                while e2 != NIL {
                    if self.edges[e2].poly_typ == self.edges[e].poly_typ && self.edges[e2].wind_delta != 0 {
                        inside = !inside;
                    }
                    e2 = self.edges[e2].prev_in_ael;
                }
                self.edges[edge].wind_cnt = i32::from(!inside);
            } else {
                self.edges[edge].wind_cnt = wd;
            }
            self.edges[edge].wind_cnt2 = self.edges[e].wind_cnt2;
            e = self.edges[e].next_in_ael; // ie get ready to calc WindCnt2
        } else {
            // nonZero, Positive or Negative filling ...
            let (ewc, ewd) = (self.edges[e].wind_cnt, self.edges[e].wind_delta);
            if ewc * ewd < 0 {
                // prev edge is 'decreasing' WindCount (WC) toward zero
                // so we're outside the previous polygon ...
                if ewc.abs() > 1 {
                    // outside prev poly but still inside another.
                    // when reversing direction of prev poly use the same WC
                    if ewd * wd < 0 {
                        self.edges[edge].wind_cnt = ewc;
                    } else {
                        // otherwise continue to 'decrease' WC ...
                        self.edges[edge].wind_cnt = ewc + wd;
                    }
                } else {
                    // now outside all polys of same polytype so set own WC ...
                    self.edges[edge].wind_cnt = if wd == 0 { 1 } else { wd };
                }
            } else {
                // prev edge is 'increasing' WindCount (WC) away from zero
                // so we're inside the previous polygon ...
                if wd == 0 {
                    self.edges[edge].wind_cnt = if ewc < 0 { ewc - 1 } else { ewc + 1 };
                } else if ewd * wd < 0 {
                    // if wind direction is reversing prev then use same WC
                    self.edges[edge].wind_cnt = ewc;
                } else {
                    // otherwise add to WC ...
                    self.edges[edge].wind_cnt = ewc + wd;
                }
            }
            self.edges[edge].wind_cnt2 = self.edges[e].wind_cnt2;
            e = self.edges[e].next_in_ael; // ie get ready to calc WindCnt2
        }

        // update WindCnt2 ...
        if self.is_even_odd_alt_fill_type(edge) {
            // EvenOdd filling ...
            while e != edge {
                if self.edges[e].wind_delta != 0 {
                    let w = self.edges[edge].wind_cnt2;
                    self.edges[edge].wind_cnt2 = i32::from(w == 0);
                }
                e = self.edges[e].next_in_ael;
            }
        } else {
            // nonZero, Positive or Negative filling ...
            while e != edge {
                self.edges[edge].wind_cnt2 += self.edges[e].wind_delta;
                e = self.edges[e].next_in_ael;
            }
        }
    }

    fn add_edge_to_sel(&mut self, edge: usize) {
        // SEL pointers in PEdge are use to build transient lists of horizontal edges.
        // However, since we don't need to worry about processing order, all additions
        // are made to the front of the list ...
        if self.sorted_edges == NIL {
            self.sorted_edges = edge;
            self.edges[edge].prev_in_sel = NIL;
            self.edges[edge].next_in_sel = NIL;
        } else {
            self.edges[edge].next_in_sel = self.sorted_edges;
            self.edges[edge].prev_in_sel = NIL;
            let s = self.sorted_edges;
            self.edges[s].prev_in_sel = edge;
            self.sorted_edges = edge;
        }
    }

    fn pop_edge_from_sel(&mut self) -> Option<usize> {
        // Pop edge from front of SEL (ie SEL is a FILO list)
        let e = self.sorted_edges;
        if e == NIL {
            return None;
        }
        self.sorted_edges = self.edges[e].next_in_sel;
        if self.sorted_edges != NIL {
            let s = self.sorted_edges;
            self.edges[s].prev_in_sel = NIL;
        }
        self.edges[e].next_in_sel = NIL;
        self.edges[e].prev_in_sel = NIL;
        Some(e)
    }

    fn copy_ael_to_sel(&mut self) {
        let mut e = self.active_edges;
        self.sorted_edges = e;
        while e != NIL {
            self.edges[e].prev_in_sel = self.edges[e].prev_in_ael;
            self.edges[e].next_in_sel = self.edges[e].next_in_ael;
            e = self.edges[e].next_in_ael;
        }
    }

    fn swap_positions_in_sel(&mut self, edge1: usize, edge2: usize) {
        if self.edges[edge1].next_in_sel == NIL && self.edges[edge1].prev_in_sel == NIL {
            return;
        }
        if self.edges[edge2].next_in_sel == NIL && self.edges[edge2].prev_in_sel == NIL {
            return;
        }
        if self.edges[edge1].next_in_sel == edge2 {
            let next = self.edges[edge2].next_in_sel;
            if next != NIL {
                self.edges[next].prev_in_sel = edge1;
            }
            let prev = self.edges[edge1].prev_in_sel;
            if prev != NIL {
                self.edges[prev].next_in_sel = edge2;
            }
            self.edges[edge2].prev_in_sel = prev;
            self.edges[edge2].next_in_sel = edge1;
            self.edges[edge1].prev_in_sel = edge2;
            self.edges[edge1].next_in_sel = next;
        } else if self.edges[edge2].next_in_sel == edge1 {
            let next = self.edges[edge1].next_in_sel;
            if next != NIL {
                self.edges[next].prev_in_sel = edge2;
            }
            let prev = self.edges[edge2].prev_in_sel;
            if prev != NIL {
                self.edges[prev].next_in_sel = edge1;
            }
            self.edges[edge1].prev_in_sel = prev;
            self.edges[edge1].next_in_sel = edge2;
            self.edges[edge2].prev_in_sel = edge1;
            self.edges[edge2].next_in_sel = next;
        } else {
            let next = self.edges[edge1].next_in_sel;
            let prev = self.edges[edge1].prev_in_sel;
            self.edges[edge1].next_in_sel = self.edges[edge2].next_in_sel;
            let n1 = self.edges[edge1].next_in_sel;
            if n1 != NIL {
                self.edges[n1].prev_in_sel = edge1;
            }
            self.edges[edge1].prev_in_sel = self.edges[edge2].prev_in_sel;
            let p1 = self.edges[edge1].prev_in_sel;
            if p1 != NIL {
                self.edges[p1].next_in_sel = edge1;
            }
            self.edges[edge2].next_in_sel = next;
            if next != NIL {
                self.edges[next].prev_in_sel = edge2;
            }
            self.edges[edge2].prev_in_sel = prev;
            if prev != NIL {
                self.edges[prev].next_in_sel = edge2;
            }
        }
        if self.edges[edge1].prev_in_sel == NIL {
            self.sorted_edges = edge1;
        } else if self.edges[edge2].prev_in_sel == NIL {
            self.sorted_edges = edge2;
        }
    }

    fn add_local_max_poly(&mut self, e1: usize, e2: usize, pt: IntPoint) {
        self.add_out_pt(e1, pt);
        if self.edges[e2].wind_delta == 0 {
            self.add_out_pt(e2, pt);
        }
        let (o1, o2) = (self.edges[e1].out_idx, self.edges[e2].out_idx);
        if o1 == o2 {
            self.edges[e1].out_idx = UNASSIGNED;
            self.edges[e2].out_idx = UNASSIGNED;
        } else if o1 < o2 {
            self.append_polygon(e1, e2);
        } else {
            self.append_polygon(e2, e1);
        }
    }

    fn add_local_min_poly(&mut self, e1: usize, e2: usize, pt: IntPoint) -> usize {
        let result;
        let e;
        let prev_e;
        if self.is_horizontal(e2) || self.edges[e1].dx > self.edges[e2].dx {
            result = self.add_out_pt(e1, pt);
            self.edges[e2].out_idx = self.edges[e1].out_idx;
            self.edges[e1].side = EdgeSide::Left;
            self.edges[e2].side = EdgeSide::Right;
            e = e1;
            prev_e = if self.edges[e].prev_in_ael == e2 {
                self.edges[e2].prev_in_ael
            } else {
                self.edges[e].prev_in_ael
            };
        } else {
            result = self.add_out_pt(e2, pt);
            self.edges[e1].out_idx = self.edges[e2].out_idx;
            self.edges[e1].side = EdgeSide::Right;
            self.edges[e2].side = EdgeSide::Left;
            e = e2;
            prev_e = if self.edges[e].prev_in_ael == e1 {
                self.edges[e1].prev_in_ael
            } else {
                self.edges[e].prev_in_ael
            };
        }

        if prev_e != NIL
            && self.edges[prev_e].out_idx >= 0
            && self.edges[prev_e].top.y < pt.y
            && self.edges[e].top.y < pt.y
        {
            let x_prev = top_x(&self.edges[prev_e], pt.y);
            let x_e = top_x(&self.edges[e], pt.y);
            if x_prev == x_e
                && self.edges[e].wind_delta != 0
                && self.edges[prev_e].wind_delta != 0
                && slopes_equal4(
                    IntPoint::new(x_prev, pt.y),
                    self.edges[prev_e].top,
                    IntPoint::new(x_e, pt.y),
                    self.edges[e].top,
                    self.use_full_range,
                )
            {
                let out_pt = self.add_out_pt(prev_e, pt);
                self.add_join(result, out_pt, self.edges[e].top);
            }
        }
        result
    }

    #[allow(clippy::cast_sign_loss)]
    fn add_out_pt(&mut self, e: usize, pt: IntPoint) -> usize {
        if self.edges[e].out_idx < 0 {
            let rec = self.create_out_rec();
            self.poly_outs[rec].is_open = self.edges[e].wind_delta == 0;
            let new_op = self.new_out_pt(self.poly_outs[rec].idx, pt);
            self.poly_outs[rec].pts = new_op;
            if !self.poly_outs[rec].is_open {
                self.set_hole_state(e, rec);
            }
            self.edges[e].out_idx = self.poly_outs[rec].idx;
            new_op
        } else {
            let rec = self.edges[e].out_idx as usize;
            // OutRec.Pts is the 'Left-most' point & OutRec.Pts.Prev is the 'Right-most'
            let op = self.poly_outs[rec].pts;
            let to_front = self.edges[e].side == EdgeSide::Left;
            if to_front && pt == self.out_pts[op].pt {
                return op;
            } else if !to_front && pt == self.out_pts[self.out_pts[op].prev].pt {
                return self.out_pts[op].prev;
            }
            let new_op = self.new_out_pt(self.poly_outs[rec].idx, pt);
            let op_prev = self.out_pts[op].prev;
            self.out_pts[new_op].next = op;
            self.out_pts[new_op].prev = op_prev;
            self.out_pts[op_prev].next = new_op;
            self.out_pts[op].prev = new_op;
            if to_front {
                self.poly_outs[rec].pts = new_op;
            }
            new_op
        }
    }

    #[allow(clippy::cast_sign_loss)]
    fn get_last_out_pt(&self, e: usize) -> usize {
        let rec = &self.poly_outs[self.edges[e].out_idx as usize];
        if self.edges[e].side == EdgeSide::Left {
            rec.pts
        } else {
            self.out_pts[rec.pts].prev
        }
    }

    #[allow(clippy::cast_sign_loss)]
    fn set_hole_state(&mut self, e: usize, rec: usize) {
        let mut e2 = self.edges[e].prev_in_ael;
        let mut e_tmp = NIL;
        while e2 != NIL {
            if self.edges[e2].out_idx >= 0 && self.edges[e2].wind_delta != 0 {
                if e_tmp == NIL {
                    e_tmp = e2;
                } else if self.edges[e_tmp].out_idx == self.edges[e2].out_idx {
                    e_tmp = NIL; // paired
                }
            }
            e2 = self.edges[e2].prev_in_ael;
        }
        if e_tmp == NIL {
            self.poly_outs[rec].first_left = NIL;
            self.poly_outs[rec].is_hole = false;
        } else {
            let fl = self.edges[e_tmp].out_idx as usize;
            self.poly_outs[rec].first_left = fl;
            self.poly_outs[rec].is_hole = !self.poly_outs[fl].is_hole;
        }
    }

    fn first_is_bottom_pt(&self, btm_pt1: usize, btm_pt2: usize) -> bool {
        let op = &self.out_pts;
        let mut p = op[btm_pt1].prev;
        while op[p].pt == op[btm_pt1].pt && p != btm_pt1 {
            p = op[p].prev;
        }
        let dx1p = get_dx(op[btm_pt1].pt, op[p].pt).abs();
        p = op[btm_pt1].next;
        while op[p].pt == op[btm_pt1].pt && p != btm_pt1 {
            p = op[p].next;
        }
        let dx1n = get_dx(op[btm_pt1].pt, op[p].pt).abs();

        p = op[btm_pt2].prev;
        while op[p].pt == op[btm_pt2].pt && p != btm_pt2 {
            p = op[p].prev;
        }
        let dx2p = get_dx(op[btm_pt2].pt, op[p].pt).abs();
        p = op[btm_pt2].next;
        while op[p].pt == op[btm_pt2].pt && p != btm_pt2 {
            p = op[p].next;
        }
        let dx2n = get_dx(op[btm_pt2].pt, op[p].pt).abs();

        if dx1p.max(dx1n) == dx2p.max(dx2n) && dx1p.min(dx1n) == dx2p.min(dx2n) {
            self.area_op(btm_pt1) > 0.0 // if otherwise identical use orientation
        } else {
            (dx1p >= dx2p && dx1p >= dx2n) || (dx1n >= dx2p && dx1n >= dx2n)
        }
    }

    fn get_bottom_pt(&self, mut pp: usize) -> usize {
        let op = &self.out_pts;
        let mut dups = NIL;
        let mut p = op[pp].next;
        while p != pp {
            if op[p].pt.y > op[pp].pt.y {
                pp = p;
                dups = NIL;
            } else if op[p].pt.y == op[pp].pt.y && op[p].pt.x <= op[pp].pt.x {
                if op[p].pt.x < op[pp].pt.x {
                    dups = NIL;
                    pp = p;
                } else if op[p].next != pp && op[p].prev != pp {
                    dups = p;
                }
            }
            p = op[p].next;
        }
        if dups != NIL {
            // there appears to be at least 2 vertices at bottomPt so ...
            while dups != p {
                if !self.first_is_bottom_pt(p, dups) {
                    pp = dups;
                }
                dups = op[dups].next;
                while op[dups].pt != op[pp].pt {
                    dups = op[dups].next;
                }
            }
        }
        pp
    }

    fn get_lowermost_rec(&mut self, rec1: usize, rec2: usize) -> usize {
        // work out which polygon fragment has the correct hole state ...
        if self.poly_outs[rec1].bottom_pt == NIL {
            self.poly_outs[rec1].bottom_pt = self.get_bottom_pt(self.poly_outs[rec1].pts);
        }
        if self.poly_outs[rec2].bottom_pt == NIL {
            self.poly_outs[rec2].bottom_pt = self.get_bottom_pt(self.poly_outs[rec2].pts);
        }
        let b1 = self.poly_outs[rec1].bottom_pt;
        let b2 = self.poly_outs[rec2].bottom_pt;
        let (p1, p2) = (self.out_pts[b1].pt, self.out_pts[b2].pt);
        if p1.y > p2.y {
            rec1
        } else if p1.y < p2.y {
            rec2
        } else if p1.x < p2.x {
            rec1
        } else if p1.x > p2.x || self.out_pts[b1].next == b1 {
            rec2
        } else if self.out_pts[b2].next == b2 || self.first_is_bottom_pt(b1, b2) {
            rec1
        } else {
            rec2
        }
    }

    fn out_rec1_right_of_out_rec2(&self, mut rec1: usize, rec2: usize) -> bool {
        loop {
            rec1 = self.poly_outs[rec1].first_left;
            if rec1 == rec2 {
                return true;
            }
            if rec1 == NIL {
                return false;
            }
        }
    }

    #[allow(clippy::cast_sign_loss)]
    fn get_out_rec(&self, idx: i32) -> usize {
        let mut r = idx as usize;
        while r != self.poly_outs[r].idx as usize {
            r = self.poly_outs[r].idx as usize;
        }
        r
    }

    #[allow(clippy::cast_sign_loss)]
    fn append_polygon(&mut self, e1: usize, e2: usize) {
        let rec1 = self.edges[e1].out_idx as usize;
        let rec2 = self.edges[e2].out_idx as usize;

        let hole_state_rec = if self.out_rec1_right_of_out_rec2(rec1, rec2) {
            rec2
        } else if self.out_rec1_right_of_out_rec2(rec2, rec1) {
            rec1
        } else {
            self.get_lowermost_rec(rec1, rec2)
        };

        // get the start and ends of both output polygons and
        // join E2 poly onto E1 poly and delete pointers to E2 ...
        let p1_lft = self.poly_outs[rec1].pts;
        let p1_rt = self.out_pts[p1_lft].prev;
        let p2_lft = self.poly_outs[rec2].pts;
        let p2_rt = self.out_pts[p2_lft].prev;

        // join e2 poly onto e1 poly and delete pointers to e2 ...
        if self.edges[e1].side == EdgeSide::Left {
            if self.edges[e2].side == EdgeSide::Left {
                // z y x a b c
                self.reverse_poly_pt_links(p2_lft);
                self.out_pts[p2_lft].next = p1_lft;
                self.out_pts[p1_lft].prev = p2_lft;
                self.out_pts[p1_rt].next = p2_rt;
                self.out_pts[p2_rt].prev = p1_rt;
                self.poly_outs[rec1].pts = p2_rt;
            } else {
                // x y z a b c
                self.out_pts[p2_rt].next = p1_lft;
                self.out_pts[p1_lft].prev = p2_rt;
                self.out_pts[p2_lft].prev = p1_rt;
                self.out_pts[p1_rt].next = p2_lft;
                self.poly_outs[rec1].pts = p2_lft;
            }
        } else if self.edges[e2].side == EdgeSide::Right {
            // a b c z y x
            self.reverse_poly_pt_links(p2_lft);
            self.out_pts[p1_rt].next = p2_rt;
            self.out_pts[p2_rt].prev = p1_rt;
            self.out_pts[p2_lft].next = p1_lft;
            self.out_pts[p1_lft].prev = p2_lft;
        } else {
            // a b c x y z
            self.out_pts[p1_rt].next = p2_lft;
            self.out_pts[p2_lft].prev = p1_rt;
            self.out_pts[p1_lft].prev = p2_rt;
            self.out_pts[p2_rt].next = p1_lft;
        }

        self.poly_outs[rec1].bottom_pt = NIL;
        if hole_state_rec == rec2 {
            if self.poly_outs[rec2].first_left != rec1 {
                self.poly_outs[rec1].first_left = self.poly_outs[rec2].first_left;
            }
            self.poly_outs[rec1].is_hole = self.poly_outs[rec2].is_hole;
        }
        self.poly_outs[rec2].pts = NIL;
        self.poly_outs[rec2].bottom_pt = NIL;
        self.poly_outs[rec2].first_left = rec1;

        let ok_idx = self.edges[e1].out_idx;
        let obsolete_idx = self.edges[e2].out_idx;

        self.edges[e1].out_idx = UNASSIGNED; // nb: safe because we only get here via AddLocalMaxPoly
        self.edges[e2].out_idx = UNASSIGNED;

        let mut e = self.active_edges;
        while e != NIL {
            if self.edges[e].out_idx == obsolete_idx {
                self.edges[e].out_idx = ok_idx;
                self.edges[e].side = self.edges[e1].side;
                break;
            }
            e = self.edges[e].next_in_ael;
        }
        self.poly_outs[rec2].idx = self.poly_outs[rec1].idx;
    }

    fn reverse_poly_pt_links(&mut self, pp: usize) {
        if pp == NIL {
            return;
        }
        let mut pp1 = pp;
        loop {
            let pp2 = self.out_pts[pp1].next;
            self.out_pts[pp1].next = self.out_pts[pp1].prev;
            self.out_pts[pp1].prev = pp2;
            pp1 = pp2;
            if pp1 == pp {
                break;
            }
        }
    }

    fn swap_sides(&mut self, e1: usize, e2: usize) {
        let s = self.edges[e1].side;
        self.edges[e1].side = self.edges[e2].side;
        self.edges[e2].side = s;
    }

    fn swap_poly_indexes(&mut self, e1: usize, e2: usize) {
        let o = self.edges[e1].out_idx;
        self.edges[e1].out_idx = self.edges[e2].out_idx;
        self.edges[e2].out_idx = o;
    }

    #[allow(clippy::too_many_lines)]
    fn intersect_edges(&mut self, e1: usize, e2: usize, pt: IntPoint) {
        // e1 will be to the left of e2 BELOW the intersection. Therefore e1 is before
        // e2 in AEL except when e1 is being inserted at the intersection point ...
        let e1_contributing = self.edges[e1].out_idx >= 0;
        let e2_contributing = self.edges[e2].out_idx >= 0;
        let ct = self.ctype();

        // if either edge is on an OPEN path ...
        let (wd1, wd2) = (self.edges[e1].wind_delta, self.edges[e2].wind_delta);
        if wd1 == 0 || wd2 == 0 {
            // ignore subject-subject open path intersections UNLESS they
            // are both open paths, AND they are both 'contributing maximas' ...
            if wd1 == 0 && wd2 == 0 {
                return;
            } else if self.edges[e1].poly_typ == self.edges[e2].poly_typ
                && wd1 != wd2
                && ct == ClipType::Union
            {
                // if intersecting a subj line with a subj poly ...
                if wd1 == 0 {
                    if e2_contributing {
                        self.add_out_pt(e1, pt);
                        if e1_contributing {
                            self.edges[e1].out_idx = UNASSIGNED;
                        }
                    }
                } else if e1_contributing {
                    self.add_out_pt(e2, pt);
                    if e2_contributing {
                        self.edges[e2].out_idx = UNASSIGNED;
                    }
                }
            } else if self.edges[e1].poly_typ != self.edges[e2].poly_typ {
                if wd1 == 0
                    && self.edges[e2].wind_cnt.abs() == 1
                    && (ct != ClipType::Union || self.edges[e2].wind_cnt2 == 0)
                {
                    self.add_out_pt(e1, pt);
                    if e1_contributing {
                        self.edges[e1].out_idx = UNASSIGNED;
                    }
                } else if wd2 == 0
                    && self.edges[e1].wind_cnt.abs() == 1
                    && (ct != ClipType::Union || self.edges[e1].wind_cnt2 == 0)
                {
                    self.add_out_pt(e2, pt);
                    if e2_contributing {
                        self.edges[e2].out_idx = UNASSIGNED;
                    }
                }
            }
            return;
        }

        // update winding counts...
        // assumes that e1 will be to the Right of e2 ABOVE the intersection
        if self.edges[e1].poly_typ == self.edges[e2].poly_typ {
            if self.is_even_odd_fill_type(e1) {
                let old = self.edges[e1].wind_cnt;
                self.edges[e1].wind_cnt = self.edges[e2].wind_cnt;
                self.edges[e2].wind_cnt = old;
            } else {
                if self.edges[e1].wind_cnt + wd2 == 0 {
                    self.edges[e1].wind_cnt = -self.edges[e1].wind_cnt;
                } else {
                    self.edges[e1].wind_cnt += wd2;
                }
                if self.edges[e2].wind_cnt - wd1 == 0 {
                    self.edges[e2].wind_cnt = -self.edges[e2].wind_cnt;
                } else {
                    self.edges[e2].wind_cnt -= wd1;
                }
            }
        } else {
            if self.is_even_odd_fill_type(e2) {
                let w = self.edges[e1].wind_cnt2;
                self.edges[e1].wind_cnt2 = i32::from(w == 0);
            } else {
                self.edges[e1].wind_cnt2 += wd2;
            }
            if self.is_even_odd_fill_type(e1) {
                let w = self.edges[e2].wind_cnt2;
                self.edges[e2].wind_cnt2 = i32::from(w == 0);
            } else {
                self.edges[e2].wind_cnt2 -= wd1;
            }
        }

        let (e1_fill, e1_fill2) = if self.edges[e1].poly_typ == PolyType::Subject {
            (self.subj_fill(), self.clip_fill())
        } else {
            (self.clip_fill(), self.subj_fill())
        };
        let (e2_fill, e2_fill2) = if self.edges[e2].poly_typ == PolyType::Subject {
            (self.subj_fill(), self.clip_fill())
        } else {
            (self.clip_fill(), self.subj_fill())
        };

        let wc = |fill: PolyFillType, cnt: i32| match fill {
            PolyFillType::Positive => cnt,
            PolyFillType::Negative => -cnt,
            _ => cnt.abs(),
        };
        let e1_wc = wc(e1_fill, self.edges[e1].wind_cnt);
        let e2_wc = wc(e2_fill, self.edges[e2].wind_cnt);

        if e1_contributing && e2_contributing {
            if (e1_wc != 0 && e1_wc != 1)
                || (e2_wc != 0 && e2_wc != 1)
                || (self.edges[e1].poly_typ != self.edges[e2].poly_typ && ct != ClipType::Xor)
            {
                self.add_local_max_poly(e1, e2, pt);
            } else {
                self.add_out_pt(e1, pt);
                self.add_out_pt(e2, pt);
                self.swap_sides(e1, e2);
                self.swap_poly_indexes(e1, e2);
            }
        } else if e1_contributing {
            if e2_wc == 0 || e2_wc == 1 {
                self.add_out_pt(e1, pt);
                self.swap_sides(e1, e2);
                self.swap_poly_indexes(e1, e2);
            }
        } else if e2_contributing {
            if e1_wc == 0 || e1_wc == 1 {
                self.add_out_pt(e2, pt);
                self.swap_sides(e1, e2);
                self.swap_poly_indexes(e1, e2);
            }
        } else if (e1_wc == 0 || e1_wc == 1) && (e2_wc == 0 || e2_wc == 1) {
            // neither edge is currently contributing ...
            let e1_wc2 = wc(e1_fill2, self.edges[e1].wind_cnt2);
            let e2_wc2 = wc(e2_fill2, self.edges[e2].wind_cnt2);

            if self.edges[e1].poly_typ != self.edges[e2].poly_typ {
                self.add_local_min_poly(e1, e2, pt);
            } else if e1_wc == 1 && e2_wc == 1 {
                match ct {
                    ClipType::Intersection => {
                        if e1_wc2 > 0 && e2_wc2 > 0 {
                            self.add_local_min_poly(e1, e2, pt);
                        }
                    }
                    ClipType::Union => {
                        if e1_wc2 <= 0 && e2_wc2 <= 0 {
                            self.add_local_min_poly(e1, e2, pt);
                        }
                    }
                    ClipType::Difference => {
                        let t = self.edges[e1].poly_typ;
                        if (t == PolyType::Clip && e1_wc2 > 0 && e2_wc2 > 0)
                            || (t == PolyType::Subject && e1_wc2 <= 0 && e2_wc2 <= 0)
                        {
                            self.add_local_min_poly(e1, e2, pt);
                        }
                    }
                    ClipType::Xor => {
                        self.add_local_min_poly(e1, e2, pt);
                    }
                }
            } else {
                self.swap_sides(e1, e2);
            }
        }
    }

    fn process_horizontals(&mut self) -> Result<(), ClipperError> {
        while let Some(horz) = self.pop_edge_from_sel() {
            self.process_horizontal(horz)?;
        }
        Ok(())
    }

    fn horz_direction(&self, e: usize) -> (Direction, i64, i64) {
        let ed = &self.edges[e];
        if ed.bot.x < ed.top.x {
            (Direction::LeftToRight, ed.bot.x, ed.top.x)
        } else {
            (Direction::RightToLeft, ed.top.x, ed.bot.x)
        }
    }

    fn get_next_in_ael(&self, e: usize, dir: Direction) -> usize {
        if dir == Direction::LeftToRight {
            self.edges[e].next_in_ael
        } else {
            self.edges[e].prev_in_ael
        }
    }

    #[allow(clippy::too_many_lines)]
    fn process_horizontal(&mut self, mut horz_edge: usize) -> Result<(), ClipperError> {
        let is_open = self.edges[horz_edge].wind_delta == 0;
        let (mut dir, mut horz_left, mut horz_right) = self.horz_direction(horz_edge);

        let mut e_last_horz = horz_edge;
        let mut e_max_pair = NIL;
        while self.edges[e_last_horz].next_in_lml != NIL
            && self.is_horizontal(self.edges[e_last_horz].next_in_lml)
        {
            e_last_horz = self.edges[e_last_horz].next_in_lml;
        }
        if self.edges[e_last_horz].next_in_lml == NIL {
            e_max_pair = self.get_maxima_pair(e_last_horz);
        }

        // the maxima list is sorted ascending; `curr_max` is an index into it
        let mut curr_max: Option<usize> = None;
        if !self.maxima.is_empty() {
            // get the first maxima in range (X) ...
            if dir == Direction::LeftToRight {
                let mut m = 0;
                while m < self.maxima.len() && self.maxima[m] <= self.edges[horz_edge].bot.x {
                    m += 1;
                }
                curr_max = if m < self.maxima.len() && self.maxima[m] >= self.edges[e_last_horz].top.x {
                    None
                } else if m < self.maxima.len() {
                    Some(m)
                } else {
                    None
                };
            } else {
                let mut m = 0;
                while m + 1 < self.maxima.len() && self.maxima[m + 1] < self.edges[horz_edge].bot.x {
                    m += 1;
                }
                curr_max = if self.maxima[m] <= self.edges[e_last_horz].top.x {
                    None
                } else {
                    Some(m)
                };
            }
        }

        let mut op1 = NIL;
        loop {
            // loop through consec. horizontal edges
            let is_last_horz = horz_edge == e_last_horz;
            let mut e = self.get_next_in_ael(horz_edge, dir);
            while e != NIL {
                // this code block inserts extra coords into horizontal edges (in output
                // polygons) whereever maxima touch these horizontal edges. This helps
                // 'simplifying' polygons (ie if the Simplify property is set).
                if let Some(m) = curr_max {
                    let mut cur = Some(m);
                    if dir == Direction::LeftToRight {
                        while let Some(mi) = cur {
                            if self.maxima[mi] >= self.edges[e].curr.x {
                                break;
                            }
                            if self.edges[horz_edge].out_idx >= 0 && !is_open {
                                let p = IntPoint::new(self.maxima[mi], self.edges[horz_edge].bot.y);
                                self.add_out_pt(horz_edge, p);
                            }
                            cur = if mi + 1 < self.maxima.len() {
                                Some(mi + 1)
                            } else {
                                None
                            };
                        }
                    } else {
                        while let Some(mi) = cur {
                            if self.maxima[mi] <= self.edges[e].curr.x {
                                break;
                            }
                            if self.edges[horz_edge].out_idx >= 0 && !is_open {
                                let p = IntPoint::new(self.maxima[mi], self.edges[horz_edge].bot.y);
                                self.add_out_pt(horz_edge, p);
                            }
                            cur = mi.checked_sub(1);
                        }
                    }
                    curr_max = cur;
                }

                if (dir == Direction::LeftToRight && self.edges[e].curr.x > horz_right)
                    || (dir == Direction::RightToLeft && self.edges[e].curr.x < horz_left)
                {
                    break;
                }

                // Also break if we've got to the end of an intermediate horizontal edge ...
                // nb: Smaller Dx's are to the right of larger Dx's ABOVE the horizontal.
                let hn = self.edges[horz_edge].next_in_lml;
                if self.edges[e].curr.x == self.edges[horz_edge].top.x
                    && hn != NIL
                    && self.edges[e].dx < self.edges[hn].dx
                {
                    break;
                }

                if self.edges[horz_edge].out_idx >= 0 && !is_open {
                    // note: may be done multiple times
                    op1 = self.add_out_pt(horz_edge, self.edges[e].curr);
                    let mut e_next_horz = self.sorted_edges;
                    while e_next_horz != NIL {
                        if self.edges[e_next_horz].out_idx >= 0
                            && horz_segments_overlap(
                                self.edges[horz_edge].bot.x,
                                self.edges[horz_edge].top.x,
                                self.edges[e_next_horz].bot.x,
                                self.edges[e_next_horz].top.x,
                            )
                        {
                            let op2 = self.get_last_out_pt(e_next_horz);
                            self.add_join(op2, op1, self.edges[e_next_horz].top);
                        }
                        e_next_horz = self.edges[e_next_horz].next_in_sel;
                    }
                    self.add_ghost_join(op1, self.edges[horz_edge].bot);
                }

                // OK, so far we're still in range of the horizontal Edge  but make sure
                // we're at the last of consec. horizontals when matching with eMaxPair
                if e == e_max_pair && is_last_horz {
                    if self.edges[horz_edge].out_idx >= 0 {
                        let top = self.edges[horz_edge].top;
                        self.add_local_max_poly(horz_edge, e_max_pair, top);
                    }
                    self.delete_from_ael(horz_edge);
                    self.delete_from_ael(e_max_pair);
                    return Ok(());
                }

                let pt = IntPoint::new(self.edges[e].curr.x, self.edges[horz_edge].curr.y);
                if dir == Direction::LeftToRight {
                    self.intersect_edges(horz_edge, e, pt);
                } else {
                    self.intersect_edges(e, horz_edge, pt);
                }
                let e_next = self.get_next_in_ael(e, dir);
                self.swap_positions_in_ael(horz_edge, e);
                e = e_next;
            } // end while(e != null)

            // Break out of loop if HorzEdge.NextInLML is not also horizontal ...
            let hn = self.edges[horz_edge].next_in_lml;
            if hn == NIL || !self.is_horizontal(hn) {
                break;
            }

            horz_edge = self.update_edge_into_ael(horz_edge)?;
            if self.edges[horz_edge].out_idx >= 0 {
                self.add_out_pt(horz_edge, self.edges[horz_edge].bot);
            }
            (dir, horz_left, horz_right) = self.horz_direction(horz_edge);
        } // end for (;;)

        if self.edges[horz_edge].out_idx >= 0 && op1 == NIL {
            op1 = self.get_last_out_pt(horz_edge);
            let mut e_next_horz = self.sorted_edges;
            while e_next_horz != NIL {
                if self.edges[e_next_horz].out_idx >= 0
                    && horz_segments_overlap(
                        self.edges[horz_edge].bot.x,
                        self.edges[horz_edge].top.x,
                        self.edges[e_next_horz].bot.x,
                        self.edges[e_next_horz].top.x,
                    )
                {
                    let op2 = self.get_last_out_pt(e_next_horz);
                    self.add_join(op2, op1, self.edges[e_next_horz].top);
                }
                e_next_horz = self.edges[e_next_horz].next_in_sel;
            }
            self.add_ghost_join(op1, self.edges[horz_edge].top);
        }

        if self.edges[horz_edge].next_in_lml == NIL {
            if self.edges[horz_edge].out_idx >= 0 {
                self.add_out_pt(horz_edge, self.edges[horz_edge].top);
            }
            self.delete_from_ael(horz_edge);
        } else if self.edges[horz_edge].out_idx >= 0 {
            op1 = self.add_out_pt(horz_edge, self.edges[horz_edge].top);
            horz_edge = self.update_edge_into_ael(horz_edge)?;
            if self.edges[horz_edge].wind_delta == 0 {
                return Ok(());
            }
            // nb: HorzEdge is no longer horizontal here
            let e_prev = self.edges[horz_edge].prev_in_ael;
            let e_next = self.edges[horz_edge].next_in_ael;
            let h = self.edges[horz_edge].clone();
            if e_prev != NIL
                && self.edges[e_prev].curr.x == h.bot.x
                && self.edges[e_prev].curr.y == h.bot.y
                && self.edges[e_prev].wind_delta != 0
                && self.edges[e_prev].out_idx >= 0
                && self.edges[e_prev].curr.y > self.edges[e_prev].top.y
                && self.slopes_equal_e(horz_edge, e_prev)
            {
                let op2 = self.add_out_pt(e_prev, h.bot);
                self.add_join(op1, op2, h.top);
            } else if e_next != NIL
                && self.edges[e_next].curr.x == h.bot.x
                && self.edges[e_next].curr.y == h.bot.y
                && self.edges[e_next].wind_delta != 0
                && self.edges[e_next].out_idx >= 0
                && self.edges[e_next].curr.y > self.edges[e_next].top.y
                && self.slopes_equal_e(horz_edge, e_next)
            {
                let op2 = self.add_out_pt(e_next, h.bot);
                self.add_join(op1, op2, h.top);
            }
        } else {
            self.update_edge_into_ael(horz_edge)?;
        }
        Ok(())
    }

    fn is_maxima(&self, e: usize, y: i64) -> bool {
        e != NIL && self.edges[e].top.y == y && self.edges[e].next_in_lml == NIL
    }

    fn is_intermediate(&self, e: usize, y: i64) -> bool {
        self.edges[e].top.y == y && self.edges[e].next_in_lml != NIL
    }

    fn get_maxima_pair(&self, e: usize) -> usize {
        let (n, p) = (self.edges[e].next, self.edges[e].prev);
        if self.edges[n].top == self.edges[e].top && self.edges[n].next_in_lml == NIL {
            n
        } else if self.edges[p].top == self.edges[e].top && self.edges[p].next_in_lml == NIL {
            p
        } else {
            NIL
        }
    }

    fn get_maxima_pair_ex(&self, e: usize) -> usize {
        // as above but returns null if MaxPair isn't in AEL (unless it's horizontal)
        let result = self.get_maxima_pair(e);
        if result == NIL
            || self.edges[result].out_idx == SKIP
            || (self.edges[result].next_in_ael == self.edges[result].prev_in_ael
                && !self.is_horizontal(result))
        {
            return NIL;
        }
        result
    }

    fn process_intersections(&mut self, top_y: i64) -> Result<bool, ClipperError> {
        if self.active_edges == NIL {
            return Ok(true);
        }
        self.build_intersect_list(top_y);
        if self.intersect_list.is_empty() {
            return Ok(true);
        }
        if self.intersect_list.len() == 1 || self.fixup_intersection_order() {
            self.process_intersect_list();
        } else {
            self.sorted_edges = NIL;
            self.intersect_list.clear();
            return Ok(false);
        }
        self.sorted_edges = NIL;
        Ok(true)
    }

    fn push_intersect_node(&mut self, node: IntersectNode) {
        // a List<T> grows its backing array 0 -> 4 -> 8 ... and keeps it on Clear():
        // the sort below depends on that length
        if self.intersect_list.len() == self.intersect_capacity {
            self.intersect_capacity = if self.intersect_capacity == 0 {
                4
            } else {
                self.intersect_capacity * 2
            };
        }
        self.intersect_list.push(node);
    }

    fn build_intersect_list(&mut self, top_y: i64) {
        if self.active_edges == NIL {
            return;
        }
        // prepare for sorting ...
        let mut e = self.active_edges;
        self.sorted_edges = e;
        while e != NIL {
            self.edges[e].prev_in_sel = self.edges[e].prev_in_ael;
            self.edges[e].next_in_sel = self.edges[e].next_in_ael;
            self.edges[e].curr.x = top_x(&self.edges[e], top_y);
            e = self.edges[e].next_in_ael;
        }

        // bubblesort ...
        let mut is_modified = true;
        while is_modified && self.sorted_edges != NIL {
            is_modified = false;
            e = self.sorted_edges;
            while self.edges[e].next_in_sel != NIL {
                let e_next = self.edges[e].next_in_sel;
                if self.edges[e].curr.x > self.edges[e_next].curr.x {
                    let mut pt = self.intersect_point(e, e_next);
                    if pt.y < top_y {
                        pt = IntPoint::new(top_x(&self.edges[e], top_y), top_y);
                    }
                    self.push_intersect_node(IntersectNode {
                        edge1: e,
                        edge2: e_next,
                        pt,
                    });
                    self.swap_positions_in_sel(e, e_next);
                    is_modified = true;
                } else {
                    e = e_next;
                }
            }
            let p = self.edges[e].prev_in_sel;
            if p == NIL {
                break;
            }
            self.edges[p].next_in_sel = NIL;
        }
        self.sorted_edges = NIL;
    }

    fn edges_adjacent(&self, node: &IntersectNode) -> bool {
        self.edges[node.edge1].next_in_sel == node.edge2 || self.edges[node.edge1].prev_in_sel == node.edge2
    }

    fn fixup_intersection_order(&mut self) -> bool {
        // pre-condition: intersections are sorted bottom-most first.
        // Now it's crucial that intersections are made only between adjacent edges,
        // so to ensure this the order of intersections may need adjusting ...
        introspective_sort(&mut self.intersect_list, self.intersect_capacity, &|n1, n2| {
            // MyIntersectNodeSort: descending Y
            let i = n2.pt.y - n1.pt.y;
            i.cmp(&0)
        });

        self.copy_ael_to_sel();
        let cnt = self.intersect_list.len();
        for i in 0..cnt {
            if !self.edges_adjacent(&self.intersect_list[i]) {
                let mut j = i + 1;
                while j < cnt && !self.edges_adjacent(&self.intersect_list[j]) {
                    j += 1;
                }
                if j == cnt {
                    return false;
                }
                self.intersect_list.swap(i, j);
            }
            let n = self.intersect_list[i];
            self.swap_positions_in_sel(n.edge1, n.edge2);
        }
        true
    }

    fn process_intersect_list(&mut self) {
        for i in 0..self.intersect_list.len() {
            let n = self.intersect_list[i];
            self.intersect_edges(n.edge1, n.edge2, n.pt);
            self.swap_positions_in_ael(n.edge1, n.edge2);
        }
        self.intersect_list.clear();
    }

    fn intersect_point(&self, e1: usize, e2: usize) -> IntPoint {
        let (edge1, edge2) = (&self.edges[e1], &self.edges[e2]);
        let mut ip = IntPoint::default();
        // nb: with very large coordinate values, it's possible for SlopesEqual() to
        // return false but for the edge.Dx value be equal due to double precision rounding.
        if edge1.dx == edge2.dx {
            ip.y = edge1.curr.y;
            ip.x = top_x(edge1, ip.y);
            return ip;
        }

        if edge1.delta.x == 0 {
            ip.x = edge1.bot.x;
            if edge2.delta.y == 0 {
                ip.y = edge2.bot.y;
            } else {
                let b2 = f(edge2.bot.y) - (f(edge2.bot.x) / edge2.dx);
                ip.y = round(f(ip.x) / edge2.dx + b2);
            }
        } else if edge2.delta.x == 0 {
            ip.x = edge2.bot.x;
            if edge1.delta.y == 0 {
                ip.y = edge1.bot.y;
            } else {
                let b1 = f(edge1.bot.y) - (f(edge1.bot.x) / edge1.dx);
                ip.y = round(f(ip.x) / edge1.dx + b1);
            }
        } else {
            let b1 = f(edge1.bot.x) - f(edge1.bot.y) * edge1.dx;
            let b2 = f(edge2.bot.x) - f(edge2.bot.y) * edge2.dx;
            let q = (b2 - b1) / (edge1.dx - edge2.dx);
            ip.y = round(q);
            if edge1.dx.abs() < edge2.dx.abs() {
                ip.x = round(edge1.dx * q + b1);
            } else {
                ip.x = round(edge2.dx * q + b2);
            }
        }

        if ip.y < edge1.top.y || ip.y < edge2.top.y {
            ip.y = if edge1.top.y > edge2.top.y {
                edge1.top.y
            } else {
                edge2.top.y
            };
            ip.x = if edge1.dx.abs() < edge2.dx.abs() {
                top_x(edge1, ip.y)
            } else {
                top_x(edge2, ip.y)
            };
        }
        // finally, don't allow 'ip' to be BELOW curr.Y (ie bottom of scanbeam) ...
        if ip.y > edge1.curr.y {
            ip.y = edge1.curr.y;
            // better to use the more vertical edge to derive X ...
            ip.x = if edge1.dx.abs() > edge2.dx.abs() {
                top_x(edge2, ip.y)
            } else {
                top_x(edge1, ip.y)
            };
        }
        ip
    }

    fn process_edges_at_top_of_scanbeam(&mut self, top_y: i64) -> Result<(), ClipperError> {
        let mut e = self.active_edges;
        while e != NIL {
            // 1. process maxima, treating them as if they're 'bent' horizontal edges,
            //    but exclude maxima with horizontal edges. nb: e can't be a horizontal.
            let mut is_maxima_edge = self.is_maxima(e, top_y);
            if is_maxima_edge {
                let e_max_pair = self.get_maxima_pair_ex(e);
                is_maxima_edge = e_max_pair == NIL || !self.is_horizontal(e_max_pair);
            }

            if is_maxima_edge {
                if self.strictly_simple {
                    self.insert_maxima(self.edges[e].top.x);
                }
                let e_prev = self.edges[e].prev_in_ael;
                self.do_maxima(e)?;
                e = if e_prev == NIL {
                    self.active_edges
                } else {
                    self.edges[e_prev].next_in_ael
                };
            } else {
                // 2. promote horizontal edges, otherwise update Curr.X and Curr.Y ...
                if self.is_intermediate(e, top_y) && self.is_horizontal(self.edges[e].next_in_lml) {
                    e = self.update_edge_into_ael(e)?;
                    if self.edges[e].out_idx >= 0 {
                        self.add_out_pt(e, self.edges[e].bot);
                    }
                    self.add_edge_to_sel(e);
                } else {
                    self.edges[e].curr.x = top_x(&self.edges[e], top_y);
                    self.edges[e].curr.y = top_y;
                }
                // When StrictlySimple and 'e' is being touched by another edge, then
                // make sure both edges have a vertex here ...
                if self.strictly_simple {
                    let e_prev = self.edges[e].prev_in_ael;
                    if self.edges[e].out_idx >= 0
                        && self.edges[e].wind_delta != 0
                        && e_prev != NIL
                        && self.edges[e_prev].out_idx >= 0
                        && self.edges[e_prev].curr.x == self.edges[e].curr.x
                        && self.edges[e_prev].wind_delta != 0
                    {
                        let ip = self.edges[e].curr;
                        let op = self.add_out_pt(e_prev, ip);
                        let op2 = self.add_out_pt(e, ip);
                        self.add_join(op, op2, ip); // StrictlySimple (type-3) join
                    }
                }
                e = self.edges[e].next_in_ael;
            }
        }

        // 3. Process horizontals at the Top of the scanbeam ...
        self.process_horizontals()?;
        self.maxima.clear();

        // 4. Promote intermediate vertices ...
        e = self.active_edges;
        while e != NIL {
            if self.is_intermediate(e, top_y) {
                let mut op = NIL;
                if self.edges[e].out_idx >= 0 {
                    op = self.add_out_pt(e, self.edges[e].top);
                }
                e = self.update_edge_into_ael(e)?;

                // if output polygons share an edge, they'll need joining later ...
                let e_prev = self.edges[e].prev_in_ael;
                let e_next = self.edges[e].next_in_ael;
                let ed = self.edges[e].clone();
                if e_prev != NIL
                    && self.edges[e_prev].curr.x == ed.bot.x
                    && self.edges[e_prev].curr.y == ed.bot.y
                    && op != NIL
                    && self.edges[e_prev].out_idx >= 0
                    && self.edges[e_prev].curr.y > self.edges[e_prev].top.y
                    && slopes_equal4(
                        ed.curr,
                        ed.top,
                        self.edges[e_prev].curr,
                        self.edges[e_prev].top,
                        self.use_full_range,
                    )
                    && ed.wind_delta != 0
                    && self.edges[e_prev].wind_delta != 0
                {
                    let op2 = self.add_out_pt(e_prev, ed.bot);
                    self.add_join(op, op2, ed.top);
                } else if e_next != NIL
                    && self.edges[e_next].curr.x == ed.bot.x
                    && self.edges[e_next].curr.y == ed.bot.y
                    && op != NIL
                    && self.edges[e_next].out_idx >= 0
                    && self.edges[e_next].curr.y > self.edges[e_next].top.y
                    && slopes_equal4(
                        ed.curr,
                        ed.top,
                        self.edges[e_next].curr,
                        self.edges[e_next].top,
                        self.use_full_range,
                    )
                    && ed.wind_delta != 0
                    && self.edges[e_next].wind_delta != 0
                {
                    let op2 = self.add_out_pt(e_next, ed.bot);
                    self.add_join(op, op2, ed.top);
                }
            }
            e = self.edges[e].next_in_ael;
        }
        Ok(())
    }

    fn do_maxima(&mut self, e: usize) -> Result<(), ClipperError> {
        let e_max_pair = self.get_maxima_pair_ex(e);
        if e_max_pair == NIL {
            if self.edges[e].out_idx >= 0 {
                self.add_out_pt(e, self.edges[e].top);
            }
            self.delete_from_ael(e);
            return Ok(());
        }

        let mut e_next = self.edges[e].next_in_ael;
        while e_next != NIL && e_next != e_max_pair {
            let top = self.edges[e].top;
            self.intersect_edges(e, e_next, top);
            self.swap_positions_in_ael(e, e_next);
            e_next = self.edges[e].next_in_ael;
        }

        if self.edges[e].out_idx == UNASSIGNED && self.edges[e_max_pair].out_idx == UNASSIGNED {
            self.delete_from_ael(e);
            self.delete_from_ael(e_max_pair);
        } else if self.edges[e].out_idx >= 0 && self.edges[e_max_pair].out_idx >= 0 {
            let top = self.edges[e].top;
            self.add_local_max_poly(e, e_max_pair, top);
            self.delete_from_ael(e);
            self.delete_from_ael(e_max_pair);
        } else if self.edges[e].wind_delta == 0 {
            if self.edges[e].out_idx >= 0 {
                self.add_out_pt(e, self.edges[e].top);
                self.edges[e].out_idx = UNASSIGNED;
            }
            self.delete_from_ael(e);
            if self.edges[e_max_pair].out_idx >= 0 {
                let top = self.edges[e].top;
                self.add_out_pt(e_max_pair, top);
                self.edges[e_max_pair].out_idx = UNASSIGNED;
            }
            self.delete_from_ael(e_max_pair);
        } else {
            return Err(ClipperError::Internal("DoMaxima error"));
        }
        Ok(())
    }

    fn point_count(&self, pts: usize) -> usize {
        if pts == NIL {
            return 0;
        }
        let mut result = 0;
        let mut p = pts;
        loop {
            result += 1;
            p = self.out_pts[p].next;
            if p == pts {
                break;
            }
        }
        result
    }

    fn build_result(&self) -> Paths {
        let mut polyg = Vec::with_capacity(self.poly_outs.len());
        for rec in &self.poly_outs {
            if rec.pts == NIL {
                continue;
            }
            let mut p = self.out_pts[rec.pts].prev;
            let cnt = self.point_count(p);
            if cnt < 2 {
                continue;
            }
            let mut pg = Vec::with_capacity(cnt);
            for _ in 0..cnt {
                pg.push(self.out_pts[p].pt);
                p = self.out_pts[p].prev;
            }
            polyg.push(pg);
        }
        polyg
    }

    fn build_result2(&mut self) -> PolyTree {
        let mut tree = PolyTree::new();
        // add each output polygon/contour to polytree ...
        for i in 0..self.poly_outs.len() {
            let cnt = self.point_count(self.poly_outs[i].pts);
            let open = self.poly_outs[i].is_open;
            if (open && cnt < 2) || (!open && cnt < 3) {
                continue;
            }
            self.fix_hole_linkage(i);
            let mut contour = Vec::with_capacity(cnt);
            let mut op = self.out_pts[self.poly_outs[i].pts].prev;
            for _ in 0..cnt {
                contour.push(self.out_pts[op].pt);
                op = self.out_pts[op].prev;
            }
            self.poly_outs[i].poly_node = tree.new_node(contour);
        }

        // fixup PolyNode links etc ...
        for i in 0..self.poly_outs.len() {
            let rec = &self.poly_outs[i];
            if rec.poly_node == NIL {
                continue;
            } else if rec.is_open {
                tree.set_open(rec.poly_node);
                tree.add_child(PolyTree::ROOT, rec.poly_node);
            } else if rec.first_left != NIL && self.poly_outs[rec.first_left].poly_node != NIL {
                tree.add_child(self.poly_outs[rec.first_left].poly_node, rec.poly_node);
            } else {
                tree.add_child(PolyTree::ROOT, rec.poly_node);
            }
        }
        tree
    }

    fn fixup_out_polyline(&mut self, rec: usize) {
        let mut pp = self.poly_outs[rec].pts;
        let mut last_pp = self.out_pts[pp].prev;
        while pp != last_pp {
            pp = self.out_pts[pp].next;
            if self.out_pts[pp].pt == self.out_pts[self.out_pts[pp].prev].pt {
                if pp == last_pp {
                    last_pp = self.out_pts[pp].prev;
                }
                let tmp_pp = self.out_pts[pp].prev;
                let pn = self.out_pts[pp].next;
                self.out_pts[tmp_pp].next = pn;
                self.out_pts[pn].prev = tmp_pp;
                pp = tmp_pp;
            }
        }
        if pp == self.out_pts[pp].prev {
            self.poly_outs[rec].pts = NIL;
        }
    }

    fn fixup_out_polygon(&mut self, rec: usize) {
        // FixupOutPolygon() - removes duplicate points and simplifies consecutive
        // parallel edges by removing the middle vertex.
        let mut last_ok = NIL;
        self.poly_outs[rec].bottom_pt = NIL;
        let mut pp = self.poly_outs[rec].pts;
        let preserve_col = self.preserve_collinear || self.strictly_simple;
        loop {
            let (prev, next) = (self.out_pts[pp].prev, self.out_pts[pp].next);
            if prev == pp || prev == next {
                self.poly_outs[rec].pts = NIL;
                return;
            }
            // test for duplicate points and collinear edges ...
            let (a, b, c) = (self.out_pts[prev].pt, self.out_pts[pp].pt, self.out_pts[next].pt);
            if b == c
                || b == a
                || (slopes_equal3(a, b, c, self.use_full_range)
                    && (!preserve_col || !pt2_is_between_pt1_and_pt3(a, b, c)))
            {
                last_ok = NIL;
                self.out_pts[prev].next = next;
                self.out_pts[next].prev = prev;
                pp = prev;
            } else if pp == last_ok {
                break;
            } else {
                if last_ok == NIL {
                    last_ok = pp;
                }
                pp = next;
            }
        }
        self.poly_outs[rec].pts = pp;
    }

    fn dup_out_pt(&mut self, out_pt: usize, insert_after: bool) -> usize {
        let src = self.out_pts[out_pt];
        let result = self.new_out_pt(src.idx, src.pt);
        if insert_after {
            self.out_pts[result].next = src.next;
            self.out_pts[result].prev = out_pt;
            self.out_pts[src.next].prev = result;
            self.out_pts[out_pt].next = result;
        } else {
            self.out_pts[result].prev = src.prev;
            self.out_pts[result].next = out_pt;
            self.out_pts[src.prev].next = result;
            self.out_pts[out_pt].prev = result;
        }
        result
    }

    #[allow(clippy::too_many_lines)]
    fn join_horz(
        &mut self,
        mut op1: usize,
        mut op1b: usize,
        mut op2: usize,
        mut op2b: usize,
        pt: IntPoint,
        discard_left: bool,
    ) -> bool {
        let dir1 = if self.out_pts[op1].pt.x > self.out_pts[op1b].pt.x {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        };
        let dir2 = if self.out_pts[op2].pt.x > self.out_pts[op2b].pt.x {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        };
        if dir1 == dir2 {
            return false;
        }

        // When DiscardLeft, we want Op1b to be on the Left of Op1, otherwise we
        // want Op1b to be on the Right. (And likewise with Op2 and Op2b.)
        // So, to facilitate this while inserting Op1b and Op2b ...
        // when DiscardLeft, make sure we're AT or RIGHT of Pt before adding Op1b,
        // otherwise make sure we're AT or LEFT of Pt. (Likewise with Op2b.)
        let (n1, n2) = self.join_horz_side(op1, dir1, pt, discard_left);
        op1 = n1;
        op1b = n2;
        let (n1, n2) = self.join_horz_side(op2, dir2, pt, discard_left);
        op2 = n1;
        op2b = n2;

        if (dir1 == Direction::LeftToRight) == discard_left {
            self.out_pts[op1].prev = op2;
            self.out_pts[op2].next = op1;
            self.out_pts[op1b].next = op2b;
            self.out_pts[op2b].prev = op1b;
        } else {
            self.out_pts[op1].next = op2;
            self.out_pts[op2].prev = op1;
            self.out_pts[op1b].prev = op2b;
            self.out_pts[op2b].next = op1b;
        }
        true
    }

    /// One half of `JoinHorz` (the code is the same for op1/op1b and op2/op2b).
    fn join_horz_side(
        &mut self,
        mut op: usize,
        dir: Direction,
        pt: IntPoint,
        discard_left: bool,
    ) -> (usize, usize) {
        let opb;
        if dir == Direction::LeftToRight {
            while self.out_pts[self.out_pts[op].next].pt.x <= pt.x
                && self.out_pts[self.out_pts[op].next].pt.x >= self.out_pts[op].pt.x
                && self.out_pts[self.out_pts[op].next].pt.y == pt.y
            {
                op = self.out_pts[op].next;
            }
            if discard_left && self.out_pts[op].pt.x != pt.x {
                op = self.out_pts[op].next;
            }
            let mut b = self.dup_out_pt(op, !discard_left);
            if self.out_pts[b].pt != pt {
                op = b;
                self.out_pts[op].pt = pt;
                b = self.dup_out_pt(op, !discard_left);
            }
            opb = b;
        } else {
            while self.out_pts[self.out_pts[op].next].pt.x >= pt.x
                && self.out_pts[self.out_pts[op].next].pt.x <= self.out_pts[op].pt.x
                && self.out_pts[self.out_pts[op].next].pt.y == pt.y
            {
                op = self.out_pts[op].next;
            }
            if !discard_left && self.out_pts[op].pt.x != pt.x {
                op = self.out_pts[op].next;
            }
            let mut b = self.dup_out_pt(op, discard_left);
            if self.out_pts[b].pt != pt {
                op = b;
                self.out_pts[op].pt = pt;
                b = self.dup_out_pt(op, discard_left);
            }
            opb = b;
        }
        (op, opb)
    }

    #[allow(clippy::too_many_lines)]
    fn join_points(&mut self, j: &mut Join, rec1: usize, rec2: usize) -> bool {
        let mut op1 = j.out_pt1;
        let mut op2 = j.out_pt2;
        let mut op1b;
        let mut op2b;

        // There are 3 kinds of joins for output polygons ...
        // 1. Horizontal joins where Join.OutPt1 & Join.OutPt2 are vertices anywhere
        // along (horizontal) collinear edges (& Join.OffPt is on the same horizontal).
        // 2. Non-horizontal joins where Join.OutPt1 & Join.OutPt2 are at the same
        // location at the Bottom of the overlapping segment (& Join.OffPt is above).
        // 3. StrictlySimple joins where edges touch but are not collinear and where
        // Join.OutPt1, Join.OutPt2 & Join.OffPt all share the same point.
        let is_horizontal = self.out_pts[j.out_pt1].pt.y == j.off_pt.y;

        if is_horizontal && j.off_pt == self.out_pts[j.out_pt1].pt && j.off_pt == self.out_pts[j.out_pt2].pt {
            // Strictly Simple join ...
            if rec1 != rec2 {
                return false;
            }
            op1b = self.out_pts[j.out_pt1].next;
            while op1b != op1 && self.out_pts[op1b].pt == j.off_pt {
                op1b = self.out_pts[op1b].next;
            }
            let reverse1 = self.out_pts[op1b].pt.y > j.off_pt.y;
            op2b = self.out_pts[j.out_pt2].next;
            while op2b != op2 && self.out_pts[op2b].pt == j.off_pt {
                op2b = self.out_pts[op2b].next;
            }
            let reverse2 = self.out_pts[op2b].pt.y > j.off_pt.y;
            if reverse1 == reverse2 {
                return false;
            }
            self.link_join(j, op1, op2, reverse1);
            true
        } else if is_horizontal {
            // treat horizontal joins differently to non-horizontal joins since with
            // them we're not yet sure where the overlapping is. OutPt1.Pt & OutPt2.Pt
            // may be anywhere along the horizontal edge.
            op1b = op1;
            while self.out_pts[self.out_pts[op1].prev].pt.y == self.out_pts[op1].pt.y
                && self.out_pts[op1].prev != op1b
                && self.out_pts[op1].prev != op2
            {
                op1 = self.out_pts[op1].prev;
            }
            while self.out_pts[self.out_pts[op1b].next].pt.y == self.out_pts[op1b].pt.y
                && self.out_pts[op1b].next != op1
                && self.out_pts[op1b].next != op2
            {
                op1b = self.out_pts[op1b].next;
            }
            if self.out_pts[op1b].next == op1 || self.out_pts[op1b].next == op2 {
                return false; // a flat 'polygon'
            }

            op2b = op2;
            while self.out_pts[self.out_pts[op2].prev].pt.y == self.out_pts[op2].pt.y
                && self.out_pts[op2].prev != op2b
                && self.out_pts[op2].prev != op1b
            {
                op2 = self.out_pts[op2].prev;
            }
            while self.out_pts[self.out_pts[op2b].next].pt.y == self.out_pts[op2b].pt.y
                && self.out_pts[op2b].next != op2
                && self.out_pts[op2b].next != op1
            {
                op2b = self.out_pts[op2b].next;
            }
            if self.out_pts[op2b].next == op2 || self.out_pts[op2b].next == op1 {
                return false; // a flat 'polygon'
            }

            // Op1 -. Op1b & Op2 -. Op2b are the extremites of the horizontal edges
            let (p1, p1b, p2, p2b) = (
                self.out_pts[op1].pt,
                self.out_pts[op1b].pt,
                self.out_pts[op2].pt,
                self.out_pts[op2b].pt,
            );
            let Some((left, right)) = get_overlap(p1.x, p1b.x, p2.x, p2b.x) else {
                return false;
            };

            // DiscardLeftSide: when overlapping edges are joined, a spike will created
            // which needs to be cleaned up. However, we don't want Op1 or Op2 caught up
            // on the discard Side as either may still be needed for other joins ...
            let (pt, discard_left_side) = if p1.x >= left && p1.x <= right {
                (p1, p1.x > p1b.x)
            } else if p2.x >= left && p2.x <= right {
                (p2, p2.x > p2b.x)
            } else if p1b.x >= left && p1b.x <= right {
                (p1b, p1b.x > p1.x)
            } else {
                (p2b, p2b.x > p2.x)
            };
            j.out_pt1 = op1;
            j.out_pt2 = op2;
            self.join_horz(op1, op1b, op2, op2b, pt, discard_left_side)
        } else {
            // nb: For non-horizontal joins ...
            //    1. Jr.OutPt1.Pt.Y == Jr.OutPt2.Pt.Y
            //    2. Jr.OutPt1.Pt > Jr.OffPt.Y

            // make sure the polygons are correctly oriented ...
            op1b = self.out_pts[op1].next;
            while self.out_pts[op1b].pt == self.out_pts[op1].pt && op1b != op1 {
                op1b = self.out_pts[op1b].next;
            }
            let reverse1 = self.out_pts[op1b].pt.y > self.out_pts[op1].pt.y
                || !slopes_equal3(
                    self.out_pts[op1].pt,
                    self.out_pts[op1b].pt,
                    j.off_pt,
                    self.use_full_range,
                );
            if reverse1 {
                op1b = self.out_pts[op1].prev;
                while self.out_pts[op1b].pt == self.out_pts[op1].pt && op1b != op1 {
                    op1b = self.out_pts[op1b].prev;
                }
                if self.out_pts[op1b].pt.y > self.out_pts[op1].pt.y
                    || !slopes_equal3(
                        self.out_pts[op1].pt,
                        self.out_pts[op1b].pt,
                        j.off_pt,
                        self.use_full_range,
                    )
                {
                    return false;
                }
            }
            op2b = self.out_pts[op2].next;
            while self.out_pts[op2b].pt == self.out_pts[op2].pt && op2b != op2 {
                op2b = self.out_pts[op2b].next;
            }
            let reverse2 = self.out_pts[op2b].pt.y > self.out_pts[op2].pt.y
                || !slopes_equal3(
                    self.out_pts[op2].pt,
                    self.out_pts[op2b].pt,
                    j.off_pt,
                    self.use_full_range,
                );
            if reverse2 {
                op2b = self.out_pts[op2].prev;
                while self.out_pts[op2b].pt == self.out_pts[op2].pt && op2b != op2 {
                    op2b = self.out_pts[op2b].prev;
                }
                if self.out_pts[op2b].pt.y > self.out_pts[op2].pt.y
                    || !slopes_equal3(
                        self.out_pts[op2].pt,
                        self.out_pts[op2b].pt,
                        j.off_pt,
                        self.use_full_range,
                    )
                {
                    return false;
                }
            }

            if op1b == op1 || op2b == op2 || op1b == op2b || (rec1 == rec2 && reverse1 == reverse2) {
                return false;
            }
            self.link_join(j, op1, op2, reverse1);
            true
        }
    }

    /// The common tail of the strictly simple and non-horizontal joins.
    fn link_join(&mut self, j: &mut Join, op1: usize, op2: usize, reverse1: bool) {
        if reverse1 {
            let op1b = self.dup_out_pt(op1, false);
            let op2b = self.dup_out_pt(op2, true);
            self.out_pts[op1].prev = op2;
            self.out_pts[op2].next = op1;
            self.out_pts[op1b].next = op2b;
            self.out_pts[op2b].prev = op1b;
            j.out_pt1 = op1;
            j.out_pt2 = op1b;
        } else {
            let op1b = self.dup_out_pt(op1, true);
            let op2b = self.dup_out_pt(op2, false);
            self.out_pts[op1].next = op2;
            self.out_pts[op2].prev = op1;
            self.out_pts[op1b].prev = op2b;
            self.out_pts[op2b].next = op1b;
            j.out_pt1 = op1;
            j.out_pt2 = op1b;
        }
    }

    fn point_in_out_polygon(&self, pt: IntPoint, mut op: usize) -> i32 {
        // returns 0 if false, +1 if true, -1 if pt ON polygon boundary
        let mut result = 0;
        let start_op = op;
        let (ptx, pty) = (pt.x, pt.y);
        let (mut poly0x, mut poly0y) = (self.out_pts[op].pt.x, self.out_pts[op].pt.y);
        loop {
            op = self.out_pts[op].next;
            let (poly1x, poly1y) = (self.out_pts[op].pt.x, self.out_pts[op].pt.y);
            if poly1y == pty && (poly1x == ptx || (poly0y == pty && ((poly1x > ptx) == (poly0x < ptx)))) {
                return -1;
            }
            if (poly0y < pty) != (poly1y < pty) {
                if poly0x >= ptx {
                    if poly1x > ptx {
                        result = 1 - result;
                    } else {
                        let d = f(poly0x - ptx) * f(poly1y - pty) - f(poly1x - ptx) * f(poly0y - pty);
                        if d == 0.0 {
                            return -1;
                        }
                        if (d > 0.0) == (poly1y > poly0y) {
                            result = 1 - result;
                        }
                    }
                } else if poly1x > ptx {
                    let d = f(poly0x - ptx) * f(poly1y - pty) - f(poly1x - ptx) * f(poly0y - pty);
                    if d == 0.0 {
                        return -1;
                    }
                    if (d > 0.0) == (poly1y > poly0y) {
                        result = 1 - result;
                    }
                }
            }
            poly0x = poly1x;
            poly0y = poly1y;
            if start_op == op {
                break;
            }
        }
        result
    }

    fn poly2_contains_poly1(&self, out_pt1: usize, out_pt2: usize) -> bool {
        let mut op = out_pt1;
        loop {
            // nb: PointInPolygon returns 0 if false, +1 if true, -1 if pt on polygon
            let res = self.point_in_out_polygon(self.out_pts[op].pt, out_pt2);
            if res >= 0 {
                return res > 0;
            }
            op = self.out_pts[op].next;
            if op == out_pt1 {
                break;
            }
        }
        true
    }

    fn fixup_first_lefts1(&mut self, old_rec: usize, new_rec: usize) {
        for i in 0..self.poly_outs.len() {
            let first_left = self.parse_first_left(self.poly_outs[i].first_left);
            if self.poly_outs[i].pts != NIL
                && first_left == old_rec
                && self.poly2_contains_poly1(self.poly_outs[i].pts, self.poly_outs[new_rec].pts)
            {
                self.poly_outs[i].first_left = new_rec;
            }
        }
    }

    fn fixup_first_lefts2(&mut self, inner: usize, outer: usize) {
        // A polygon has split into two such that one is now the inner of the other.
        // It's possible that these polygons now wrap around other polygons, so check
        // every polygon that's also contained by OuterOutRec's FirstLeft container
        // (including nil) to see if they've become inner to the new inner polygon ...
        let orfl = self.poly_outs[outer].first_left;
        for i in 0..self.poly_outs.len() {
            if self.poly_outs[i].pts == NIL || i == outer || i == inner {
                continue;
            }
            let first_left = self.parse_first_left(self.poly_outs[i].first_left);
            if first_left != orfl && first_left != inner && first_left != outer {
                continue;
            }
            if self.poly2_contains_poly1(self.poly_outs[i].pts, self.poly_outs[inner].pts) {
                self.poly_outs[i].first_left = inner;
            } else if self.poly2_contains_poly1(self.poly_outs[i].pts, self.poly_outs[outer].pts) {
                self.poly_outs[i].first_left = outer;
            } else if self.poly_outs[i].first_left == inner || self.poly_outs[i].first_left == outer {
                self.poly_outs[i].first_left = orfl;
            }
        }
    }

    fn fixup_first_lefts3(&mut self, old_rec: usize, new_rec: usize) {
        // same as FixupFirstLefts1 but doesn't call Poly2ContainsPoly1()
        for i in 0..self.poly_outs.len() {
            let first_left = self.parse_first_left(self.poly_outs[i].first_left);
            if self.poly_outs[i].pts != NIL && first_left == old_rec {
                self.poly_outs[i].first_left = new_rec;
            }
        }
    }

    fn parse_first_left(&self, mut first_left: usize) -> usize {
        while first_left != NIL && self.poly_outs[first_left].pts == NIL {
            first_left = self.poly_outs[first_left].first_left;
        }
        first_left
    }

    fn join_common_edges(&mut self) {
        for i in 0..self.joins.len() {
            let mut join = self.joins[i];
            let rec1 = self.get_out_rec(self.out_pts[join.out_pt1].idx);
            let mut rec2 = self.get_out_rec(self.out_pts[join.out_pt2].idx);

            if self.poly_outs[rec1].pts == NIL || self.poly_outs[rec2].pts == NIL {
                continue;
            }
            if self.poly_outs[rec1].is_open || self.poly_outs[rec2].is_open {
                continue;
            }

            // get the polygon fragment with the correct hole state (FirstLeft)
            // before calling JoinPoints() ...
            let hole_state_rec = if rec1 == rec2 {
                rec1
            } else if self.out_rec1_right_of_out_rec2(rec1, rec2) {
                rec2
            } else if self.out_rec1_right_of_out_rec2(rec2, rec1) {
                rec1
            } else {
                self.get_lowermost_rec(rec1, rec2)
            };

            let joined = self.join_points(&mut join, rec1, rec2);
            self.joins[i] = join;
            if !joined {
                continue;
            }

            if rec1 == rec2 {
                // instead of joining two polygons, we've just created a new one by
                // splitting one polygon into two.
                self.poly_outs[rec1].pts = join.out_pt1;
                self.poly_outs[rec1].bottom_pt = NIL;
                rec2 = self.create_out_rec();
                self.poly_outs[rec2].pts = join.out_pt2;

                // update all OutRec2.Pts Idx's ...
                self.update_out_pt_idxs(rec2);

                if self.poly2_contains_poly1(self.poly_outs[rec2].pts, self.poly_outs[rec1].pts) {
                    // outRec1 contains outRec2 ...
                    self.poly_outs[rec2].is_hole = !self.poly_outs[rec1].is_hole;
                    self.poly_outs[rec2].first_left = rec1;
                    if self.using_poly_tree {
                        self.fixup_first_lefts2(rec2, rec1);
                    }
                    if (self.poly_outs[rec2].is_hole ^ self.reverse_solution) == (self.area_rec(rec2) > 0.0) {
                        self.reverse_poly_pt_links(self.poly_outs[rec2].pts);
                    }
                } else if self.poly2_contains_poly1(self.poly_outs[rec1].pts, self.poly_outs[rec2].pts) {
                    // outRec2 contains outRec1 ...
                    self.poly_outs[rec2].is_hole = self.poly_outs[rec1].is_hole;
                    self.poly_outs[rec1].is_hole = !self.poly_outs[rec2].is_hole;
                    self.poly_outs[rec2].first_left = self.poly_outs[rec1].first_left;
                    self.poly_outs[rec1].first_left = rec2;
                    if self.using_poly_tree {
                        self.fixup_first_lefts2(rec1, rec2);
                    }
                    if (self.poly_outs[rec1].is_hole ^ self.reverse_solution) == (self.area_rec(rec1) > 0.0) {
                        self.reverse_poly_pt_links(self.poly_outs[rec1].pts);
                    }
                } else {
                    // the 2 polygons are completely separate ...
                    self.poly_outs[rec2].is_hole = self.poly_outs[rec1].is_hole;
                    self.poly_outs[rec2].first_left = self.poly_outs[rec1].first_left;
                    // fixup FirstLeft pointers that may need reassigning to OutRec2
                    if self.using_poly_tree {
                        self.fixup_first_lefts1(rec1, rec2);
                    }
                }
            } else {
                // joined 2 polygons together ...
                self.poly_outs[rec2].pts = NIL;
                self.poly_outs[rec2].bottom_pt = NIL;
                self.poly_outs[rec2].idx = self.poly_outs[rec1].idx;

                self.poly_outs[rec1].is_hole = self.poly_outs[hole_state_rec].is_hole;
                if hole_state_rec == rec2 {
                    self.poly_outs[rec1].first_left = self.poly_outs[rec2].first_left;
                }
                self.poly_outs[rec2].first_left = rec1;

                // fixup FirstLeft pointers that may need reassigning to OutRec1
                if self.using_poly_tree {
                    self.fixup_first_lefts3(rec2, rec1);
                }
            }
        }
    }

    fn update_out_pt_idxs(&mut self, rec: usize) {
        let start = self.poly_outs[rec].pts;
        let idx = self.poly_outs[rec].idx;
        let mut op = start;
        loop {
            self.out_pts[op].idx = idx;
            op = self.out_pts[op].prev;
            if op == start {
                break;
            }
        }
    }

    fn do_simple_polygons(&mut self) {
        let mut i = 0;
        while i < self.poly_outs.len() {
            let rec = i;
            i += 1;
            let mut op = self.poly_outs[rec].pts;
            if op == NIL || self.poly_outs[rec].is_open {
                continue;
            }
            loop {
                // for each Pt in Polygon until duplicate found do ...
                let mut op2 = self.out_pts[op].next;
                while op2 != self.poly_outs[rec].pts {
                    if self.out_pts[op].pt == self.out_pts[op2].pt
                        && self.out_pts[op2].next != op
                        && self.out_pts[op2].prev != op
                    {
                        // split the polygon into two ...
                        let op3 = self.out_pts[op].prev;
                        let op4 = self.out_pts[op2].prev;
                        self.out_pts[op].prev = op4;
                        self.out_pts[op4].next = op;
                        self.out_pts[op2].prev = op3;
                        self.out_pts[op3].next = op2;

                        self.poly_outs[rec].pts = op;
                        let rec2 = self.create_out_rec();
                        self.poly_outs[rec2].pts = op2;
                        self.update_out_pt_idxs(rec2);
                        if self.poly2_contains_poly1(self.poly_outs[rec2].pts, self.poly_outs[rec].pts) {
                            // OutRec2 is contained by OutRec1 ...
                            self.poly_outs[rec2].is_hole = !self.poly_outs[rec].is_hole;
                            self.poly_outs[rec2].first_left = rec;
                            if self.using_poly_tree {
                                self.fixup_first_lefts2(rec2, rec);
                            }
                        } else if self.poly2_contains_poly1(self.poly_outs[rec].pts, self.poly_outs[rec2].pts)
                        {
                            // OutRec1 is contained by OutRec2 ...
                            self.poly_outs[rec2].is_hole = self.poly_outs[rec].is_hole;
                            self.poly_outs[rec].is_hole = !self.poly_outs[rec2].is_hole;
                            self.poly_outs[rec2].first_left = self.poly_outs[rec].first_left;
                            self.poly_outs[rec].first_left = rec2;
                            if self.using_poly_tree {
                                self.fixup_first_lefts2(rec, rec2);
                            }
                        } else {
                            // the 2 polygons are separate ...
                            self.poly_outs[rec2].is_hole = self.poly_outs[rec].is_hole;
                            self.poly_outs[rec2].first_left = self.poly_outs[rec].first_left;
                            if self.using_poly_tree {
                                self.fixup_first_lefts1(rec, rec2);
                            }
                        }
                        op2 = op; // ie get ready for the next iteration
                    }
                    op2 = self.out_pts[op2].next;
                }
                op = self.out_pts[op].next;
                if op == self.poly_outs[rec].pts {
                    break;
                }
            }
        }
    }

    fn area_rec(&self, rec: usize) -> f64 {
        self.area_op(self.poly_outs[rec].pts)
    }

    fn area_op(&self, op: usize) -> f64 {
        if op == NIL {
            return 0.0;
        }
        let first = op;
        let mut op = op;
        let mut a = 0.0;
        loop {
            let prev = self.out_pts[op].prev;
            a += f(self.out_pts[prev].pt.x + self.out_pts[op].pt.x)
                * f(self.out_pts[prev].pt.y - self.out_pts[op].pt.y);
            op = self.out_pts[op].next;
            if op == first {
                break;
            }
        }
        a * 0.5
    }
}

fn top_x(edge: &TEdge, current_y: i64) -> i64 {
    if current_y == edge.top.y {
        return edge.top.x;
    }
    edge.bot.x + round(edge.dx * f(current_y - edge.bot.y))
}

fn get_overlap(a1: i64, a2: i64, b1: i64, b2: i64) -> Option<(i64, i64)> {
    let (left, right) = if a1 < a2 {
        if b1 < b2 {
            (a1.max(b1), a2.min(b2))
        } else {
            (a1.max(b2), a2.min(b1))
        }
    } else if b1 < b2 {
        (a2.max(b1), a1.min(b2))
    } else {
        (a2.max(b2), a1.min(b1))
    };
    (left < right).then_some((left, right))
}

/// `Clipper.GetBounds`: (left, top, right, bottom) of all points.
#[must_use]
pub fn get_bounds(paths: &[Path]) -> (i64, i64, i64, i64) {
    let Some(first) = paths.iter().find(|p| !p.is_empty()) else {
        return (0, 0, 0, 0);
    };
    let (mut left, mut top) = (first[0].x, first[0].y);
    let (mut right, mut bottom) = (left, top);
    let start = paths.iter().position(|p| !p.is_empty()).unwrap_or(0);
    for p in &paths[start..] {
        for pt in p {
            if pt.x < left {
                left = pt.x;
            } else if pt.x > right {
                right = pt.x;
            }
            if pt.y < top {
                top = pt.y;
            } else if pt.y > bottom {
                bottom = pt.y;
            }
        }
    }
    (left, top, right, bottom)
}
