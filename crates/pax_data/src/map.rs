//! Province maps (M3-7, D12): the picture the client draws, and how it maps to the
//! scenario's provinces. The engine never reads it: provinces, markets and nations
//! are the scenario's. A map only says where each province is drawn.
//!
//! A scenario names a map directory (`map = "map"` in `scenario.toml`) holding:
//! * `provinces.png`: every province painted in one unique colour (8-bit RGB or
//!   RGBA, no anti-aliasing), optionally on a background colour such as sea;
//! * `provinces.toml`: each province's colour and label anchor (format in DATA_FORMAT).
//!
//! Loading validates the pair against the scenario:
//! * every province appears exactly once, with a unique colour;
//! * every pixel is a listed colour or the background;
//! * every province owns at least one pixel, and its label sits on one of them.
//!
//! The client turns the image into its province-ID texture with the colour table,
//! and checks `StaticData.map_hash` against its own copy of these two files.

use std::collections::BTreeMap;
use std::path::Path;

use pax_content::ContentHash;
use pax_engine::World;
use serde::Deserialize;

use crate::{LoadError, parse, read};

/// The largest map accepted, per side: a sanity bound against corrupt files.
const MAX_SIDE: u32 = 16_384;

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

/// Loads and validates the map in `dir` against `world`'s provinces.
pub(crate) fn load(dir: &Path, world: &World) -> Result<(MapData, MapFiles), LoadError> {
    let toml = read(&dir.join("provinces.toml"))?;
    let png_path = dir.join("provinces.png");
    let png = std::fs::read(&png_path).map_err(|e| LoadError::single(format!("{}: {e}", png_path.display())))?;
    let file: MapFile = parse("map/provinces.toml", &toml)?;
    let (width, height, pixels) =
        decode_png(&png).map_err(|e| LoadError::single(format!("{}: {e}", png_path.display())))?;

    let keys = &world.geography.province_keys;
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
        return Err(LoadError { messages: errors });
    }

    // Every pixel must belong to a province or the background; every province needs pixels.
    let mut painted = vec![0u64; keys.len()];
    let mut stray: BTreeMap<[u8; 3], (u32, u32)> = BTreeMap::new();
    for (i, px) in pixels.iter().enumerate() {
        match owner.get(px) {
            Some(&p) => painted[p] += 1,
            None if Some(*px) == file.background => {}
            None => {
                stray.entry(*px).or_insert((i as u32 % width, i as u32 / width));
            }
        }
    }
    for (color, (x, y)) in stray.iter().take(5) {
        errors.push(format!("map: colour {color:?} (first at pixel {x},{y}) is not a province or the background"));
    }
    for (p, &n) in painted.iter().enumerate() {
        if n == 0 {
            errors.push(format!("map: province '{}' has no pixels", keys[p]));
        }
    }
    for (p, &[x, y]) in labels.iter().enumerate() {
        let on_own_pixel = x < width && y < height && owner.get(&pixels[(y * width + x) as usize]) == Some(&p);
        if !on_own_pixel {
            errors.push(format!("map: province '{}' has its label at {x},{y}, outside its own pixels", keys[p]));
        }
    }
    if !errors.is_empty() {
        return Err(LoadError { messages: errors });
    }

    let map_hash = pax_content::map_hash(toml.as_bytes(), &png);
    let files = MapFiles { toml, png };
    let map = MapData {
        width,
        height,
        colors: colors.into_iter().map(|c| c.expect("checked above")).collect(),
        labels,
        background: file.background,
        map_hash,
    };
    Ok((map, files))
}
