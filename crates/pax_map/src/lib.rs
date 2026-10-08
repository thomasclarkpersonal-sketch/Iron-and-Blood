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
//! about the format. It checks the pair against the province keys it is given:
//! * every province appears exactly once, with a unique colour;
//! * every pixel is a listed colour or the background;
//! * every province owns at least one pixel, and its label sits on one of them.
//!
//! The map's two files hash to [`ProvinceMap::map_hash`], `StaticData.map_hash` on the
//! wire (D22): the client compares it with its own copy before drawing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The largest map accepted, per side: a sanity bound against corrupt files.
pub const MAX_SIDE: u32 = 16_384;

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
    /// The province each pixel belongs to, row by row; `None` for background.
    pub pixels: Vec<Option<u32>>,
    /// `pax_content::map_hash` of the two files: `StaticData.map_hash` (D22).
    pub map_hash: u64,
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
    /// Reads the map files in `dir` against `province_keys` (scenario order). Every
    /// problem found is reported, each prefixed `map:`.
    pub fn load(dir: &Path, province_keys: &[String]) -> Result<ProvinceMap, Vec<String>> {
        let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| vec![format!("map: {name}: {e}")]);
        let toml = read(TOML_FILE)?;
        let png = read(PNG_FILE)?;
        let toml = String::from_utf8(toml).map_err(|_| vec![format!("map: {TOML_FILE} is not UTF-8")])?;
        ProvinceMap::read(&toml, &png, province_keys)
    }

    /// Validates the two files' contents against `province_keys` (scenario order).
    pub fn read(toml: &str, png: &[u8], province_keys: &[String]) -> Result<ProvinceMap, Vec<String>> {
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
        let mut pixels = Vec::with_capacity(image.len());
        for (i, px) in image.iter().enumerate() {
            let p = owner.get(px).copied();
            match p {
                Some(p) => painted[p] += 1,
                None if Some(*px) == file.background => {}
                None => {
                    stray.entry(*px).or_insert((i as u32 % width, i as u32 / width));
                }
            }
            pixels.push(p.map(|p| p as u32));
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
            let on_own_pixel = x < width && y < height && pixels[(y * width + x) as usize] == Some(p as u32);
            if !on_own_pixel {
                errors.push(format!("map: province '{}' has its label at {x},{y}, outside its own pixels", keys[p]));
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(ProvinceMap {
            width,
            height,
            colors: colors.into_iter().map(|c| c.expect("checked above")).collect(),
            labels,
            background: file.background,
            pixels,
            map_hash: pax_content::map_hash(toml.as_bytes(), png),
        })
    }
}

/// The map directory a scenario names (`map = "…"` in its `scenario.toml`), or `None`
/// if it has no map. For the client, which has the scenario's files but not
/// `pax_data`'s full loader; `pax_data` reads the same key with the rest of the file.
pub fn scenario_map_dir(scenario_dir: &Path) -> Result<Option<PathBuf>, String> {
    let path = scenario_dir.join("scenario.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: toml::Table = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    match value.get("map") {
        None => Ok(None),
        Some(toml::Value::String(dir)) => Ok(Some(scenario_dir.join(dir))),
        Some(_) => Err(format!("{}: `map` must be a directory name", path.display())),
    }
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
        let dir = scenario_map_dir(&two_states()).unwrap().expect("two_states has a map");
        let map = ProvinceMap::load(&dir, &keys).unwrap();
        assert_eq!(map.pixels.len(), (map.width * map.height) as usize);
        for (p, &[x, y]) in map.labels.iter().enumerate() {
            assert_eq!(map.pixels[(y * map.width + x) as usize], Some(p as u32), "a label sits on its province");
        }
        let toml = std::fs::read(dir.join(TOML_FILE)).unwrap();
        let png = std::fs::read(dir.join(PNG_FILE)).unwrap();
        assert_eq!(map.map_hash, pax_content::map_hash(&toml, &png));
    }

    #[test]
    fn keys_the_map_does_not_cover_are_reported() {
        let keys: Vec<String> = ["riverlands", "coast", "dale", "peaks", "nowhere"].map(String::from).to_vec();
        let dir = scenario_map_dir(&two_states()).unwrap().unwrap();
        let errors = ProvinceMap::load(&dir, &keys).unwrap_err();
        assert_eq!(errors, ["map: province 'nowhere' has no colour"]);
    }
}
