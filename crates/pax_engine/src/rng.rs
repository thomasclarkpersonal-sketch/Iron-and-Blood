//! Counter-based deterministic random numbers.
//!
//! Systems run in parallel and in any thread interleaving, so a shared,
//! stateful RNG would make results depend on scheduling. Instead every random
//! draw is a pure function of `(world seed, stream, day, entity, draw index)`:
//! the same inputs always give the same number, regardless of thread count or
//! iteration order (DECISIONS.md D3).
//!
//! The mixer is SplitMix64 (Steele, Lea & Flood 2014), applied to each key in
//! turn. It passes BigCrush when used this way and costs a few multiplies.

use crate::fixed::{Fixed, SCALE};

/// Identifies which system is drawing, so two systems never reuse a stream.
/// Add a new constant for every new consumer of randomness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stream(pub u64);

impl Stream {
    pub const PROMOTION: Stream = Stream(1);
    pub const MIGRATION: Stream = Stream(2);
    pub const REBELLION: Stream = Stream(3);
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A uniformly distributed `u64` for the given key.
pub fn draw_u64(seed: u64, stream: Stream, day: u64, entity: u64, index: u64) -> u64 {
    let mut h = splitmix64(seed);
    for k in [stream.0, day, entity, index] {
        h = splitmix64(h ^ k);
    }
    h
}

/// A uniformly distributed value in `[0, 1)` with 10^-6 resolution.
pub fn draw_unit(seed: u64, stream: Stream, day: u64, entity: u64, index: u64) -> Fixed {
    // Multiply-shift maps u64 uniformly onto [0, SCALE) without modulo bias.
    let r = draw_u64(seed, stream, day, entity, index);
    Fixed::from_raw(((r as u128 * SCALE as u128) >> 64) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_function_of_key() {
        let a = draw_u64(42, Stream::PROMOTION, 10, 7, 0);
        assert_eq!(a, draw_u64(42, Stream::PROMOTION, 10, 7, 0));
        assert_ne!(a, draw_u64(42, Stream::PROMOTION, 10, 7, 1));
        assert_ne!(a, draw_u64(42, Stream::MIGRATION, 10, 7, 0));
        assert_ne!(a, draw_u64(43, Stream::PROMOTION, 10, 7, 0));
    }

    #[test]
    fn unit_draws_in_range_and_roughly_uniform() {
        let n = 100_000u64;
        let mut sum: i64 = 0;
        for i in 0..n {
            let u = draw_unit(1, Stream::REBELLION, 0, i, 0);
            assert!(u >= Fixed::ZERO && u < Fixed::ONE);
            sum += u.raw();
        }
        let mean = sum / n as i64;
        assert!((mean - SCALE / 2).abs() < SCALE / 100, "mean {mean}");
    }

    #[test]
    fn golden_values_are_stable() {
        // Pinned outputs. If these change, every recorded replay hash changes
        // too, so treat a failure here as a breaking change.
        assert_eq!(draw_u64(0, Stream(0), 0, 0, 0), 0x78AE_5A9A_6B5F_D45E);
        assert_eq!(draw_u64(7, Stream(1), 100, 42, 3), 0x1A5D_5669_0D14_8051);
    }
}
