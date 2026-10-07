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
//! deterministic.

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
    let total = total as i128;
    let mut shares = Vec::with_capacity(weights.len());
    let mut remainders = Vec::with_capacity(weights.len());
    let mut assigned: i128 = 0;
    for (i, &w) in weights.iter().enumerate() {
        let product = total * w as i128;
        let share = product / weight_sum;
        assigned += share;
        shares.push(share as i64);
        remainders.push((product % weight_sum, i));
    }
    let leftover = (total - assigned) as usize;
    if leftover > 0 {
        // Largest remainder first; equal remainders go to the lower index.
        remainders.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for &(_, i) in remainders.iter().take(leftover) {
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
