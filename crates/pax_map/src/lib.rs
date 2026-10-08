//! Province maps (M3-7, D12): the picture the client draws, and which province each
//! colour is. The engine never reads a map: provinces, markets and nations are the
//! scenario's. A map only says where each province is drawn.
//!
//! A scenario names a map directory (`map = "map"` in `scenario.toml`) holding:
//! * `provinces.png`: every province painted in one unique colour (8-bit RGB or
//!   RGBA, no anti-aliasing), optionally on a background colour such as sea;
//! * `provinces.toml`: each province's colour and label anchor (format in DATA_FORMAT).
//!
//! [`ProvinceMap::read`] is the one reader. `pax_data` validates a scenario's map with
//! it, and the client (`pax_godot`) draws the map with it, so the two can't disagree
//! about the format. D9 makes this crate the one exception to "all parsing lives in
//! `pax_data`": the map files only, never `scenario.toml`. It checks the pair against the province keys it is given:
//! * every province appears exactly once, with a unique colour;
//! * every pixel is a listed colour or the background;
//! * every province owns at least one pixel, and its label sits on one of them.
//!
//! The map's two files hash to [`ProvinceMap::map_hash`], `StaticData.map_hash` on the
//! wire (D22): the client compares it with its own copy before drawing.
//!
//! Each pixel's province is kept as a compact `u32` id ([`BACKGROUND_ID`] for sea,
//! index + 1 otherwise), the same encoding as the client's ID texture.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The largest map accepted, per side: a sanity bound against corrupt files.
pub const MAX_SIDE: u32 = 16_384;

/// The most provinces a map can have: what the client's 16-bit ID texture can draw
/// (id 0 is background). The server refuses a bigger map at load, so a client never
/// connects to a game it can't draw.
pub const MAX_PROVINCES: usize = 65_535;

/// The files of a map directory.
pub const TOML_FILE: &str = "provinces.toml";
pub const PNG_FILE: &str = "provinces.png";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MapFile {
    /// Colour of pixels that belong to no province (sea, wasteland), if any.
    background: Option<[u8; 3]>,
    #[serde(default)]
    province: Vec<MapProvinceEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MapProvinceEntry {
    key: String,
    color: [u8; 3],
    /// Pixel `[x, y]` where the province's name is drawn.
    label: [u32; 2],
}

/// A validated province map, in the order of the province keys it was read against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvinceMap {
    pub width: u32,
    pub height: u32,
    /// Each province's colour (index = province id).
    pub colors: Vec<[u8; 3]>,
    /// Each province's label anchor, `[x, y]` in pixels.
    pub labels: Vec<[u32; 2]>,
    pub background: Option<[u8; 3]>,
    /// `pax_content::map_hash` of the two files: `StaticData.map_hash` (D22).
    pub map_hash: u64,
}

/// Each pixel's province id, row by row: what the client draws and picks with. Only
/// [`ProvinceMap::read_with_ids`] makes one, so it always holds `width × height` ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvinceIds {
    width: u32,
    height: u32,
    ids: Vec<u32>,
}

impl ProvinceIds {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Every pixel's id, row by row ([`province_of`] reads one).
    pub fn ids(&self) -> &[u32] {
        &self.ids
    }

    /// The province at pixel `(x, y)`, or `None` for background or off the map.
    pub fn province_at(&self, x: i64, y: i64) -> Option<u32> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        if x >= self.width || y >= self.height {
            return None;
        }
        province_of(self.ids[(y * self.width + x) as usize])
    }
}

/// The id stored for a background pixel (sea). A province's id is its index + 1:
/// the one definition of the encoding, which the client's ID texture also uses
/// (`pax_godot::map`, and its shader, which documents that it mirrors this).
pub const BACKGROUND_ID: u32 = 0;

/// The province index an id stores, or `None` for background.
pub fn province_of(id: u32) -> Option<u32> {
    id.checked_sub(1)
}

/// The id stored for province index `p`.
fn id_of(p: usize) -> u32 {
    u32::try_from(p + 1).expect("fewer provinces than u32::MAX")
}

/// Where a scenario's map lives: its `map` setting, relative to the scenario
/// directory. The one resolution rule: `pax_data` resolves the setting it parsed,
/// and the client the one the server sent (`StaticData.map_dir`).
pub fn resolve_map_dir(scenario_dir: &Path, map: &str) -> PathBuf {
    scenario_dir.join(map)
}

/// RGB pixels of an 8-bit RGB or RGBA PNG.
fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<[u8; 3]>), String> {
    let mut reader =
        png::Decoder::new(std::io::Cursor::new(bytes)).read_info().map_err(|e| format!("not a PNG: {e}"))?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("the image is {width}×{height}; sides must be 1 to {MAX_SIDE} pixels"));
    }
    let channels = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgb, png::BitDepth::Eight) => 3,
        (png::ColorType::Rgba, png::BitDepth::Eight) => 4,
        (kind, depth) => return Err(format!("the image is {kind:?} at {depth:?}; use 8-bit RGB or RGBA")),
    };
    let mut buf = vec![0; reader.output_buffer_size().ok_or("the image is too large to decode")?];
    let frame = reader.next_frame(&mut buf).map_err(|e| format!("the PNG is damaged: {e}"))?;
    let pixels = buf[..frame.buffer_size()].chunks_exact(channels).map(|p| [p[0], p[1], p[2]]).collect();
    Ok((width, height, pixels))
}

impl ProvinceMap {
    /// Validates the two files' contents against `province_keys` (scenario order).
    /// Every problem found is reported, each prefixed `map:`. The server reads maps
    /// this way: it never draws, so it never builds the per-pixel ids.
    pub fn read(toml: &str, png: &[u8], province_keys: &[String]) -> Result<ProvinceMap, Vec<String>> {
        validate(toml, png, province_keys, Ids::Skip).map(|(map, _)| map)
    }

    /// [`ProvinceMap::read`], plus each pixel's province id: what the client draws with.
    pub fn read_with_ids(
        toml: &str,
        png: &[u8],
        province_keys: &[String],
    ) -> Result<(ProvinceMap, ProvinceIds), Vec<String>> {
        let (map, ids) = validate(toml, png, province_keys, Ids::Build)?;
        let ids = ProvinceIds { width: map.width, height: map.height, ids: ids.expect("asked for") };
        Ok((map, ids))
    }

    /// Reads the map files in `dir` with their per-pixel ids (the client).
    pub fn load_with_ids(dir: &Path, province_keys: &[String]) -> Result<(ProvinceMap, ProvinceIds), Vec<String>> {
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| vec![format!("map: {name}: {e}")]);
        let toml = read(TOML_FILE)?;
        let png = read(PNG_FILE)?;
        let toml = String::from_utf8(toml).map_err(|_| vec![format!("map: {TOML_FILE} is not UTF-8")])?;
        ProvinceMap::read_with_ids(&toml, &png, province_keys)
    }
}

/// Whether a read builds each pixel's province id.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ids {
    /// The server: it never draws.
    Skip,
    /// The client: it draws and picks with them.
    Build,
}

/// The checks every reader runs; the per-pixel ids only when asked for.
fn validate(
    toml: &str,
    png: &[u8],
    province_keys: &[String],
    want: Ids,
) -> Result<(ProvinceMap, Option<Vec<u32>>), Vec<String>> {
    if province_keys.len() > MAX_PROVINCES {
        return Err(vec![format!(
            "map: the scenario has {} provinces; a map can draw at most {MAX_PROVINCES}",
            province_keys.len()
        )]);
    }
    let file: MapFile = toml::from_str(toml).map_err(|e| vec![format!("map: {TOML_FILE}: {e}")])?;
    let (width, height, image) = decode_png(png).map_err(|e| vec![format!("map: {PNG_FILE}: {e}")])?;

    let keys = province_keys;
    let mut errors = Vec::new();
    let mut colors: Vec<Option<[u8; 3]>> = vec![None; keys.len()];
    let mut labels = vec![[0, 0]; keys.len()];
    let mut owner: BTreeMap<[u8; 3], usize> = BTreeMap::new();
    for entry in &file.province {
        let Some(p) = keys.iter().position(|k| *k == entry.key) else {
            errors.push(format!("map: province '{}' is not in the scenario", entry.key));
            continue;
        };
        if colors[p].is_some() {
            errors.push(format!("map: province '{}' is listed twice", entry.key));
            continue;
        }
        if Some(entry.color) == file.background {
            errors.push(format!("map: province '{}' uses the background colour", entry.key));
        }
        if let Some(&other) = owner.get(&entry.color) {
            errors.push(format!(
                "map: provinces '{}' and '{}' share the colour {:?}",
                keys[other], entry.key, entry.color
            ));
        }
        owner.insert(entry.color, p);
        colors[p] = Some(entry.color);
        labels[p] = entry.label;
    }
    for (p, color) in colors.iter().enumerate() {
        if color.is_none() {
            errors.push(format!("map: province '{}' has no colour", keys[p]));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Every pixel must belong to a province or the background; every province needs pixels.
    let mut painted = vec![0u64; keys.len()];
    let mut stray: BTreeMap<[u8; 3], (u32, u32)> = BTreeMap::new();
    let mut ids = Vec::with_capacity(if want == Ids::Build { image.len() } else { 0 });
    for (i, px) in image.iter().enumerate() {
        let p = owner.get(px).copied();
        match p {
            Some(p) => painted[p] += 1,
            None if Some(*px) == file.background => {}
            None => {
                stray.entry(*px).or_insert((i as u32 % width, i as u32 / width));
            }
        }
        if want == Ids::Build {
            ids.push(p.map_or(BACKGROUND_ID, id_of));
        }
    }
    const SHOWN: usize = 5;
    for (color, (x, y)) in stray.iter().take(SHOWN) {
        errors.push(format!("map: colour {color:?} (first at pixel {x},{y}) is not a province or the background"));
    }
    if stray.len() > SHOWN {
        errors.push(format!("map: ...and {} more stray colours", stray.len() - SHOWN));
    }
    for (p, &n) in painted.iter().enumerate() {
        if n == 0 {
            errors.push(format!("map: province '{}' has no pixels", keys[p]));
        }
    }
    for (p, &[x, y]) in labels.iter().enumerate() {
        let on_own_pixel = x < width && y < height && owner.get(&image[(y * width + x) as usize]) == Some(&p);
        if !on_own_pixel {
            errors.push(format!("map: province '{}' has its label at {x},{y}, outside its own pixels", keys[p]));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let map = ProvinceMap {
        width,
        height,
        colors: colors.into_iter().map(|c| c.expect("checked above")).collect(),
        labels,
        background: file.background,
        map_hash: pax_content::map_hash(toml.as_bytes(), png),
    };
    Ok((map, (want == Ids::Build).then_some(ids)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_states() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states")
    }

    #[test]
    fn the_two_states_map_reads_with_every_pixel_assigned() {
        let keys: Vec<String> = ["riverlands", "coast", "dale", "peaks"].map(String::from).to_vec();
        let dir = resolve_map_dir(&two_states(), "map");
        let (map, ids) = ProvinceMap::load_with_ids(&dir, &keys).unwrap();
        assert_eq!(ids.ids().len(), (map.width * map.height) as usize);
        for (p, &[x, y]) in map.labels.iter().enumerate() {
            assert_eq!(ids.province_at(i64::from(x), i64::from(y)), Some(p as u32), "a label sits on its province");
        }
        assert_eq!(ids.province_at(-1, 0), None);
        let toml = std::fs::read(dir.join(TOML_FILE)).unwrap();
        let png = std::fs::read(dir.join(PNG_FILE)).unwrap();
        assert_eq!(map.map_hash, pax_content::map_hash(&toml, &png));
    }

    /// The client's ID texture draws at most MAX_PROVINCES; the reader refuses more,
    /// so the server never serves a map a client can't draw.
    #[test]
    fn more_provinces_than_a_map_can_draw_are_refused() {
        let keys: Vec<String> = (0..=MAX_PROVINCES).map(|p| format!("p{p}")).collect();
        let dir = resolve_map_dir(&two_states(), "map");
        let errors = ProvinceMap::load_with_ids(&dir, &keys).unwrap_err();
        assert!(errors[0].contains("at most 65535"), "{errors:?}");
    }

    #[test]
    fn keys_the_map_does_not_cover_are_reported() {
        let keys: Vec<String> = ["riverlands", "coast", "dale", "peaks", "nowhere"].map(String::from).to_vec();
        let dir = resolve_map_dir(&two_states(), "map");
        let errors = ProvinceMap::load_with_ids(&dir, &keys).unwrap_err();
        assert_eq!(errors, ["map: province 'nowhere' has no colour"]);
    }
}
