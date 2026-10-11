//! Ordering of paths to shorten the travel between them: LaserGRBL's
//! `GrblFile.OptimizePaths` (greedy nearest neighbour from the path that starts
//! nearest to the origin) and `ParallelOptimizePaths` (the same on blocks of 2048
//! paths, concatenated in order).

use crate::Point;

/// Paths per block of [`nearest_neighbour_blocks`].
pub const BLOCK_SIZE: usize = 2048;

/// Greedy nearest-neighbour order of paths given by their first and last points.
/// Returns indices into `ends`. Ties go to the lower index, as in LaserGRBL.
#[must_use]
pub fn nearest_neighbour(ends: &[(Point, Point)]) -> Vec<usize> {
    let n = ends.len();
    if n <= 1 {
        return (0..n).collect();
    }
    let sq = |a: Point, b: Point| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        dx * dx + dy * dy
    };
    // find the one that starts nearest to the origin
    let mut nearest = 0;
    let mut best = f64::MAX;
    for (i, (first, _)) in ends.iter().enumerate() {
        let d = first.x * first.x + first.y * first.y;
        if d < best {
            nearest = i;
            best = d;
        }
    }
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    order.push(nearest);
    visited[nearest] = true;
    let mut last = nearest;
    while order.len() < n {
        let mut best_index = 0;
        let mut best_distance = f64::MAX;
        for next in 0..n {
            if visited[next] {
                continue;
            }
            let d = sq(ends[last].1, ends[next].0);
            if d < best_distance {
                best_index = next;
                best_distance = d;
            }
        }
        order.push(best_index);
        visited[best_index] = true;
        last = best_index;
    }
    order
}

/// [`nearest_neighbour`] on blocks of [`BLOCK_SIZE`] (`ParallelOptimizePaths`).
#[must_use]
pub fn nearest_neighbour_blocks(ends: &[(Point, Point)]) -> Vec<usize> {
    let n = ends.len();
    if n <= 1 {
        return (0..n).collect();
    }
    let blocks = n.div_ceil(BLOCK_SIZE);
    if blocks <= 1 {
        return nearest_neighbour(ends);
    }
    let mut out = Vec::with_capacity(n);
    for b in 0..blocks {
        let from = b * n / blocks;
        let to = (b + 1) * n / blocks;
        out.extend(nearest_neighbour(&ends[from..to]).into_iter().map(|i| i + from));
    }
    out
}

/// Reorders `items` by `order` (indices from the functions above).
#[must_use]
pub fn apply<T: Clone>(items: &[T], order: &[usize]) -> Vec<T> {
    order.iter().map(|&i| items[i].clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(x0: f64, x1: f64) -> (Point, Point) {
        (Point::new(x0, 0.0), Point::new(x1, 0.0))
    }

    #[test]
    fn starts_near_the_origin_and_hops_to_the_nearest() {
        let ends = vec![seg(10.0, 11.0), seg(0.5, 1.0), seg(20.0, 21.0), seg(1.5, 9.0)];
        assert_eq!(nearest_neighbour(&ends), vec![1, 3, 0, 2]);
        assert_eq!(nearest_neighbour(&[]), Vec::<usize>::new());
        assert_eq!(nearest_neighbour(&[seg(1.0, 2.0)]), vec![0]);
        // ties go to the lower index
        let tie = vec![seg(0.0, 1.0), seg(2.0, 3.0), seg(0.0, 9.0)];
        assert_eq!(nearest_neighbour(&tie)[0], 0);
        assert_eq!(apply(&["a", "b", "c"], &[2, 0, 1]), vec!["c", "a", "b"]);
    }

    #[test]
    fn blocks_are_ordered_independently() {
        let ends: Vec<(Point, Point)> = (0..5000)
            .map(|i| seg(f64::from(5000 - i), f64::from(5000 - i)))
            .collect();
        let order = nearest_neighbour_blocks(&ends);
        assert_eq!(order.len(), 5000);
        // three blocks: each starts at its own path nearest to the origin
        assert_eq!(order[0], 1665);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..5000).collect::<Vec<_>>());
        assert_eq!(
            nearest_neighbour_blocks(&ends[..10]),
            nearest_neighbour(&ends[..10])
        );
        assert!(nearest_neighbour_blocks(&[]).is_empty());
    }
}
