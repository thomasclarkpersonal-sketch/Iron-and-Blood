//! Policy rates between the wire and the UI. A rate travels as a raw `Fixed`
//! (value × `pax_protocol::FIXED_ONE`, D3). The UI shows it as a fraction, and its
//! sliders step in tenths of a percent (per mille). Every conversion is here, with
//! tests, so GDScript never does `Fixed` arithmetic.

use pax_protocol::FIXED_ONE;

/// Raw units in one per mille (a tenth of a percent).
pub const PER_MILLE: i64 = FIXED_ONE / 1000;

/// A raw rate as a fraction (0.15 for 15%), for display only.
pub fn fraction(raw: i64) -> f64 {
    crate::decode::display(pax_protocol::wire::Fixed::new(raw))
}

/// A slider's per-mille value as the raw rate a command carries: exact integers.
pub fn from_per_mille(per_mille: i64) -> Result<i64, String> {
    per_mille.checked_mul(PER_MILLE).ok_or_else(|| format!("{per_mille} per mille is out of range"))
}

/// A raw rate in whole per mille, rounded down: where a slider shows it.
pub fn per_mille(raw: i64) -> i64 {
    raw.div_euclid(PER_MILLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_steps_round_trip_exactly() {
        for step in [0, 1, 150, 999, 1000] {
            assert_eq!(per_mille(from_per_mille(step).unwrap()), step);
        }
        assert_eq!(from_per_mille(150), Ok(150_000));
        assert_eq!(per_mille(123_456), 123, "a finer rate shows at the step below");
        assert_eq!(fraction(150_000), 0.15);
        assert!(from_per_mille(i64::MAX).is_err());
    }
}
