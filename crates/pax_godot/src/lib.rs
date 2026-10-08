//! `pax_godot`: the client's side of the wire protocol, as a Godot GDExtension (D12).
//!
//! GDScript owns the UI. This library owns everything about the protocol: framing,
//! size limits, verification, and decoding into Godot values. That code is shared with
//! the server through `pax_protocol` and tested in Rust (`decode.rs`), not written a
//! second time in another language.
//!
//! It depends on `pax_protocol` only, never on `pax_engine`, so the client can't
//! simulate (D10).
//!
//! **M3-0 spike state:** decoding of `Welcome` and `DayUpdate`, plus demo data
//! (`demo.rs`) until the server sends real frames. The TCP connection and the rest of
//! the messages arrive with M3-8.

// Presentation code: Fixed becomes float for display only (D3), as in pax_cli.
#![allow(clippy::float_arithmetic)]

pub mod decode;
pub mod demo;

use decode::ServerEvent;
use godot::prelude::*;
use pax_protocol::{Direction, FrameDecoder};

/// The extension's entry point. godot-rust requires it to be an `unsafe impl`; this
/// module is the crate's only `unsafe` (see Cargo.toml).
mod entry {
    #![allow(unsafe_code)]
    use godot::prelude::*;

    struct PaxExtension;

    #[gdextension]
    unsafe impl ExtensionLibrary for PaxExtension {}
}

/// Turns bytes from the server into decoded messages for GDScript.
///
/// Feed it whatever the socket delivered, in any chunking, with `push`. It returns
/// every complete message as a `Dictionary` with a `"type"` key. A protocol error is
/// fatal: `push` returns what decoded before the error and sets `last_error`, and the
/// caller must disconnect (D22).
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct PaxServerReader {
    decoder: FrameDecoder,
    last_error: GString,
}

#[godot_api]
impl IRefCounted for PaxServerReader {
    fn init(_base: Base<RefCounted>) -> Self {
        PaxServerReader { decoder: FrameDecoder::new(Direction::ServerToClient), last_error: GString::new() }
    }
}

#[godot_api]
impl PaxServerReader {
    #[func]
    fn push(&mut self, bytes: PackedByteArray) -> Array<VarDictionary> {
        self.decoder.push(bytes.as_slice());
        let mut out = Array::new();
        loop {
            match self.decoder.next_frame() {
                Ok(Some(frame)) => match decode::decode_server_frame(&frame) {
                    Ok(event) => out.push(&to_dictionary(event)),
                    Err(e) => {
                        self.last_error = GString::from(&e.to_string());
                        break;
                    }
                },
                Ok(None) => break,
                Err(e) => {
                    self.last_error = GString::from(&e.to_string());
                    break;
                }
            }
        }
        out
    }

    /// Empty while the stream is healthy.
    #[func]
    fn last_error(&self) -> GString {
        self.last_error.clone()
    }
}

fn strings(v: &[String]) -> PackedStringArray {
    v.iter().map(GString::from).collect()
}

fn to_dictionary(event: ServerEvent) -> VarDictionary {
    let mut d = VarDictionary::new();
    match event {
        ServerEvent::Welcome(w) => {
            d.set("type", "welcome");
            d.set("day", w.day as i64);
            d.set("scenario", &GString::from(&w.scenario));
            // Godot ints are signed 64-bit; the hash is an identifier, so its bits are kept as-is.
            d.set("content_hash", w.content_hash as i64);
            d.set("goods", &strings(&w.goods));
            d.set("provinces", &strings(&w.provinces));
            d.set("province_market", &w.province_market.iter().map(|&m| m as i32).collect::<PackedInt32Array>());
            d.set("markets", &strings(&w.markets));
            d.set("nations", &strings(&w.nations));
        }
        ServerEvent::DayUpdate(u) => {
            d.set("type", "day_update");
            d.set("day", u.day as i64);
            d.set("skipped", u.skipped as i64);
            d.set("state_hash", u.state_hash as i64);
            d.set("map_mode", u.map_mode.map_or(0, |m| i64::from(m.0)));
            d.set("map_values", &u.map_values.into_iter().collect::<PackedFloat32Array>());
        }
        ServerEvent::Other(name) => {
            d.set("type", name);
        }
    }
    d
}

/// Demo data for the M3-0 spike (see `demo.rs`), until the server exists.
#[derive(GodotClass)]
#[class(base = RefCounted, init)]
pub struct PaxDemo;

#[godot_api]
impl PaxDemo {
    /// A `Welcome` then a `DayUpdate` for `provinces` provinces, as raw server bytes.
    #[func]
    fn frames(provinces: i64) -> PackedByteArray {
        PackedByteArray::from(demo::frames(provinces.clamp(1, 65_535) as usize).as_slice())
    }

    /// An RGB8 province-ID image (`r + 256·g` = province index) for the map shader.
    #[func]
    fn province_id_image(width: i64, height: i64, provinces: i64) -> PackedByteArray {
        let (w, h, n) =
            (width.clamp(1, 8192) as usize, height.clamp(1, 8192) as usize, provinces.clamp(1, 65_535) as usize);
        PackedByteArray::from(demo::province_id_image(w, h, n).as_slice())
    }
}
