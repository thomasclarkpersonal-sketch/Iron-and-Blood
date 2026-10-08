//! Client messages, verified and copied into owned values (D22).
//!
//! Decoding happens in the connection's network task, so the sim thread only ever
//! sees well-formed requests. The FlatBuffers views borrow the frame; these don't.

use pax_protocol::{ProtocolError, read_client_message, wire};

/// One engine command as it arrived on the wire, before `World::validate` (D21).
/// `None` fields were absent on the wire: the server answers `Malformed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// The names mirror `pax_engine::Command` and the schema on purpose.
#[allow(clippy::enum_variant_names)]
pub enum WireCommand {
    SetIncomeTax { nation: u32, rate_raw: Option<i64> },
    SetTransferRate { nation: u32, rate_raw: Option<i64> },
    SetConsumptionRate { nation: u32, rate_raw: Option<i64> },
}

/// A verified client message (`ClientPayload`), owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// `name` is display-only; absent stays absent.
    Hello {
        major: u16,
        minor: u16,
        name: Option<String>,
        requested_nation: Option<u32>,
        resume_token: u64,
    },
    /// `command` is `None` when the union member is missing or unknown to this version.
    SubmitCommand {
        client_seq: u32,
        command: Option<WireCommand>,
    },
    SetSpeed {
        speed: wire::Speed,
    },
    Subscribe {
        map_mode: wire::MapMode,
        map_good: u16,
        market: Option<u32>,
        province: Option<u32>,
    },
    Ack {
        day: u64,
    },
    Ping {
        nonce: u64,
    },
    /// An absent name stays `None`; the save handler rejects it (M3-6), never a default.
    SaveGame {
        name: Option<String>,
    },
    LoadGame {
        name: Option<String>,
    },
    ListSaves,
    /// End another player's session (host only, D24).
    Kick {
        player: u16,
    },
}

/// Why a frame isn't a usable request. Fatal to the session (D22).
#[derive(Debug)]
pub enum RequestError {
    Protocol(ProtocolError),
    /// The message has no payload, or a payload type this server doesn't know.
    UnknownPayload,
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Protocol(e) => write!(f, "{e}"),
            RequestError::UnknownPayload => write!(f, "message without a known payload"),
        }
    }
}

fn rate(r: Option<&wire::Fixed>) -> Option<i64> {
    r.map(|f| f.raw())
}

fn command(s: &wire::SubmitCommand<'_>) -> Option<WireCommand> {
    // flatbuffers' Rust unions are open newtypes, so this match can't be exhaustive:
    // NONE and members newer than this build fall to `_` and become "no command",
    // which the server answers with `Malformed`. The exhaustive matches that
    // NETWORK_PROTOCOL §5 requires are over the engine's enums (`commands.rs`).
    match s.command_type() {
        wire::Command::SetIncomeTax => s
            .command_as_set_income_tax()
            .map(|c| WireCommand::SetIncomeTax { nation: c.nation(), rate_raw: rate(c.rate()) }),
        wire::Command::SetTransferRate => s
            .command_as_set_transfer_rate()
            .map(|c| WireCommand::SetTransferRate { nation: c.nation(), rate_raw: rate(c.rate()) }),
        wire::Command::SetConsumptionRate => s
            .command_as_set_consumption_rate()
            .map(|c| WireCommand::SetConsumptionRate { nation: c.nation(), rate_raw: rate(c.rate()) }),
        _ => None,
    }
}

/// Verifies `frame` (length prefix included) and copies it into a [`Request`].
pub fn decode(frame: &[u8]) -> Result<Request, RequestError> {
    let msg = read_client_message(frame).map_err(RequestError::Protocol)?;
    // Absent strings stay absent: whoever owns a field decides whether that's an error.
    let owned = |s: Option<&str>| s.map(str::to_owned);
    use wire::ClientPayload as P;
    let request = match msg.payload_type() {
        P::Hello => msg.payload_as_hello().map(|h| Request::Hello {
            major: h.protocol_major(),
            minor: h.protocol_minor(),
            name: owned(h.client_name()),
            requested_nation: h.requested_nation(),
            resume_token: h.resume_token(),
        }),
        P::SubmitCommand => msg
            .payload_as_submit_command()
            .map(|s| Request::SubmitCommand { client_seq: s.client_seq(), command: command(&s) }),
        P::SetSpeed => msg.payload_as_set_speed().map(|s| Request::SetSpeed { speed: s.speed() }),
        P::Subscribe => msg.payload_as_subscribe().map(|s| Request::Subscribe {
            map_mode: s.map_mode(),
            map_good: s.map_good(),
            market: s.market(),
            province: s.province(),
        }),
        P::Ack => msg.payload_as_ack().map(|a| Request::Ack { day: a.day() }),
        P::Ping => msg.payload_as_ping().map(|p| Request::Ping { nonce: p.nonce() }),
        P::SaveGame => msg.payload_as_save_game().map(|s| Request::SaveGame { name: owned(s.name()) }),
        P::LoadGame => msg.payload_as_load_game().map(|s| Request::LoadGame { name: owned(s.name()) }),
        P::ListSaves => msg.payload_as_list_saves().map(|_| Request::ListSaves),
        P::Kick => msg.payload_as_kick().map(|k| Request::Kick { player: k.player() }),
        _ => None,
    };
    request.ok_or(RequestError::UnknownPayload)
}
