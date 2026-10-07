//! Stable state hashing for determinism checks.
//!
//! `std::collections::hash_map::DefaultHasher` is explicitly *not* guaranteed
//! to be stable across Rust releases, so replay hashes would change on a
//! compiler upgrade. FNV-1a (64-bit) is tiny, fully specified, and fast enough
//! for hashing a few hundred MB of columns per check.

use crate::fixed::Fixed;

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// Incremental FNV-1a hasher over little-endian encodings.
#[derive(Clone, Debug)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> Self {
        StateHasher(FNV_OFFSET)
    }
}

impl StateHasher {
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u32s(&mut self, vs: &[u32]) {
        self.u64(vs.len() as u64);
        for v in vs {
            self.bytes(&v.to_le_bytes());
        }
    }

    pub fn u16s(&mut self, vs: &[u16]) {
        self.u64(vs.len() as u64);
        for v in vs {
            self.bytes(&v.to_le_bytes());
        }
    }

    pub fn fixeds(&mut self, vs: &[Fixed]) {
        self.u64(vs.len() as u64);
        for v in vs {
            self.bytes(&v.raw().to_le_bytes());
        }
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_fnv1a() {
        // Published FNV-1a 64 test vectors.
        let mut h = StateHasher::default();
        assert_eq!(h.finish(), 0xCBF2_9CE4_8422_2325);
        h.bytes(b"a");
        assert_eq!(h.finish(), 0xAF63_DC4C_8601_EC8C);
        let mut h = StateHasher::default();
        h.bytes(b"foobar");
        assert_eq!(h.finish(), 0x8594_4171_F739_67E8);
    }

    #[test]
    fn length_prefix_distinguishes_column_boundaries() {
        let mut a = StateHasher::default();
        a.u32s(&[1, 2]);
        a.u32s(&[3]);
        let mut b = StateHasher::default();
        b.u32s(&[1]);
        b.u32s(&[2, 3]);
        assert_ne!(a.finish(), b.finish());
    }
}
