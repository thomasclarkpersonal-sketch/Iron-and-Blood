//! The client's province map (M3-8b, D12): the scenario's map files, read with the
//! same reader the server validates them with (`pax_map`), checked against the
//! session's `StaticData.map_hash`, and turned into the province-ID texture the map
//! shader draws.
//!
//! **ID texture:** RGB8, one texel per map pixel. `r + 256·g` is the province index
//! plus one, and 0 is background (sea). So a map can have at most [`MAX_PROVINCES`]
//! provinces. A map-mode change then rewrites only the small per-province colour
//! table, never this image.

use std::path::Path;

use pax_map::ProvinceMap;

/// The most provinces the 16-bit ID texture can address (0 is background).
pub const MAX_PROVINCES: usize = 65_535;

/// The map, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapImage {
    pub width: u32,
    pub height: u32,
    /// RGB8 province-ID texels, row by row (see the module docs).
    pub ids: Vec<u8>,
    /// Each province's label anchor, `[x, y]` in pixels.
    pub labels: Vec<[u32; 2]>,
}

/// Loads the map of the scenario in `scenario_dir` for a session whose provinces are
/// `provinces` (Welcome order) and whose server's map hashes to `expected_hash`.
/// `Ok(None)` when the scenario has no map.
pub fn load(scenario_dir: &Path, provinces: &[String], expected_hash: Option<u64>) -> Result<Option<MapImage>, String> {
    let Some(dir) = pax_map::scenario_map_dir(scenario_dir)? else {
        return match expected_hash {
            None => Ok(None),
            Some(_) => Err("the server has a map for this scenario, but this copy of it has none".to_owned()),
        };
    };
    let map = ProvinceMap::load(&dir, provinces).map_err(|errors| errors.join("\n"))?;
    if expected_hash != Some(map.map_hash) {
        return Err(format!(
            "this copy of the map ({}) differs from the server's: map hash {:#018x}, the server's is {}",
            dir.display(),
            map.map_hash,
            expected_hash.map_or("none".to_owned(), |h| format!("{h:#018x}"))
        ));
    }
    image(&map).map(Some)
}

/// The ID texture of a validated map.
pub fn image(map: &ProvinceMap) -> Result<MapImage, String> {
    if map.colors.len() > MAX_PROVINCES {
        return Err(format!("the map has {} provinces; the client draws at most {MAX_PROVINCES}", map.colors.len()));
    }
    let mut ids = Vec::with_capacity(map.pixels.len() * 3);
    for p in &map.pixels {
        // Index + 1, so 0 is background; fits in 16 bits (checked above).
        let id = p.map_or(0, |p| p + 1);
        ids.extend_from_slice(&[(id & 0xff) as u8, (id >> 8) as u8, 0]);
    }
    Ok(MapImage { width: map.width, height: map.height, ids, labels: map.labels.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_states() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states")
    }

    fn keys() -> Vec<String> {
        ["riverlands", "coast", "dale", "peaks"].map(String::from).to_vec()
    }

    fn true_hash() -> u64 {
        let dir = pax_map::scenario_map_dir(&two_states()).unwrap().unwrap();
        ProvinceMap::load(&dir, &keys()).unwrap().map_hash
    }

    #[test]
    fn the_id_image_encodes_each_pixels_province_plus_one() {
        let map = load(&two_states(), &keys(), Some(true_hash())).unwrap().expect("two_states has a map");
        assert_eq!(map.ids.len(), (map.width * map.height * 3) as usize);
        for (p, &[x, y]) in map.labels.iter().enumerate() {
            let at = ((y * map.width + x) * 3) as usize;
            let id = map.ids[at] as usize + 256 * map.ids[at + 1] as usize;
            assert_eq!(id, p + 1, "the label pixel belongs to its province");
        }
        assert!(map.ids.chunks(3).any(|t| t == [0, 0, 0]), "two_states has sea");
    }

    #[test]
    fn a_map_that_differs_from_the_servers_is_refused() {
        let error = load(&two_states(), &keys(), Some(true_hash() ^ 1)).unwrap_err();
        assert!(error.contains("differs from the server's"), "{error}");
        let error = load(&two_states(), &keys(), None).unwrap_err();
        assert!(error.contains("differs from the server's"), "{error}");
    }
}
