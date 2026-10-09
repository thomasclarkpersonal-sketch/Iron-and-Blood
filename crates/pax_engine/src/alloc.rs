//! Exact pro-rata division of an integer total.
//!
//! Stock-flow consistency (DECISIONS.md D5) requires that whenever an amount is
//! split among recipients (wages among workers, revenue among sellers, cash of
//! a splitting POP…) the parts sum to *exactly* the whole. Naively flooring
//! each share `total × wᵢ / W` leaks up to `n − 1` raw units per split, which
//! over millions of ticks is a visible money sink.
//!
//! This module implements the **largest-remainder (Hamilton) method**:
//!
//! 1. `shareᵢ = ⌊total × wᵢ / W⌋`, `remᵢ = (total × wᵢ) mod W`
//! 2. the leftover `total − Σ shareᵢ` (always `< n`) is handed out one raw unit
//!    at a time to the largest `remᵢ`, ties broken by lower index.
//!
//! The tie-break makes the result a pure function of its inputs, which keeps it
//! deterministic. Finding the `leftover` largest remainders uses selection
//! (`select_nth_unstable_by`, O(n)) rather than a full sort. Because the
//! comparator is a strict total order (remainder, then index), the selected set
//! is unique, so the result is identical to sorting.

use crate::fixed::Fixed;

/// Splits `total` raw units pro rata to `weights` with the largest-remainder
/// method. The result always sums to exactly `total`.
///
/// Returns `None` when `total > 0` but every weight is zero (there is nobody to
/// receive the amount); callers must then keep the amount with its source.
///
/// # Panics
/// If `total` or any weight is negative.
pub fn allocate_raw(total: i64, weights: &[i64]) -> Option<Vec<i64>> {
    assert!(total >= 0, "allocate: negative total");
    assert!(weights.iter().all(|&w| w >= 0), "allocate: negative weight");
    let weight_sum: i128 = weights.iter().map(|&w| w as i128).sum();
    if weight_sum == 0 {
        return if total == 0 { Some(vec![0; weights.len()]) } else { None };
    }
    let mut shares = Vec::with_capacity(weights.len());
    let mut remainders = Vec::with_capacity(weights.len());
    let mut assigned: i128 = 0;
    // Fast path (M4-11): when every `total × w` and the weight sum fit i64, 64-bit
    // division gives exactly the i128 shares and remainders, without i128 division's
    // library call. Wages, dividends and transfers split over many rows here.
    let max_weight = weights.iter().copied().max().unwrap_or(0);
    if let (Ok(sum), Some(_)) = (i64::try_from(weight_sum), total.checked_mul(max_weight)) {
        for (i, &w) in weights.iter().enumerate() {
            let product = total * w;
            let share = product / sum;
            assigned += share as i128;
            shares.push(share);
            remainders.push(((product % sum) as i128, i));
        }
    } else {
        let total = total as i128;
        for (i, &w) in weights.iter().enumerate() {
            let product = total * w as i128;
            let share = product / weight_sum;
            assigned += share;
            shares.push(share as i64);
            remainders.push((product % weight_sum, i));
        }
    }
    let total = total as i128;
    let leftover = (total - assigned) as usize;
    if leftover > 0 {
        // Largest remainder first; equal remainders go to the lower index.
        let order = |a: &(i128, usize), b: &(i128, usize)| b.0.cmp(&a.0).then(a.1.cmp(&b.1));
        if leftover < remainders.len() {
            remainders.select_nth_unstable_by(leftover - 1, order);
        }
        for &(_, i) in &remainders[..leftover] {
            shares[i] += 1;
        }
    }
    Some(shares)
}

/// [`allocate_raw`] for [`Fixed`] totals and weights.
pub fn allocate(total: Fixed, weights: &[Fixed]) -> Option<Vec<Fixed>> {
    let raw: Vec<i64> = weights.iter().map(|w| w.raw()).collect();
    allocate_raw(total.raw(), &raw).map(|v| v.into_iter().map(Fixed::from_raw).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_exactly() {
        let parts = allocate_raw(100, &[1, 1, 1]).unwrap();
        assert_eq!(parts, vec![34, 33, 33]);
        assert_eq!(parts.iter().sum::<i64>(), 100);
    }

    #[test]
    fn largest_remainder_wins() {
        // Exact shares: 10 * [0.15, 0.25, 0.6] = [1.5, 2.5, 6.0] -> floors [1,2,6],
        // one unit left, remainders tie between 0 and 1 -> lower index wins.
        assert_eq!(allocate_raw(10, &[15, 25, 60]).unwrap(), vec![2, 2, 6]);
    }

    #[test]
    fn zero_weights() {
        assert_eq!(allocate_raw(0, &[0, 0]).unwrap(), vec![0, 0]);
        assert!(allocate_raw(5, &[0, 0]).is_none());
        assert!(allocate_raw(5, &[]).is_none());
        assert_eq!(allocate_raw(5, &[0, 3]).unwrap(), vec![0, 5]);
    }

    /// Reference implementation: the original full sort.
    fn allocate_by_sorting(total: i64, weights: &[i64]) -> Vec<i64> {
        let w: i128 = weights.iter().map(|&w| w as i128).sum();
        let mut shares: Vec<i64> = weights.iter().map(|&x| (total as i128 * x as i128 / w) as i64).collect();
        let mut rem: Vec<(i128, usize)> =
            weights.iter().enumerate().map(|(i, &x)| ((total as i128 * x as i128) % w, i)).collect();
        rem.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let leftover = (total - shares.iter().sum::<i64>()) as usize;
        for &(_, i) in rem.iter().take(leftover) {
            shares[i] += 1;
        }
        shares
    }

    #[test]
    fn selection_matches_full_sort() {
        let mut s: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for _ in 0..3_000 {
            let n = (next() % 200 + 1) as usize;
            // Small weight range forces many equal remainders, exercising the tie-break.
            let weights: Vec<i64> = (0..n).map(|_| (next() % 7) as i64).collect();
            if weights.iter().all(|&w| w == 0) {
                continue;
            }
            let total = (next() % 100_000) as i64;
            assert_eq!(allocate_raw(total, &weights).unwrap(), allocate_by_sorting(total, &weights));
        }
    }

    /// M4-11: both of `allocate_raw`'s paths (i64 when every `total × w` fits, i128
    /// otherwise) give exactly the i128 reference's shares, with totals and weights
    /// that land on both sides of the i64 boundary.
    #[test]
    fn both_paths_match_the_i128_reference() {
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let (mut fast, mut wide) = (0, 0);
        for _ in 0..3_000 {
            let n = (next() % 50 + 1) as usize;
            // Magnitudes from tiny to near i64::MAX for both total and weights.
            let magnitude = |v: u64, bits: u64| (v >> 1) as i64 >> (62 - bits % 62);
            let weights: Vec<i64> = (0..n).map(|_| magnitude(next(), next())).collect();
            let total = magnitude(next(), next());
            if weights.iter().all(|&w| w == 0) {
                continue;
            }
            let max = weights.iter().copied().max().unwrap_or(0);
            let sum: i128 = weights.iter().map(|&w| w as i128).sum();
            if total.checked_mul(max).is_some() && i64::try_from(sum).is_ok() {
                fast += 1;
            } else {
                wide += 1;
            }
            assert_eq!(
                allocate_raw(total, &weights).unwrap(),
                allocate_by_sorting(total, &weights),
                "{total} {weights:?}"
            );
        }
        assert!(fast > 200 && wide > 200, "both paths exercised: {fast} fast, {wide} wide");
    }

    #[test]
    fn conservation_over_many_random_splits() {
        // Cheap deterministic pseudo-random sweep (no external crates).
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for _ in 0..2_000 {
            let n = (next() % 50 + 1) as usize;
            let weights: Vec<i64> = (0..n).map(|_| (next() % 1_000_000) as i64).collect();
            let total = (next() % 10_000_000_000) as i64;
            if let Some(parts) = allocate_raw(total, &weights) {
                assert_eq!(parts.iter().sum::<i64>(), total);
                for (p, w) in parts.iter().zip(&weights) {
                    if *w == 0 {
                        assert_eq!(*p, 0);
                    }
                }
            }
        }
    }
}
