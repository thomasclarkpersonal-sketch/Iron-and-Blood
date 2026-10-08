//! Content hashing shared by the server and the client (D22): FNV-1a (64-bit) over
//! files, each keyed by its role (not its path) and length-prefixed.
//!
//! It lives here, in the engine-free protocol crate, so the client computes it with
//! exactly the server's scheme. `Welcome.content_hash` (every file the scenario loader
//! reads) and `StaticData.map_hash` (just the map files the client draws) both use it.
//! It is deliberately separate from the engine's state hash, so changing one never
//! silently changes the other.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// An FNV-1a hash of role-keyed files.
#[derive(Clone, Debug)]
pub struct ContentHash(u64);

impl Default for ContentHash {
    fn default() -> Self {
        ContentHash(FNV_OFFSET)
    }
}

impl ContentHash {
    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    /// Adds one file. Role and content are each length-prefixed, so ("ab", "c") and
    /// ("a", "bc") hash differently.
    pub fn file(&mut self, role: &str, content: &[u8]) {
        for part in [role.as_bytes(), content] {
            self.bytes(&(part.len() as u64).to_le_bytes());
            self.bytes(part);
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
    fn matches_the_published_fnv1a_test_vector() {
        // FNV-1a 64 of "a" is 0xaf63dc4c8601ec8c (Fowler/Noll/Vo reference vectors).
        let mut h = ContentHash::default();
        h.bytes(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn roles_and_boundaries_matter() {
        let hash = |files: &[(&str, &[u8])]| {
            let mut h = ContentHash::default();
            files.iter().for_each(|(role, content)| h.file(role, content));
            h.finish()
        };
        assert_ne!(hash(&[("x", b"ab"), ("y", b"c")]), hash(&[("x", b"a"), ("y", b"bc")]));
        assert_ne!(hash(&[("x", b"a")]), hash(&[("y", b"a")]));
        assert_eq!(hash(&[("x", b"a")]), hash(&[("x", b"a")]));
    }
}
