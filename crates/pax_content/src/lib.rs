//! `pax_content`: content hashing shared by the server and the client (D22). FNV-1a
//! (64-bit) over files, each keyed by its role (not its path) and length-prefixed.
//!
//! It has no dependencies. `pax_data` (the engine side) and the client bridge (the
//! wire side) both link it, so both compute hashes with exactly one scheme, and
//! neither side's types leak into the other (AGENTS.md §2).
//! * `Welcome.content_hash` covers every file the scenario loader reads.
//! * `StaticData.map_hash` covers just the map files the client draws. It is
//!   [`map_hash`], whose file roles are defined here, once.
//!
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

/// Role of a scenario map's province table (`map/provinces.toml`, M3-7).
pub const MAP_TOML_ROLE: &str = "map/provinces.toml";
/// Role of a scenario map's province image (`map/provinces.png`, M3-7).
pub const MAP_PNG_ROLE: &str = "map/provinces.png";

/// Adds a scenario map's two files to `hash`, with their roles, in this order.
pub fn add_map_files(hash: &mut ContentHash, toml: &[u8], png: &[u8]) {
    hash.file(MAP_TOML_ROLE, toml);
    hash.file(MAP_PNG_ROLE, png);
}

/// `StaticData.map_hash`: the hash of a scenario map's two files. The server sends
/// it, and the client computes it over its own copies with this same function.
pub fn map_hash(toml: &[u8], png: &[u8]) -> u64 {
    let mut hash = ContentHash::default();
    add_map_files(&mut hash, toml, png);
    hash.finish()
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

    /// Pins the map-hash scheme (roles, order, length prefixes). If this changes,
    /// clients built before the change refuse every map: bump the protocol.
    #[test]
    fn map_hash_is_pinned() {
        assert_eq!(map_hash(b"toml", b"png"), 0x0f75_714a_66db_78ed);
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
