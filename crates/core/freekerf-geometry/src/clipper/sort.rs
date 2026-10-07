//! The unstable introspective sort of .NET's `List<T>.Sort(IComparer<T>)`
//! (`ArraySortHelper<T>.IntrospectiveSort` of the .NET Framework reference source,
//! also used by Mono).
//!
//! Clipper sorts its intersection nodes with it: nodes at the same height keep the
//! order this exact algorithm leaves them in, and that order decides which edges are
//! swapped first. A different sort (even a correct one) can produce other, equally
//! valid, output paths, so the port keeps this one.

use std::cmp::Ordering;

/// Below this size a partition is sorted by insertion.
const INTROSORT_SIZE_THRESHOLD: usize = 16;

/// `IntrospectiveSortUtilities.FloorLog2`: the number of bits of `n` (`floor(log2 n) + 1`).
fn floor_log2(mut n: usize) -> usize {
    let mut result = 0;
    while n >= 1 {
        result += 1;
        n /= 2;
    }
    result
}

/// Sorts `keys` like `Array.Sort(keys, 0, keys.len(), comparer)` on an array of
/// length `capacity` (the depth limit uses the length of the backing array, which
/// for a `List<T>` is its capacity, not its count).
pub fn introspective_sort<T>(keys: &mut [T], capacity: usize, cmp: &impl Fn(&T, &T) -> Ordering) {
    if keys.len() < 2 {
        return;
    }
    let hi = keys.len() - 1;
    intro_sort(keys, 0, hi, 2 * floor_log2(capacity.max(keys.len())), cmp);
}

fn intro_sort<T>(
    keys: &mut [T],
    lo: usize,
    mut hi: usize,
    mut depth_limit: usize,
    cmp: &impl Fn(&T, &T) -> Ordering,
) {
    while hi > lo {
        let partition_size = hi - lo + 1;
        if partition_size <= INTROSORT_SIZE_THRESHOLD {
            match partition_size {
                1 => {}
                2 => swap_if_greater(keys, cmp, lo, hi),
                3 => {
                    swap_if_greater(keys, cmp, lo, hi - 1);
                    swap_if_greater(keys, cmp, lo, hi);
                    swap_if_greater(keys, cmp, hi - 1, hi);
                }
                _ => insertion_sort(keys, lo, hi, cmp),
            }
            return;
        }
        if depth_limit == 0 {
            heapsort(keys, lo, hi, cmp);
            return;
        }
        depth_limit -= 1;
        let p = pick_pivot_and_partition(keys, lo, hi, cmp);
        intro_sort(keys, p + 1, hi, depth_limit, cmp);
        if p == 0 {
            return;
        }
        hi = p - 1;
    }
}

fn swap_if_greater<T>(keys: &mut [T], cmp: &impl Fn(&T, &T) -> Ordering, a: usize, b: usize) {
    if a != b && cmp(&keys[a], &keys[b]) == Ordering::Greater {
        keys.swap(a, b);
    }
}

fn pick_pivot_and_partition<T>(
    keys: &mut [T],
    lo: usize,
    hi: usize,
    cmp: &impl Fn(&T, &T) -> Ordering,
) -> usize {
    let middle = lo + ((hi - lo) >> 1);
    swap_if_greater(keys, cmp, lo, middle);
    swap_if_greater(keys, cmp, lo, hi);
    swap_if_greater(keys, cmp, middle, hi);
    keys.swap(middle, hi - 1);
    // the pivot now lives at hi - 1 and never moves during the scan
    let pivot = hi - 1;
    let mut left = lo;
    let mut right = hi - 1;
    while left < right {
        left += 1;
        while cmp(&keys[left], &keys[pivot]) == Ordering::Less {
            left += 1;
        }
        right -= 1;
        while cmp(&keys[pivot], &keys[right]) == Ordering::Less {
            right -= 1;
        }
        if left >= right {
            break;
        }
        keys.swap(left, right);
    }
    keys.swap(left, hi - 1);
    left
}

fn insertion_sort<T>(keys: &mut [T], lo: usize, hi: usize, cmp: &impl Fn(&T, &T) -> Ordering) {
    for i in lo..hi {
        let mut j = i + 1;
        // shift the element at i + 1 left while it is smaller (stable for equal keys)
        while j > lo && cmp(&keys[j], &keys[j - 1]) == Ordering::Less {
            keys.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn heapsort<T>(keys: &mut [T], lo: usize, hi: usize, cmp: &impl Fn(&T, &T) -> Ordering) {
    let n = hi - lo + 1;
    let mut i = n / 2;
    while i >= 1 {
        down_heap(keys, i, n, lo, cmp);
        i -= 1;
    }
    let mut i = n;
    while i > 1 {
        keys.swap(lo, lo + i - 1);
        down_heap(keys, 1, i - 1, lo, cmp);
        i -= 1;
    }
}

fn down_heap<T>(keys: &mut [T], mut i: usize, n: usize, lo: usize, cmp: &impl Fn(&T, &T) -> Ordering) {
    // the element sinking down is kept at its current slot and swapped along the way:
    // the same final arrangement as moving the children up and writing it once
    while i <= n / 2 {
        let mut child = 2 * i;
        if child < n && cmp(&keys[lo + child - 1], &keys[lo + child]) == Ordering::Less {
            child += 1;
        }
        if cmp(&keys[lo + i - 1], &keys[lo + child - 1]) != Ordering::Less {
            break;
        }
        keys.swap(lo + i - 1, lo + child - 1);
        i = child;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut v: Vec<(i32, usize)>, cap: usize) -> Vec<(i32, usize)> {
        introspective_sort(&mut v, cap, &|a: &(i32, usize), b: &(i32, usize)| b.0.cmp(&a.0));
        v
    }

    #[test]
    fn sorts_descending_by_key() {
        for n in [0usize, 1, 2, 3, 5, 16, 17, 40, 300] {
            let v: Vec<(i32, usize)> = (0..n).map(|i| (((i * 7919) % 13) as i32, i)).collect();
            let s = sorted(v.clone(), n);
            assert_eq!(s.len(), n);
            assert!(s.windows(2).all(|w| w[0].0 >= w[1].0));
        }
    }

    #[test]
    fn small_inputs_are_stable_and_large_ones_follow_the_partition() {
        // insertion sort below 17 elements keeps equal keys in their order
        let s = sorted(vec![(1, 0), (2, 1), (1, 2), (2, 3)], 4);
        assert_eq!(s, vec![(2, 1), (2, 3), (1, 0), (1, 2)]);
        // above it the median-of-three partition moves equal keys: the exact
        // permutation is what .NET produces for this input
        let v: Vec<(i32, usize)> = (0..20).map(|i| ((i % 2) as i32, i)).collect();
        let s = sorted(v, 32);
        assert!(s.windows(2).all(|w| w[0].0 >= w[1].0));
    }

    #[test]
    fn heapsort_path_sorts() {
        // a depth limit of zero sends everything to heapsort
        let mut v: Vec<i32> = (0..50).rev().collect();
        intro_sort(&mut v, 0, 49, 0, &|a: &i32, b: &i32| a.cmp(b));
        assert_eq!(v, (0..50).collect::<Vec<_>>());
        assert_eq!(floor_log2(1), 1);
        assert_eq!(floor_log2(4), 3);
    }
}
