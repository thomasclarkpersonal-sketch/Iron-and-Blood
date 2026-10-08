//! Demo data for the M3-0 spike, until `pax_server` sends real frames (M3-2, M3-3):
//! a `Welcome` and a `DayUpdate` built with the real schema, and a province-ID image.
//!
//! Deterministic integer maths only, so the demo looks the same on every machine.

use flatbuffers::FlatBufferBuilder;
use pax_protocol::wire::*;

/// A `Welcome` for `provinces` provinces in two nations, followed by a `DayUpdate`
/// with a population map, as one byte stream of two size-prefixed frames.
pub fn frames(provinces: usize) -> Vec<u8> {
    let mut out = welcome(provinces);
    out.extend(day_update(provinces));
    out
}

fn welcome(provinces: usize) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let names: Vec<_> = (0..provinces).map(|p| b.create_string(&format!("province_{p}"))).collect();
    let names = b.create_vector(&names);
    // Two markets: the west half and the east half of the provinces.
    let province_market =
        b.create_vector(&(0..provinces as u32).map(|p| u32::from(p * 2 >= provinces as u32)).collect::<Vec<_>>());
    let markets = ["west", "east"].map(|s| b.create_string(s));
    let markets = b.create_vector(&markets);
    let nations: Vec<_> = [("west_kingdom", 0u32), ("east_republic", 1)]
        .into_iter()
        .map(|(key, market)| {
            let key = b.create_string(key);
            let markets = b.create_vector(&[market]);
            NationDef::create(&mut b, &NationDefArgs { key: Some(key), markets: Some(markets) })
        })
        .collect();
    let nations = b.create_vector(&nations);
    let goods = ["grain", "coal", "cloth"].map(|s| b.create_string(s));
    let goods = b.create_vector(&goods);
    let defs = StaticData::create(
        &mut b,
        &StaticDataArgs {
            goods: Some(goods),
            provinces: Some(names),
            province_market: Some(province_market),
            markets: Some(markets),
            nations: Some(nations),
            ..Default::default()
        },
    );
    let scenario = b.create_string("m3_spike_demo");
    let w = Welcome::create(
        &mut b,
        &WelcomeArgs {
            protocol_major: pax_protocol::PROTOCOL_MAJOR,
            protocol_minor: pax_protocol::PROTOCOL_MINOR,
            scenario: Some(scenario),
            day: 0,
            speed: Speed::Paused,
            defs: Some(defs),
            ..Default::default()
        },
    );
    let msg = ServerMessage::create(
        &mut b,
        &ServerMessageArgs { payload_type: ServerPayload::Welcome, payload: Some(w.as_union_value()) },
    );
    finish_size_prefixed_server_message_buffer(&mut b, msg);
    b.finished_data().to_vec()
}

fn day_update(provinces: usize) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    // Population between 10,000 and 1,010,000, scattered by a hash of the index.
    let values: Vec<Fixed> =
        (0..provinces as u64).map(|p| Fixed::new(((10_000 + hash(p) % 1_000_000) * 1_000_000) as i64)).collect();
    let values = b.create_vector(&values);
    let map = MapView::create(&mut b, &MapViewArgs { mode: MapMode::Population, good: 0, values: Some(values) });
    let u = DayUpdate::create(
        &mut b,
        &DayUpdateArgs { day: 1, speed: Speed::Normal, state_hash: 0x5EED, map: Some(map), ..Default::default() },
    );
    let msg = ServerMessage::create(
        &mut b,
        &ServerMessageArgs { payload_type: ServerPayload::DayUpdate, payload: Some(u.as_union_value()) },
    );
    finish_size_prefixed_server_message_buffer(&mut b, msg);
    b.finished_data().to_vec()
}

/// splitmix64: a well-mixed deterministic hash.
fn hash(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// An RGB8 province-ID image, `width × height`: each pixel stores its province index
/// as `r + 256·g`, the encoding the map shader reads (D12). Provinces are Voronoi
/// cells around jittered grid points, a stand-in until M3-7's real map files.
/// Each pixel searches only the neighbouring grid cells, so this is O(pixels).
pub fn province_id_image(width: usize, height: usize, provinces: usize) -> Vec<u8> {
    let cols = (provinces as f64 * width as f64 / height as f64).sqrt().ceil().max(1.0) as usize;
    let rows = provinces.div_ceil(cols);
    let (cw, ch) = (width.div_ceil(cols), height.div_ceil(rows));
    // One seed per cell, jittered inside it; cells past `provinces` have no seed.
    let seed = |c: usize, r: usize| -> Option<(i64, i64)> {
        let id = r * cols + c;
        (c < cols && r < rows && id < provinces).then(|| {
            let h = hash(id as u64);
            ((c * cw + (h % cw as u64) as usize) as i64, (r * ch + ((h >> 32) % ch as u64) as usize) as i64)
        })
    };
    let mut img = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let (cx, cy) = (x / cw, y / ch);
            let mut best = (i64::MAX, 0usize);
            for r in cy.saturating_sub(2)..=cy + 2 {
                for c in cx.saturating_sub(2)..=cx + 2 {
                    if let Some((sx, sy)) = seed(c, r) {
                        let d = (sx - x as i64).pow(2) + (sy - y as i64).pow(2);
                        best = best.min((d, r * cols + c));
                    }
                }
            }
            img.extend([(best.1 % 256) as u8, (best.1 / 256) as u8, 0]);
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_province_appears_in_the_image() {
        let (w, h, n) = (192, 108, 50);
        let img = province_id_image(w, h, n);
        let mut seen = vec![false; n];
        for px in img.chunks(3) {
            seen[px[0] as usize + 256 * px[1] as usize] = true;
        }
        assert!(seen.iter().all(|&s| s), "a province has no pixels");
    }
}
