//! Province maps (M3-7, D12) in a scenario: `pax_map` reads and validates the files
//! (the same reader the client draws with), and the scenario keeps what the server
//! needs. That is everything but the per-pixel province ids: the server never draws.
//! The two files also go into the scenario's content hash.

use std::path::Path;

use pax_content::ContentHash;
use pax_engine::World;
use pax_map::{PNG_FILE, ProvinceMap, TOML_FILE};

use crate::{LoadError, read};

/// A scenario's validated province map: where it is, and what `pax_map` read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapData {
    /// The map's directory, relative to the scenario's (`scenario.toml`'s `map`): sent
    /// to the client as `StaticData.map_dir`.
    pub dir: String,
    /// The map: sizes, colours and labels in scenario order, and `map_hash`
    /// (`StaticData.map_hash`, D22), which the client computes over its own copies.
    pub map: ProvinceMap,
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

/// Loads and validates the map in `dir` (the scenario's `map` setting, relative to
/// `scenario_dir`) against `world`'s provinces.
pub(crate) fn load(scenario_dir: &Path, dir: &str, world: &World) -> Result<(MapData, MapFiles), LoadError> {
    let path =
        pax_map::resolve_map_dir(scenario_dir, dir).map_err(|e| LoadError::single(format!("scenario.toml: {e}")))?;
    let toml = read(&path.join(TOML_FILE))?;
    let png_path = path.join(PNG_FILE);
    let png = std::fs::read(&png_path).map_err(|e| LoadError::single(format!("{}: {e}", png_path.display())))?;
    // The server never draws, so it reads without the per-pixel ids.
    let map = ProvinceMap::read(&toml, &png, &world.geography.province_keys).map_err(|messages| LoadError {
        messages: messages.into_iter().map(|m| format!("{}: {m}", path.display())).collect(),
    })?;
    Ok((MapData { dir: dir.to_owned(), map }, MapFiles { toml, png }))
}
