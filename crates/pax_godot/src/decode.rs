//! The client's protocol logic, as plain Rust values: no Godot types, so all of it is
//! unit-tested without a running engine. `lib.rs` only turns these values into
//! Godot ones.
//!
//! [`ServerStream`] enforces D22 on the client side:
//! * any protocol error is fatal and permanent;
//! * `Welcome` must come first, with a compatible protocol version and consistent
//!   tables;
//! * every later message must agree with those tables.
//!
//! None of this is left to GDScript.

use pax_protocol::{Direction, FrameDecoder, PROTOCOL_MAJOR, ProtocolError, read_server_message, wire};

/// A `Fixed` raw value (value × 10⁶) as a display float. This is the bridge's only
/// float arithmetic. It is presentation (D3), and the lint stays on everywhere else,
/// so code that builds commands (simulation input) can never use floats.
#[allow(clippy::float_arithmetic)]
pub fn display(fixed: wire::Fixed) -> f32 {
    (fixed.raw() as f64 / 1_000_000.0) as f32
}

/// Why the server's stream can't be used. Every case ends the session (D22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    /// Framing, identifier or verification failure.
    Protocol(String),
    /// The server speaks another major version.
    IncompatibleVersion { server_major: u16 },
    /// A message broke the session's rules, or contradicts the `Welcome` tables.
    Invalid(String),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Protocol(e) => write!(f, "protocol error: {e}"),
            StreamError::IncompatibleVersion { server_major } => {
                write!(f, "the server speaks protocol {server_major}.x; this client speaks {PROTOCOL_MAJOR}.x")
            }
            StreamError::Invalid(e) => write!(f, "invalid message: {e}"),
        }
    }
}

impl From<ProtocolError> for StreamError {
    fn from(e: ProtocolError) -> Self {
        StreamError::Protocol(e.to_string())
    }
}

/// The parts of `Welcome` the client needs to lay out the map and labels. Every table
/// is required: ids elsewhere index into them (D22).
#[derive(Debug, Clone, PartialEq)]
pub struct WelcomeView {
    pub protocol_minor: u16,
    pub nation: Option<u32>,
    pub day: u64,
    pub scenario: String,
    pub content_hash: u64,
    pub goods: Vec<String>,
    pub professions: Vec<String>,
    pub provinces: Vec<String>,
    pub province_market: Vec<u32>,
    pub markets: Vec<String>,
    pub nations: Vec<String>,
}

/// The parts of `DayUpdate` the client draws so far: the day and the map mode.
#[derive(Debug, Clone, PartialEq)]
pub struct DayUpdateView {
    pub day: u64,
    pub skipped: u32,
    /// Present on checkpoint days only (D23).
    pub state_hash: Option<u64>,
    pub map_mode: Option<wire::MapMode>,
    /// One display value per province, in `Welcome` order (checked).
    pub map_values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    Welcome(WelcomeView),
    DayUpdate(DayUpdateView),
    /// A message kind the client doesn't decode yet.
    Other(wire::ServerPayload),
}

impl ServerEvent {
    /// The tag GDScript matches on (`"welcome"`, `"day_update"`, …): the
    /// `snake_case` form of the schema's `ServerPayload` member, from one table.
    pub fn tag(&self) -> &'static str {
        payload_tag(match self {
            ServerEvent::Welcome(_) => wire::ServerPayload::Welcome,
            ServerEvent::DayUpdate(_) => wire::ServerPayload::DayUpdate,
            ServerEvent::Other(kind) => *kind,
        })
    }
}

/// `snake_case` tag for a `ServerPayload` member: the only table of message names.
fn payload_tag(kind: wire::ServerPayload) -> &'static str {
    match kind {
        wire::ServerPayload::Welcome => "welcome",
        wire::ServerPayload::Rejected => "rejected",
        wire::ServerPayload::DayUpdate => "day_update",
        wire::ServerPayload::CommandResult => "command_result",
        wire::ServerPayload::ServerState => "server_state",
        wire::ServerPayload::Pong => "pong",
        wire::ServerPayload::SaveResult => "save_result",
        wire::ServerPayload::SaveList => "save_list",
        wire::ServerPayload::Goodbye => "goodbye",
        _ => "unknown",
    }
}

fn required<T>(value: Option<T>, what: &str) -> Result<T, StreamError> {
    value.ok_or_else(|| StreamError::Invalid(format!("Welcome without {what}")))
}

fn strings(
    v: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<&str>>>,
    what: &str,
) -> Result<Vec<String>, StreamError> {
    Ok(required(v, what)?.iter().map(str::to_owned).collect())
}

fn welcome(w: wire::Welcome<'_>) -> Result<WelcomeView, StreamError> {
    if w.protocol_major() != PROTOCOL_MAJOR {
        return Err(StreamError::IncompatibleVersion { server_major: w.protocol_major() });
    }
    let defs = required(w.defs(), "StaticData")?;
    let view = WelcomeView {
        protocol_minor: w.protocol_minor(),
        nation: w.nation(),
        day: w.day(),
        scenario: required(w.scenario(), "a scenario name")?.to_owned(),
        content_hash: w.content_hash(),
        goods: strings(defs.goods(), "goods")?,
        professions: strings(defs.professions(), "professions")?,
        provinces: strings(defs.provinces(), "provinces")?,
        province_market: required(defs.province_market(), "province_market")?.iter().collect(),
        markets: strings(defs.markets(), "markets")?,
        nations: required(defs.nations(), "nations")?
            .iter()
            .map(|n| required(n.key(), "a nation key").map(str::to_owned))
            .collect::<Result<_, _>>()?,
    };
    if view.province_market.len() != view.provinces.len() {
        return Err(StreamError::Invalid(format!(
            "{} provinces but {} province_market entries",
            view.provinces.len(),
            view.province_market.len()
        )));
    }
    if let Some(m) = view.province_market.iter().find(|&&m| m as usize >= view.markets.len()) {
        return Err(StreamError::Invalid(format!(
            "province_market names market {m}, but there are {}",
            view.markets.len()
        )));
    }
    if let Some(n) = view.nation.filter(|&n| n as usize >= view.nations.len()) {
        return Err(StreamError::Invalid(format!(
            "this session's nation {n} is not in the {}-nation table",
            view.nations.len()
        )));
    }
    Ok(view)
}

fn day_update(u: wire::DayUpdate<'_>, provinces: usize) -> Result<DayUpdateView, StreamError> {
    let map = u.map();
    let map_values: Vec<f32> =
        map.and_then(|m| m.values()).map(|v| v.iter().map(|f| display(*f)).collect()).unwrap_or_default();
    // Nation mode carries no values (drawn from StaticData); every other mode has one per province.
    if let Some(m) = map
        && m.mode() != wire::MapMode::Nation
        && map_values.len() != provinces
    {
        return Err(StreamError::Invalid(format!("map has {} values for {provinces} provinces", map_values.len())));
    }
    Ok(DayUpdateView {
        day: u.day(),
        skipped: u.skipped(),
        state_hash: u.state_hash(),
        map_mode: map.map(|m| m.mode()),
        map_values,
    })
}

/// Where the session is. The `Welcome` tables are fixed for a session (D22), so the
/// only way to replace them is a reload the client asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Before the first `Welcome`: only `Welcome`, `Rejected` or `Goodbye` may arrive.
    AwaitingWelcome,
    /// After `Welcome`: every message is checked against its tables.
    Session { provinces: usize },
    /// The client sent `LoadGame` ([`ServerStream::begin_reload`]): the next
    /// `Welcome` replaces the tables (NETWORK_PROTOCOL §3).
    Reloading { provinces: usize },
}

/// The server's side of one connection, as seen by the client. Feed it bytes in any
/// chunking. It yields events until the first error, and then nothing ever again:
/// the caller must disconnect (D22).
#[derive(Debug)]
pub struct ServerStream {
    frames: FrameDecoder,
    phase: Phase,
    failed: Option<StreamError>,
}

impl Default for ServerStream {
    fn default() -> Self {
        ServerStream {
            frames: FrameDecoder::new(Direction::ServerToClient),
            phase: Phase::AwaitingWelcome,
            failed: None,
        }
    }
}

impl ServerStream {
    /// The events in `bytes` (plus anything buffered), up to the first error.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<ServerEvent> {
        let mut events = Vec::new();
        if self.failed.is_some() {
            return events;
        }
        self.frames.push(bytes);
        loop {
            let event = match self.frames.next_frame() {
                Ok(Some(frame)) => self.decode(&frame),
                Ok(None) => break,
                Err(e) => Err(StreamError::Protocol(e.to_string())),
            };
            match event {
                Ok(event) => events.push(event),
                Err(e) => {
                    self.failed = Some(e);
                    break;
                }
            }
        }
        events
    }

    /// The error that ended the stream, if any. It never clears.
    pub fn error(&self) -> Option<&StreamError> {
        self.failed.as_ref()
    }

    /// Call when sending `LoadGame`: the server answers with a new `Welcome` whose
    /// tables replace the current ones. Without this, a second `Welcome` is fatal.
    pub fn begin_reload(&mut self) {
        if let Phase::Session { provinces } = self.phase {
            self.phase = Phase::Reloading { provinces };
        }
    }

    fn decode(&mut self, frame: &[u8]) -> Result<ServerEvent, StreamError> {
        use wire::ServerPayload as P;
        let msg = read_server_message(frame)?;
        let kind = msg.payload_type();
        match (self.phase, kind) {
            // A Welcome opens a session, or replaces it after a requested reload.
            (Phase::AwaitingWelcome | Phase::Reloading { .. }, P::Welcome) => {
                let view = welcome(required(msg.payload_as_welcome(), "a body")?)?;
                self.phase = Phase::Session { provinces: view.provinces.len() };
                Ok(ServerEvent::Welcome(view))
            }
            (Phase::Session { .. }, P::Welcome) => {
                Err(StreamError::Invalid("a second Welcome without a LoadGame".to_owned()))
            }
            // Before the first Welcome, only a refusal can explain why there will be none.
            (Phase::AwaitingWelcome, P::Rejected | P::Goodbye) => Ok(ServerEvent::Other(kind)),
            (Phase::AwaitingWelcome, _) => Err(StreamError::Invalid(format!("{} before Welcome", payload_tag(kind)))),
            (Phase::Session { provinces } | Phase::Reloading { provinces }, P::DayUpdate) => {
                let update = msg
                    .payload_as_day_update()
                    .ok_or_else(|| StreamError::Invalid("DayUpdate without a body".to_owned()))?;
                Ok(ServerEvent::DayUpdate(day_update(update, provinces)?))
            }
            (Phase::Session { .. } | Phase::Reloading { .. }, _) => Ok(ServerEvent::Other(kind)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use flatbuffers::FlatBufferBuilder;

    fn server_frame(
        b: &mut FlatBufferBuilder<'_>,
        kind: wire::ServerPayload,
        payload: flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>,
    ) -> Vec<u8> {
        let msg =
            wire::ServerMessage::create(b, &wire::ServerMessageArgs { payload_type: kind, payload: Some(payload) });
        wire::finish_size_prefixed_server_message_buffer(b, msg);
        b.finished_data().to_vec()
    }

    fn pong() -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();
        let p = wire::Pong::create(&mut b, &wire::PongArgs { nonce: 1 });
        server_frame(&mut b, wire::ServerPayload::Pong, p.as_union_value())
    }

    /// A Welcome with `provinces` provinces whose province_market has `markets_listed` entries.
    fn welcome_frame(major: u16, provinces: usize, markets_listed: usize, with_defs: bool) -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();
        let names: Vec<_> = (0..provinces).map(|p| b.create_string(&format!("p{p}"))).collect();
        let names = b.create_vector(&names);
        let province_market = b.create_vector(&vec![0u32; markets_listed]);
        let market = b.create_string("m0");
        let markets = b.create_vector(&[market]);
        let empty_strings = b.create_vector::<flatbuffers::WIPOffset<&str>>(&[]);
        let nations = b.create_vector::<flatbuffers::WIPOffset<wire::NationDef>>(&[]);
        let defs = with_defs.then(|| {
            wire::StaticData::create(
                &mut b,
                &wire::StaticDataArgs {
                    goods: Some(empty_strings),
                    professions: Some(empty_strings),
                    producer_types: Some(empty_strings),
                    provinces: Some(names),
                    province_market: Some(province_market),
                    markets: Some(markets),
                    nations: Some(nations),
                },
            )
        });
        let scenario = b.create_string("t");
        let w = wire::Welcome::create(
            &mut b,
            &wire::WelcomeArgs { protocol_major: major, scenario: Some(scenario), defs, ..Default::default() },
        );
        server_frame(&mut b, wire::ServerPayload::Welcome, w.as_union_value())
    }

    #[test]
    fn decodes_the_demo_session() {
        let mut s = ServerStream::default();
        let events = s.push(&demo::frames(12));
        assert_eq!(s.error(), None);
        let [ServerEvent::Welcome(w), ServerEvent::DayUpdate(u)] = events.as_slice() else { panic!("got {events:?}") };
        assert_eq!((w.provinces.len(), w.province_market.len(), w.nations.len()), (12, 12, 2));
        assert_eq!((u.map_mode, u.map_values.len()), (Some(wire::MapMode::Population), 12));
        assert!(u.map_values.iter().all(|v| v.is_finite() && *v > 0.0));
    }

    #[test]
    fn any_chunking_gives_the_same_events() {
        let bytes = demo::frames(30);
        let whole = ServerStream::default().push(&bytes);
        let mut s = ServerStream::default();
        let mut chunked = Vec::new();
        for chunk in bytes.chunks(7) {
            chunked.extend(s.push(chunk));
        }
        assert_eq!(chunked, whole);
    }

    #[test]
    fn errors_latch_and_later_frames_are_never_decoded() {
        let mut bytes = demo::frames(5);
        let corrupt_at = bytes.len();
        bytes.extend(pong());
        bytes[corrupt_at + 8..corrupt_at + 12].copy_from_slice(b"PAXC"); // wrong identifier
        bytes.extend(pong()); // valid, but after the error
        let mut s = ServerStream::default();
        let events = s.push(&bytes);
        assert_eq!(events.len(), 2, "Welcome and DayUpdate decode; nothing after the corrupt frame");
        assert!(matches!(s.error(), Some(StreamError::Protocol(_))));
        // Valid input after the error is ignored for good.
        assert!(s.push(&pong()).is_empty());
        assert!(s.error().is_some());
    }

    #[test]
    fn a_welcome_without_tables_or_with_mismatched_tables_is_fatal() {
        let mut s = ServerStream::default();
        assert!(s.push(&welcome_frame(PROTOCOL_MAJOR, 3, 3, false)).is_empty());
        assert_eq!(s.error(), Some(&StreamError::Invalid("Welcome without StaticData".into())));

        let mut s = ServerStream::default();
        assert!(s.push(&welcome_frame(PROTOCOL_MAJOR, 3, 2, true)).is_empty());
        assert!(matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("3 provinces but 2")));
    }

    #[test]
    fn an_incompatible_major_version_is_fatal() {
        let mut s = ServerStream::default();
        assert!(s.push(&welcome_frame(PROTOCOL_MAJOR + 1, 1, 1, true)).is_empty());
        assert_eq!(s.error(), Some(&StreamError::IncompatibleVersion { server_major: PROTOCOL_MAJOR + 1 }));
    }

    #[test]
    fn a_second_welcome_is_fatal_unless_a_reload_was_requested() {
        let mut s = ServerStream::default();
        let mut bytes = welcome_frame(PROTOCOL_MAJOR, 2, 2, true);
        bytes.extend(welcome_frame(PROTOCOL_MAJOR, 3, 3, true));
        assert_eq!(s.push(&bytes).len(), 1);
        assert!(matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("second Welcome")));

        // After begin_reload, the new Welcome's tables replace the old ones.
        let mut s = ServerStream::default();
        assert_eq!(s.push(&welcome_frame(PROTOCOL_MAJOR, 2, 2, true)).len(), 1);
        s.begin_reload();
        assert_eq!(s.push(&welcome_frame(PROTOCOL_MAJOR, 5, 5, true)).len(), 1);
        let update = &demo::frames(5)[demo::welcome_len(5)..]; // 5 map values: matches the new tables
        assert_eq!(s.push(update).len(), 1);
        assert_eq!(s.error(), None);
    }

    #[test]
    fn nothing_but_rejected_or_goodbye_may_precede_welcome() {
        let mut s = ServerStream::default();
        assert!(s.push(&pong()).is_empty());
        assert!(matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("pong before Welcome")));
    }

    #[test]
    fn a_map_with_the_wrong_province_count_is_fatal() {
        let mut s = ServerStream::default();
        let mut bytes = welcome_frame(PROTOCOL_MAJOR, 4, 4, true);
        bytes.extend(&demo::frames(5)[demo::welcome_len(5)..]); // a DayUpdate with 5 map values
        let events = s.push(&bytes);
        assert_eq!(events.len(), 1);
        assert!(matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("5 values for 4 provinces")));
    }

    #[test]
    fn tags_are_snake_case_for_every_message() {
        let mut s = ServerStream::default();
        let mut bytes = demo::frames(2);
        bytes.extend(pong());
        let tags: Vec<_> = s.push(&bytes).iter().map(ServerEvent::tag).collect();
        assert_eq!(tags, ["welcome", "day_update", "pong"]);
    }

    #[test]
    fn display_converts_fixed_raw_values() {
        assert_eq!(display(wire::Fixed::new(1_500_000)), 1.5);
        assert_eq!(display(wire::Fixed::new(-250_000)), -0.25);
    }
}
