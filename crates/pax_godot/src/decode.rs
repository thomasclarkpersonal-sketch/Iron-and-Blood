//! Server messages decoded into plain Rust values: no Godot types, so this is
//! unit-tested without a running engine. `lib.rs` turns these into Godot values.
//!
//! This is presentation code (D3): `Fixed` values become `f32` here, for display only.
//! Nothing decoded here flows back into the simulation. Commands are built from the
//! raw integers the UI chose, never from these floats.

use pax_protocol::{ProtocolError, read_server_message, wire};

/// A `Fixed` raw value (value × 10⁶) as a display float.
pub fn display(fixed: wire::Fixed) -> f32 {
    (fixed.raw() as f64 / 1_000_000.0) as f32
}

/// The parts of `Welcome` the client needs to lay out the map and labels.
#[derive(Debug, Clone, PartialEq)]
pub struct WelcomeView {
    pub day: u64,
    pub scenario: String,
    pub content_hash: u64,
    pub goods: Vec<String>,
    pub provinces: Vec<String>,
    pub province_market: Vec<u32>,
    pub markets: Vec<String>,
    pub nations: Vec<String>,
}

/// The parts of `DayUpdate` the spike draws: the day, and the subscribed map mode.
#[derive(Debug, Clone, PartialEq)]
pub struct DayUpdateView {
    pub day: u64,
    pub skipped: u32,
    pub state_hash: u64,
    pub map_mode: Option<wire::MapMode>,
    pub map_values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    Welcome(WelcomeView),
    DayUpdate(DayUpdateView),
    /// A message kind the spike doesn't handle yet, by name.
    Other(&'static str),
}

fn strings(v: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<&str>>>) -> Vec<String> {
    v.map(|v| v.iter().map(str::to_owned).collect()).unwrap_or_default()
}

/// Decodes one verified frame (length prefix included).
pub fn decode_server_frame(frame: &[u8]) -> Result<ServerEvent, ProtocolError> {
    let msg = read_server_message(frame)?;
    if let Some(w) = msg.payload_as_welcome() {
        let defs = w.defs();
        return Ok(ServerEvent::Welcome(WelcomeView {
            day: w.day(),
            scenario: w.scenario().unwrap_or_default().to_owned(),
            content_hash: w.content_hash(),
            goods: strings(defs.and_then(|d| d.goods())),
            provinces: strings(defs.and_then(|d| d.provinces())),
            province_market: defs.and_then(|d| d.province_market()).map(|v| v.iter().collect()).unwrap_or_default(),
            markets: strings(defs.and_then(|d| d.markets())),
            nations: defs
                .and_then(|d| d.nations())
                .map(|v| v.iter().map(|n| n.key().unwrap_or_default().to_owned()).collect())
                .unwrap_or_default(),
        }));
    }
    if let Some(u) = msg.payload_as_day_update() {
        let map = u.map();
        return Ok(ServerEvent::DayUpdate(DayUpdateView {
            day: u.day(),
            skipped: u.skipped(),
            state_hash: u.state_hash(),
            map_mode: map.map(|m| m.mode()),
            map_values: map
                .and_then(|m| m.values())
                .map(|v| v.iter().map(|f| display(*f)).collect())
                .unwrap_or_default(),
        }));
    }
    Ok(ServerEvent::Other(msg.payload_type().variant_name().unwrap_or("unknown")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use pax_protocol::{Direction, FrameDecoder};

    #[test]
    fn decodes_the_demo_session() {
        let mut d = FrameDecoder::new(Direction::ServerToClient);
        d.push(&demo::frames(12));
        let welcome = decode_server_frame(&d.next_frame().unwrap().unwrap()).unwrap();
        let update = decode_server_frame(&d.next_frame().unwrap().unwrap()).unwrap();
        let ServerEvent::Welcome(w) = welcome else { panic!("expected Welcome, got {welcome:?}") };
        assert_eq!((w.provinces.len(), w.province_market.len(), w.nations.len()), (12, 12, 2));
        let ServerEvent::DayUpdate(u) = update else { panic!("expected DayUpdate, got {update:?}") };
        assert_eq!((u.map_mode, u.map_values.len()), (Some(wire::MapMode::Population), 12));
        assert!(u.map_values.iter().all(|v| v.is_finite() && *v > 0.0));
    }

    #[test]
    fn display_converts_fixed_raw_values() {
        assert_eq!(display(wire::Fixed::new(1_500_000)), 1.5);
        assert_eq!(display(wire::Fixed::new(-250_000)), -0.25);
    }
}
