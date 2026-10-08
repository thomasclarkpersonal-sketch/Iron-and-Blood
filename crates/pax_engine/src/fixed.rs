//! Deterministic decimal fixed-point arithmetic.
//!
//! Every simulation quantity that feeds back into state (money, prices, goods,
//! rates, militancy, literacy…) is a [`Fixed`]. Floating point is banned from
//! simulation state because IEEE-754 results can differ across compilers, CPU
//! instruction sets (x87 vs SSE vs NEON, FMA contraction) and optimisation
//! levels, which would desync multiplayer lockstep and break save/load replay
//! (see `docs/DECISIONS.md`, D3).
//!
//! # Representation
//!
//! `Fixed(raw)` represents the real number `raw / SCALE` with `SCALE = 10^6`.
//! A decimal scale (rather than binary) is used so that values written in data
//! files such as `0.015` are represented *exactly*, and so that debug output is
//! human-readable. Range: about ±9.2 × 10^12 with 6 decimal places.
//!
//! # Rounding
//!
//! Multiplication and division round toward negative infinity (floor). One
//! rule, applied everywhere, keeps results reproducible; code that splits a
//! total among recipients must use [`crate::alloc`] so that rounding residue is
//! handed out instead of silently destroyed.
//!
//! # Overflow
//!
//! All operations are checked and **panic on overflow**, in release builds too.
//! An overflow is always a simulation bug (e.g. a price escaping its technical
//! bounds); wrapping would silently create or destroy money.

use std::fmt;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

/// Number of raw units per 1.0.
pub const SCALE: i64 = 1_000_000;
const SCALE_I128: i128 = SCALE as i128;
/// Number of decimal digits after the point.
pub const DECIMALS: usize = 6;

/// Decimal fixed-point number with 6 fractional digits. See the module docs.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(i64);

impl Fixed {
    pub const ZERO: Fixed = Fixed(0);
    pub const ONE: Fixed = Fixed(SCALE);
    /// Smallest positive value (10^-6).
    pub const EPSILON: Fixed = Fixed(1);
    pub const MAX: Fixed = Fixed(i64::MAX);

    /// Builds a value from its raw representation (`raw / 10^6`).
    pub const fn from_raw(raw: i64) -> Fixed {
        Fixed(raw)
    }

    /// Raw representation (`self * 10^6`).
    pub const fn raw(self) -> i64 {
        self.0
    }

    /// Exact conversion from an integer. Panics if out of range.
    pub fn from_int(v: i64) -> Fixed {
        Fixed(v.checked_mul(SCALE).expect("Fixed::from_int overflow"))
    }

    /// Exact `numerator / denominator` rounded down, for constants in code and tests.
    pub fn ratio(numerator: i64, denominator: i64) -> Fixed {
        assert!(denominator > 0, "Fixed::ratio denominator must be positive");
        narrow((numerator as i128 * SCALE_I128).div_euclid(denominator as i128))
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// `self × rhs`, rounded down.
    ///
    /// Deliberately a named method rather than `std::ops::Mul`: every
    /// multiplication rounds, and call sites should show it.
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, rhs: Fixed) -> Fixed {
        // Fast path (M4-11): when the product fits i64, 64-bit division by the
        // constant scale gives the same floor as the i128 path, without i128
        // division's library call. `mul_div`, `mul_ceil` and `div` do the same.
        match self.0.checked_mul(rhs.0) {
            Some(p) => Fixed(p.div_euclid(SCALE)),
            None => narrow((self.0 as i128 * rhs.0 as i128).div_euclid(SCALE_I128)),
        }
    }

    /// `self × rhs`, rounded up. Used where under-counting would let an agent
    /// consume more than it holds (e.g. input goods consumed by production).
    pub fn mul_ceil(self, rhs: Fixed) -> Fixed {
        if let Some(neg) = self.0.checked_mul(rhs.0).and_then(i64::checked_neg) {
            return Fixed(-neg.div_euclid(SCALE));
        }
        let p = self.0 as i128 * rhs.0 as i128;
        narrow(-((-p).div_euclid(SCALE_I128)))
    }

    /// `self × n` for an integer `n` (exact).
    pub fn mul_int(self, n: i64) -> Fixed {
        Fixed(self.0.checked_mul(n).expect("Fixed::mul_int overflow"))
    }

    /// `self / rhs`, rounded down. Panics if `rhs` is zero.
    #[allow(clippy::should_implement_trait)]
    pub fn div(self, rhs: Fixed) -> Fixed {
        assert!(rhs.0 != 0, "Fixed::div by zero");
        if let Some(n) = self.0.checked_mul(SCALE)
            && let Some(q) = n.checked_div_euclid(rhs.0)
        {
            return Fixed(if rhs.0 < 0 && n.rem_euclid(rhs.0) != 0 { q - 1 } else { q });
        }
        let n = self.0 as i128 * SCALE_I128;
        let d = rhs.0 as i128;
        // Floor division for either sign of the divisor.
        let q = n.div_euclid(d);
        let q = if d < 0 && n.rem_euclid(d) != 0 { q - 1 } else { q };
        narrow(q)
    }

    /// `self / n` for a positive integer `n`, rounded down.
    pub fn div_int(self, n: i64) -> Fixed {
        assert!(n > 0, "Fixed::div_int divisor must be positive");
        Fixed(self.0.div_euclid(n))
    }

    /// `self × num / den` with a single rounding (down) and an i128
    /// intermediate, so it cannot overflow unless the result itself does.
    pub fn mul_div(self, num: Fixed, den: Fixed) -> Fixed {
        assert!(den.0 > 0, "Fixed::mul_div denominator must be positive");
        match self.0.checked_mul(num.0) {
            Some(p) => Fixed(p.div_euclid(den.0)),
            None => narrow((self.0 as i128 * num.0 as i128).div_euclid(den.0 as i128)),
        }
    }

    pub fn min(self, other: Fixed) -> Fixed {
        if self <= other { self } else { other }
    }

    pub fn max(self, other: Fixed) -> Fixed {
        if self >= other { self } else { other }
    }

    pub fn clamp(self, lo: Fixed, hi: Fixed) -> Fixed {
        debug_assert!(lo <= hi);
        self.max(lo).min(hi)
    }

    pub fn abs(self) -> Fixed {
        Fixed(self.0.checked_abs().expect("Fixed::abs overflow"))
    }

    /// Integer part, rounded down.
    pub fn floor_int(self) -> i64 {
        self.0.div_euclid(SCALE)
    }

    /// Parses an exact decimal such as `"12"`, `"-0.015"` or `"3.250000"`.
    ///
    /// More than 6 fractional digits is an error rather than a silent rounding,
    /// so data files can never introduce hidden precision loss.
    pub fn parse_decimal(s: &str) -> Result<Fixed, String> {
        let s = s.trim();
        let (neg, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int_part, frac_part) = match body.split_once('.') {
            Some((i, f)) => (i, f),
            None => (body, ""),
        };
        let digits_ok = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
        if (int_part.is_empty() && frac_part.is_empty()) || !digits_ok(int_part) || !digits_ok(frac_part) {
            return Err(format!("'{s}' is not a decimal number"));
        }
        if frac_part.len() > DECIMALS {
            return Err(format!("'{s}' has more than {DECIMALS} decimal places"));
        }
        let int_val: i128 =
            if int_part.is_empty() { 0 } else { int_part.parse().map_err(|_| format!("'{s}' is out of range"))? };
        let mut frac_val: i128 = if frac_part.is_empty() { 0 } else { frac_part.parse().unwrap() };
        for _ in frac_part.len()..DECIMALS {
            frac_val *= 10;
        }
        let raw = int_val.checked_mul(SCALE_I128).ok_or_else(|| format!("'{s}' is out of range"))? + frac_val;
        let raw = if neg { -raw } else { raw };
        i64::try_from(raw).map(Fixed).map_err(|_| format!("'{s}' is out of range"))
    }
}

/// Converts an i128 intermediate back to `Fixed`, panicking on overflow.
fn narrow(v: i128) -> Fixed {
    Fixed(i64::try_from(v).expect("Fixed arithmetic overflow"))
}

impl Add for Fixed {
    type Output = Fixed;
    fn add(self, rhs: Fixed) -> Fixed {
        Fixed(self.0.checked_add(rhs.0).expect("Fixed add overflow"))
    }
}

impl Sub for Fixed {
    type Output = Fixed;
    fn sub(self, rhs: Fixed) -> Fixed {
        Fixed(self.0.checked_sub(rhs.0).expect("Fixed sub overflow"))
    }
}

impl AddAssign for Fixed {
    fn add_assign(&mut self, rhs: Fixed) {
        *self = *self + rhs;
    }
}

impl SubAssign for Fixed {
    fn sub_assign(&mut self, rhs: Fixed) {
        *self = *self - rhs;
    }
}

impl Neg for Fixed {
    type Output = Fixed;
    fn neg(self) -> Fixed {
        Fixed(self.0.checked_neg().expect("Fixed neg overflow"))
    }
}

impl std::iter::Sum for Fixed {
    fn sum<I: Iterator<Item = Fixed>>(iter: I) -> Fixed {
        iter.fold(Fixed::ZERO, |a, b| a + b)
    }
}

impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = (self.0 as i128).abs();
        write!(f, "{sign}{}.{:06}", abs / SCALE_I128, abs % SCALE_I128)
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    /// M4-11: the i64 fast paths give exactly the i128 results, so they change no
    /// simulation result (D3, D11). The references are the original i128 formulas.
    #[test]
    fn the_fast_paths_match_the_i128_formulas() {
        fn reference_mul(a: i64, b: i64) -> Option<i64> {
            i64::try_from((a as i128 * b as i128).div_euclid(SCALE_I128)).ok()
        }
        fn reference_mul_ceil(a: i64, b: i64) -> Option<i64> {
            i64::try_from(-((-(a as i128 * b as i128)).div_euclid(SCALE_I128))).ok()
        }
        fn reference_div(a: i64, b: i64) -> Option<i64> {
            let (n, d) = (a as i128 * SCALE_I128, b as i128);
            let q = n.div_euclid(d);
            i64::try_from(if d < 0 && n.rem_euclid(d) != 0 { q - 1 } else { q }).ok()
        }
        fn reference_mul_div(a: i64, b: i64, c: i64) -> Option<i64> {
            i64::try_from((a as i128 * b as i128).div_euclid(c as i128)).ok()
        }
        // Magnitudes from tiny to i64's edge, both signs, so products land on both
        // sides of the i64 boundary.
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            let z = z ^ (z >> 31);
            let bits = z % 64;
            let v = (z >> 1) as i64 >> (63 - bits.max(1));
            if z & 1 == 0 { v } else { v.wrapping_neg() }
        };
        let edges = [0, 1, -1, 7, -7, SCALE, -SCALE, SCALE - 1, i64::MAX, i64::MIN, i64::MIN + 1, 3_037_000_499];
        let mut values: Vec<i64> = edges.to_vec();
        values.extend((0..2_000).map(|_| next()));
        // Where the reference overflows, both paths panic: the fast path falls back to
        // the same i128 formula. Only results that exist are compared.
        let same = |fast: &dyn Fn() -> Fixed, reference: Option<i64>, what: &str| {
            if let Some(expected) = reference {
                assert_eq!(fast().raw(), expected, "{what}");
            }
        };
        let mut checked = 0;
        for (k, &a) in values.iter().enumerate() {
            for &b in values.iter().skip(k % 7).step_by(7) {
                let (x, y) = (Fixed::from_raw(a), Fixed::from_raw(b));
                same(&|| x.mul(y), reference_mul(a, b), &format!("mul {a} {b}"));
                same(&|| x.mul_ceil(y), reference_mul_ceil(a, b), &format!("mul_ceil {a} {b}"));
                if b != 0 {
                    same(&|| x.div(y), reference_div(a, b), &format!("div {a} {b}"));
                }
                let c = b.unsigned_abs().clamp(1, i64::MAX as u64) as i64;
                same(&|| x.mul_div(y, Fixed::from_raw(c)), reference_mul_div(a, b, c), &format!("mul_div {a} {b} {c}"));
                checked += 1;
            }
        }
        assert!(checked > 500_000, "{checked}");
    }

    use super::*;

    fn d(s: &str) -> Fixed {
        Fixed::parse_decimal(s).unwrap()
    }

    #[test]
    fn parse_and_display_round_trip() {
        for s in ["0.000000", "1.000000", "-0.015000", "123456.789012", "-7.000001"] {
            assert_eq!(d(s).to_string(), s);
        }
        assert_eq!(d("0.1").raw(), 100_000);
        assert_eq!(d(".5").raw(), 500_000);
        assert_eq!(d("+2").raw(), 2_000_000);
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(Fixed::parse_decimal("1.0000001").is_err());
        assert!(Fixed::parse_decimal("abc").is_err());
        assert!(Fixed::parse_decimal("1e5").is_err());
        assert!(Fixed::parse_decimal("").is_err());
        assert!(Fixed::parse_decimal("99999999999999999").is_err());
        // 34 integer digits fit i128 but overflow once scaled by 10^6 (audit 2026-10-08).
        assert!(Fixed::parse_decimal("1000000000000000000000000000000000").is_err());
        assert!(Fixed::parse_decimal("-99999999999999999999999999999999999999").is_err());
    }

    #[test]
    fn multiplication_floors() {
        assert_eq!(d("0.1").mul(d("0.1")), d("0.01"));
        // 1/3 * 3 loses a raw unit to flooring; that is the documented rule.
        let third = Fixed::ratio(1, 3);
        assert_eq!(third, d("0.333333"));
        assert_eq!(third.mul(Fixed::from_int(3)), d("0.999999"));
        // Floor, not truncation, for negatives.
        assert_eq!(Fixed::from_raw(-1).mul(d("0.5")), Fixed::from_raw(-1));
    }

    #[test]
    fn mul_ceil_rounds_up() {
        assert_eq!(Fixed::from_raw(1).mul_ceil(d("0.5")), Fixed::from_raw(1));
        assert_eq!(d("2").mul_ceil(d("0.5")), d("1"));
    }

    #[test]
    fn division_floors_for_both_signs() {
        assert_eq!(Fixed::ONE.div(Fixed::from_int(3)), d("0.333333"));
        assert_eq!((-Fixed::ONE).div(Fixed::from_int(3)), d("-0.333334"));
        assert_eq!(Fixed::ONE.div(Fixed::from_int(-3)), d("-0.333334"));
        assert_eq!((-Fixed::ONE).div(Fixed::from_int(-3)), d("0.333333"));
    }

    #[test]
    fn mul_div_uses_wide_intermediate() {
        let big = Fixed::from_int(1_000_000_000);
        assert_eq!(big.mul_div(big, big), big);
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn overflow_panics() {
        let _ = Fixed::MAX + Fixed::EPSILON;
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn mul_overflow_panics() {
        let big = Fixed::from_int(10_000_000_000);
        let _ = big.mul(big);
    }
}
