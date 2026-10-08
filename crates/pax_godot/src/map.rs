//! The client's province map (M3-8b, D12): the scenario's map files, read with the
//! same reader the server validates them with (`pax_map`), checked against the
//! session's `StaticData.map_hash`, and turned into the province-ID texture the map
//! shader draws.
//!
//! **ID texture:** RGB8, one texel per map pixel, holding `pax_map`'s id for it
//! (`r + 256·g`: the province index + 1, and [`pax_map::BACKGROUND_ID`] for sea), so a
//! map can have at most `pax_map::MAX_PROVINCES` provinces (the reader enforces it). A map-mode change then rewrites
//! only the small per-province colour table, never this image. `client/ui/map.gdshader`
//! decodes the same texels (it says it mirrors this); picking goes through
//! [`MapImage::province_at`], so GDScript never decodes them.

use std::path::Path;

use pax_map::{ProvinceIds, ProvinceMap};

use crate::decode::WelcomeView;

/// A loaded map: what the bridge keeps for picking and labels. The texels go to
/// Godot once ([`load`] returns them separately) and aren't kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapImage {
    /// Each pixel's `pax_map` id (and the map's size): what [`MapImage::province_at`]
    /// reads.
    pub ids: ProvinceIds,
    /// Each province's label anchor, `[x, y]` in pixels.
    pub labels: Vec<[u32; 2]>,
}

impl MapImage {
    /// The province at pixel `(x, y)`, or `None` for sea or off the map.
    pub fn province_at(&self, x: i64, y: i64) -> Option<u32> {
        self.ids.province_at(x, y)
    }
}

/// Loads the map of the session `welcome` describes, from the scenario's files in
/// `scenario_dir`: from where the server said it is (`map_dir`), checked against its
/// provinces (Welcome order) and its `map_hash`. Returns the map and its RGB8 texels
/// (see the module docs), or `Ok(None)` when the session has no map. Only the server
/// reads `scenario.toml` (D9).
pub fn load(scenario_dir: &Path, welcome: &WelcomeView) -> Result<Option<(MapImage, Vec<u8>)>, String> {
    let (Some(map_dir), Some(expected)) = (welcome.map_dir.as_deref(), welcome.map_hash) else { return Ok(None) };
    let dir = pax_map::resolve_map_dir(scenario_dir, map_dir)?;
    let (map, ids) = ProvinceMap::load_with_ids(&dir, &welcome.provinces).map_err(|errors| errors.join("\n"))?;
    if map.map_hash != expected {
        return Err(format!(
            "this copy of the map ({}) differs from the server's: map hash {:#018x}, the server's is {expected:#018x}",
            dir.display(),
            map.map_hash,
        ));
    }
    let texels = texels(&ids);
    Ok(Some((MapImage { ids, labels: map.labels }, texels)))
}

/// The RGB8 ID texture of `ids`. The reader bounds the provinces at
/// `pax_map::MAX_PROVINCES`, so every id fits the texture's 16 bits.
pub fn texels(ids: &ProvinceIds) -> Vec<u8> {
    let mut texels = Vec::with_capacity(ids.ids().len() * 3);
    for &id in ids.ids() {
        texels.extend_from_slice(&[(id & 0xff) as u8, (id >> 8) as u8, 0]);
    }
    texels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_states() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states")
    }

    /// A session's `Welcome` as far as the map is concerned.
    fn welcome(map_dir: Option<&str>, map_hash: Option<u64>) -> WelcomeView {
        WelcomeView {
            protocol_minor: 1,
            player: 0,
            nation: None,
            day: 0,
            speed: pax_protocol::wire::Speed::Paused,
            scenario: "Two States".into(),
            content_hash: 0,
            map_hash,
            map_dir: map_dir.map(str::to_owned),
            goods: vec![],
            professions: vec![],
            producer_types: vec![],
            provinces: ["riverlands", "coast", "dale", "peaks"].map(String::from).to_vec(),
            province_market: vec![0; 4],
            markets: vec!["m".into()],
            nations: vec![],
            nation_markets: vec![],
        }
    }

    fn true_hash() -> u64 {
        let dir = pax_map::resolve_map_dir(&two_states(), "map").unwrap();
        ProvinceMap::read_with_ids(
            &std::fs::read_to_string(dir.join(pax_map::TOML_FILE)).unwrap(),
            &std::fs::read(dir.join(pax_map::PNG_FILE)).unwrap(),
            &welcome(None, None).provinces,
        )
        .unwrap()
        .0
        .map_hash
    }

    #[test]
    fn the_id_texture_encodes_each_pixels_province_plus_one() {
        let (map, texels) = load(&two_states(), &welcome(Some("map"), Some(true_hash()))).unwrap().expect("a map");
        let (width, height) = (map.ids.width(), map.ids.height());
        assert_eq!(texels.len(), (width * height * 3) as usize);
        for (p, &[x, y]) in map.labels.iter().enumerate() {
            let at = ((y * width + x) * 3) as usize;
            assert_eq!(texels[at] as usize + 256 * texels[at + 1] as usize, p + 1, "the label pixel is its province's");
            assert_eq!(map.province_at(i64::from(x), i64::from(y)), Some(p as u32));
        }
        assert!(texels.chunks(3).any(|t| t == [0, 0, 0]), "two_states has sea");
        assert_eq!(map.province_at(-1, 0), None);
        assert_eq!(map.province_at(i64::from(width), 0), None);
    }

    #[test]
    fn a_map_that_differs_from_the_servers_is_refused() {
        let error = load(&two_states(), &welcome(Some("map"), Some(true_hash() ^ 1))).unwrap_err();
        assert!(error.contains("differs from the server's"), "{error}");
        assert_eq!(load(&two_states(), &welcome(None, None)), Ok(None), "no map in the session: nothing to draw");
        assert!(load(&two_states(), &welcome(Some("../two_states/map"), Some(true_hash()))).is_err());
    }
}
