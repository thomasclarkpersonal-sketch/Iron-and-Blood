//! xorshift64*: deterministic noise for the hostile-input tests (M3-10), without a
//! dependency. One source, compiled into both test layers: the crate's unit tests
//! (`#[cfg(test)] mod noise`) and the integration tests (`tests/common` includes this
//! file with `#[path]`). Test code depends on the source tree, not the reverse.

pub struct Noise(u64);

impl Noise {
    /// A generator from `seed`, which must not be 0: xorshift never leaves 0.
    pub fn new(seed: u64) -> Noise {
        assert_ne!(seed, 0, "xorshift never leaves a zero seed");
        Noise(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n`.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// An index into something of `len` items.
    pub fn index(&mut self, len: usize) -> usize {
        self.below(len as u64) as usize
    }
}
