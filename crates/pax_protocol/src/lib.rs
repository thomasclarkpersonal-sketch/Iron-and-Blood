//! The wire protocol between `pax_server` and its clients (D22,
//! `docs/NETWORK_PROTOCOL.md`).
//!
//! * [`wire`]: the FlatBuffers types, generated from `schemas/*.fbs` by
//!   `scripts/gen-protocol.sh` and checked in. CI regenerates them and fails on any
//!   difference, so the schemas and this code can't drift apart.
//! * [`frame`]: size-prefixed framing with a size limit per direction, as a decoder
//!   that does no IO. The tokio server and the Godot bridge feed it bytes from their
//!   own sockets.
//! * [`read_client_message`] and [`read_server_message`]: check a frame's file
//!   identifier, then run the FlatBuffers verifier, before anything reads it.
//!
//! The crate never depends on `pax_engine`: clients link it, and a client must not be
//! able to simulate (D10, D12). Conversions between engine and wire types live in
//! `pax_server`, the only crate that sees both.
//!
//! Simulation values travel as [`wire::Fixed`], whose `raw` is `pax_engine::Fixed`'s
//! raw `i64` (value × 10⁶, D3). They are never floats on the wire.

// Generated code: excluded from formatting and from the lints that hand-written code
// must pass. `unsafe` is allowed here only; see this crate's Cargo.toml.
#[allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    clippy::all,
    clippy::pedantic,
    dead_code,
    unused_imports,
    unused_lifetimes,
    non_camel_case_types,
    non_snake_case,
    mismatched_lifetime_syntaxes,
    missing_docs,
    rustdoc::all
)]
#[rustfmt::skip]
mod generated;

pub mod frame;

pub use frame::{FrameDecoder, FrameError};
pub use generated::pax::net as wire;

/// Protocol major version. A different major version is refused at `Hello` (D22).
pub const PROTOCOL_MAJOR: u16 = 1;
/// Protocol minor version: bumped for compatible additions (NETWORK_PROTOCOL §8).
pub const PROTOCOL_MINOR: u16 = 0;

/// Largest client→server frame body, in bytes. Client messages are commands and
/// controls, so 64 KiB is generous; a bigger frame is a protocol error (D22).
pub const MAX_CLIENT_FRAME: usize = 64 * 1024;
/// Largest server→client frame body, in bytes (D22). The largest real message, a
/// `DayUpdate` with every view, is budgeted at 128 KiB.
pub const MAX_SERVER_FRAME: usize = 16 * 1024 * 1024;

/// Why an inbound frame can't be read. Every variant ends the session (D22): the
/// receiver never guesses at a malformed message.
#[derive(Debug)]
pub enum ProtocolError {
    /// The frame's 4-byte file identifier is not the one for this direction
    /// (`PAXC` for client messages, `PAXS` for server messages).
    WrongIdentifier { expected: &'static str },
    /// The frame failed the FlatBuffers verifier. The verifier guarantees memory-safe
    /// reads; it does not detect corrupted values (TCP's job).
    Invalid(flatbuffers::InvalidFlatbuffer),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::WrongIdentifier { expected } => write!(f, "frame is not a {expected} message"),
            ProtocolError::Invalid(e) => write!(f, "frame failed verification: {e}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Bytes before the root table in a size-prefixed buffer: the length prefix, the root
/// offset and the file identifier.
const HEADER_LEN: usize = 4 + 4 + 4;

/// Whether `frame` (as produced by [`FrameDecoder`], length prefix included) carries
/// `identifier`. Checks the length first, because flatbuffers' own check asserts on
/// short input, and a hostile peer controls frame length.
fn has_identifier(frame: &[u8], identifier: &str) -> bool {
    frame.len() >= HEADER_LEN && &frame[8..12] == identifier.as_bytes()
}

/// Reads a client→server frame: identifier `PAXC`, then the verifier.
pub fn read_client_message(frame: &[u8]) -> Result<wire::ClientMessage<'_>, ProtocolError> {
    if !has_identifier(frame, wire::CLIENT_MESSAGE_IDENTIFIER) {
        return Err(ProtocolError::WrongIdentifier { expected: wire::CLIENT_MESSAGE_IDENTIFIER });
    }
    wire::size_prefixed_root_as_client_message(frame).map_err(ProtocolError::Invalid)
}

/// Reads a server→client frame: identifier `PAXS`, then the verifier.
pub fn read_server_message(frame: &[u8]) -> Result<wire::ServerMessage<'_>, ProtocolError> {
    if !has_identifier(frame, wire::SERVER_MESSAGE_IDENTIFIER) {
        return Err(ProtocolError::WrongIdentifier { expected: wire::SERVER_MESSAGE_IDENTIFIER });
    }
    wire::size_prefixed_root_as_server_message(frame).map_err(ProtocolError::Invalid)
}
