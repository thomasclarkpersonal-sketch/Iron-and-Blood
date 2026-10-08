//! `pax_godot`: the client's side of the wire protocol, as a Godot GDExtension (D12).
//!
//! GDScript owns the UI. This library owns everything about the protocol: framing,
//! size limits, verification, session rules, and decoding into Godot values. That
//! logic lives in `decode.rs` as plain Rust, unit-tested without Godot and shared with
//! the server through `pax_protocol`. This file only converts its results into Godot
//! types.
//!
//! It depends on `pax_protocol` only, never on `pax_engine`, so the client can't
//! simulate (D10). Float arithmetic is linted everywhere except `decode::display`:
//! code that builds commands (simulation input) must use integers (D3).
//!
//! **M3-0 state:** decoding `Welcome` and `DayUpdate`, plus demo data behind the
//! `demo` feature. The TCP connection and the remaining messages arrive with M3-8.

pub mod decode;
#[cfg(any(test, feature = "demo"))]
pub mod demo;

use decode::{ServerEvent, ServerStream};
use godot::prelude::*;

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
/// every complete message as a `Dictionary`, whose `"type"` is the message's
/// `snake_case` tag: `"welcome"`, `"day_update"`, `"pong"` and so on.
///
/// The first protocol error ends the stream for good: `push` returns what decoded
/// before it, `failed()` turns true, and later input is ignored. The caller must
/// disconnect (D22). The bridge enforces this, not GDScript.
///
/// An absent id is `null`, never a number. A sandbox session's `"nation"`, for
/// example, is `null`, so a lookup such as `nations[welcome["nation"]]` fails loudly
/// instead of silently indexing from the end (D22: no `-1` sentinels).
#[derive(GodotClass)]
#[class(base = RefCounted, init)]
pub struct PaxServerReader {
    stream: ServerStream,
}

#[godot_api]
impl PaxServerReader {
    #[func]
    fn push(&mut self, bytes: PackedByteArray) -> Array<VarDictionary> {
        let mut out = Array::new();
        for event in self.stream.push(bytes.as_slice()) {
            out.push(&to_dictionary(event));
        }
        out
    }

    /// True once the stream has failed. It never resets.
    #[func]
    fn failed(&self) -> bool {
        self.stream.error().is_some()
    }

    /// Why the stream failed, for the connection-lost screen; empty while healthy.
    #[func]
    fn error(&self) -> GString {
        self.stream.error().map(|e| GString::from(&e.to_string())).unwrap_or_default()
    }
}

fn strings(v: &[String]) -> PackedStringArray {
    v.iter().map(GString::from).collect()
}

fn to_dictionary(event: ServerEvent) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("type", event.tag());
    match event {
        ServerEvent::Welcome(w) => {
            d.set("protocol_minor", i64::from(w.protocol_minor));
            d.set("nation", &w.nation.map_or(Variant::nil(), |n| Variant::from(i64::from(n))));
            d.set("day", w.day as i64);
            d.set("scenario", &GString::from(&w.scenario));
            // Godot ints are signed 64-bit; the hash is an identifier, so its bits are kept as-is.
            d.set("content_hash", w.content_hash as i64);
            d.set("goods", &strings(&w.goods));
            d.set("professions", &strings(&w.professions));
            d.set("provinces", &strings(&w.provinces));
            d.set("province_market", &w.province_market.iter().map(|&m| m as i32).collect::<PackedInt32Array>());
            d.set("markets", &strings(&w.markets));
            d.set("nations", &strings(&w.nations));
        }
        ServerEvent::DayUpdate(u) => {
            d.set("day", u.day as i64);
            d.set("skipped", i64::from(u.skipped));
            // Bits kept as-is (an identifier); null between checkpoint days (D23).
            d.set("state_hash", &u.state_hash.map_or(Variant::nil(), |h| Variant::from(h as i64)));
            d.set("map_mode", u.map_mode.map_or(0, |m| i64::from(m.0)));
            d.set("map_values", &u.map_values.into_iter().collect::<PackedFloat32Array>());
        }
        ServerEvent::Other(_) => {}
    }
    d
}

/// Demo data for the M3-0 spike (see `demo.rs`), only with the `demo` feature.
#[cfg(feature = "demo")]
#[derive(GodotClass)]
#[class(base = RefCounted, init)]
pub struct PaxDemo;

#[cfg(feature = "demo")]
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
