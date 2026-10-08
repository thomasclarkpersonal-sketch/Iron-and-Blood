//! Province maps (M3-7, D12) in a scenario: `pax_map` reads and validates the files
//! (the same reader the client draws with), and the scenario keeps what the server
//! needs. That is everything but the per-pixel province ids: the server never draws.
//! The two files also go into the scenario's content hash.

use std::path::Path;

use pax_content::ContentHash;
use pax_engine::World;
use pax_map::{PNG_FILE, ProvinceMap, TOML_FILE};

use crate::{LoadError, read};

/// A validated province map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapData {
    pub width: u32,
    pub height: u32,
    /// Each province's colour, in scenario order (index = province id).
    pub colors: Vec<[u8; 3]>,
    /// Each province's label anchor, in scenario order.
    pub labels: Vec<[u32; 2]>,
    pub background: Option<[u8; 3]>,
    /// `pax_content::map_hash` of the two map files: `StaticData.map_hash` (D22). The
    /// client computes the same function over its own copies.
    pub map_hash: u64,
}

/// The map's files, for the scenario's content hash.
pub(crate) struct MapFiles {
    pub toml: String,
    pub png: Vec<u8>,
}

impl MapFiles {
    /// Adds the map files to the scenario's content hash, with `pax_content`'s roles.
    pub(crate) fn hash_into(&self, h: &mut ContentHash) {
        pax_content::add_map_files(h, self.toml.as_bytes(), &self.png);
    }
}

/// Loads and validates the map in `dir` against `world`'s provinces.
pub(crate) fn load(dir: &Path, world: &World) -> Result<(MapData, MapFiles), LoadError> {
    let toml = read(&dir.join(TOML_FILE))?;
    let png_path = dir.join(PNG_FILE);
    let png = std::fs::read(&png_path).map_err(|e| LoadError::single(format!("{}: {e}", png_path.display())))?;
    let map =
        ProvinceMap::read(&toml, &png, &world.geography.province_keys).map_err(|messages| LoadError { messages })?;
    let ProvinceMap { width, height, colors, labels, background, pixels: _, map_hash } = map;
    Ok((MapData { width, height, colors, labels, background, map_hash }, MapFiles { toml, png }))
}
