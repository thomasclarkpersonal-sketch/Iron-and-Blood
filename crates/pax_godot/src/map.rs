//! The client's province map (M3-8b, D12): the scenario's map files, read with the
//! same reader the server validates them with (`pax_map`), checked against the
//! session's `StaticData.map_hash`, and turned into the province-ID texture the map
//! shader draws.
//!
//! **ID texture:** RGB8, one texel per map pixel, holding `pax_map`'s id for it
//! (`r + 256·g`: the province index + 1, and [`pax_map::BACKGROUND_ID`] for sea), so a
//! map can have at most [`MAX_PROVINCES`] provinces. A map-mode change then rewrites
//! only the small per-province colour table, never this image. `client/ui/map.gdshader`
//! decodes the same texels (it says it mirrors this); picking goes through
//! [`MapImage::province_at`], so GDScript never decodes them.

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
    pub texels: Vec<u8>,
    /// Each pixel's `pax_map` id, row by row: what [`MapImage::province_at`] reads.
    pub ids: Vec<u32>,
    /// Each province's label anchor, `[x, y]` in pixels.
    pub labels: Vec<[u32; 2]>,
}

impl MapImage {
    /// The province at pixel `(x, y)`, or `None` for sea or off the map.
    pub fn province_at(&self, x: i64, y: i64) -> Option<u32> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        if x >= self.width || y >= self.height {
            return None;
        }
        pax_map::province_of(self.ids[(y * self.width + x) as usize])
    }
}

/// Loads the map of the scenario in `scenario_dir` for a session whose `Welcome`
/// said where its map is (`map_dir`, relative to the scenario) and what its files
/// hash to (`expected_hash`), with `provinces` in Welcome order. `Ok(None)` when the
/// session has no map. Only the server reads `scenario.toml` (D9).
pub fn load(
    scenario_dir: &Path,
    map_dir: Option<&str>,
    provinces: &[String],
    expected_hash: Option<u64>,
) -> Result<Option<MapImage>, String> {
    let (Some(map_dir), Some(expected)) = (map_dir, expected_hash) else { return Ok(None) };
    let dir = pax_map::resolve_map_dir(scenario_dir, map_dir);
    let map = ProvinceMap::load(&dir, provinces, true).map_err(|errors| errors.join("\n"))?;
    if map.map_hash != expected {
        return Err(format!(
            "this copy of the map ({}) differs from the server's: map hash {:#018x}, the server's is {expected:#018x}",
            dir.display(),
            map.map_hash,
        ));
    }
    image(&map).map(Some)
}

/// The ID texture of a validated map.
pub fn image(map: &ProvinceMap) -> Result<MapImage, String> {
    if map.colors.len() > MAX_PROVINCES {
        return Err(format!("the map has {} provinces; the client draws at most {MAX_PROVINCES}", map.colors.len()));
    }
    let mut texels = Vec::with_capacity(map.ids.len() * 3);
    for &id in &map.ids {
        // Fits in 16 bits (checked above).
        texels.extend_from_slice(&[(id & 0xff) as u8, (id >> 8) as u8, 0]);
    }
    Ok(MapImage { width: map.width, height: map.height, texels, ids: map.ids.clone(), labels: map.labels.clone() })
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
        ProvinceMap::load(&pax_map::resolve_map_dir(&two_states(), "map"), &keys(), false).unwrap().map_hash
    }

    #[test]
    fn the_id_image_encodes_each_pixels_province_plus_one() {
        let map = load(&two_states(), Some("map"), &keys(), Some(true_hash())).unwrap().expect("two_states has a map");
        assert_eq!(map.texels.len(), (map.width * map.height * 3) as usize);
        for (p, &[x, y]) in map.labels.iter().enumerate() {
            let at = ((y * map.width + x) * 3) as usize;
            let id = map.texels[at] as usize + 256 * map.texels[at + 1] as usize;
            assert_eq!(id, p + 1, "the label pixel belongs to its province");
            assert_eq!(map.province_at(i64::from(x), i64::from(y)), Some(p as u32));
        }
        assert!(map.texels.chunks(3).any(|t| t == [0, 0, 0]), "two_states has sea");
        assert_eq!(map.province_at(-1, 0), None);
        assert_eq!(map.province_at(i64::from(map.width), 0), None);
    }

    #[test]
    fn a_map_that_differs_from_the_servers_is_refused() {
        let error = load(&two_states(), Some("map"), &keys(), Some(true_hash() ^ 1)).unwrap_err();
        assert!(error.contains("differs from the server's"), "{error}");
        assert_eq!(load(&two_states(), None, &keys(), None), Ok(None), "no map in the session: nothing to draw");
    }
}
