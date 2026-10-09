//! The client's protocol logic, as plain Rust values: no Godot types, so all of it is
//! unit-tested without a running engine. `lib.rs` only turns these values into
//! Godot ones.
//!
//! [`ServerStream`] enforces D22 on the client side:
//! * any protocol error is fatal and permanent;
//! * `Welcome` must come first, with a compatible protocol version and consistent
//!   tables;
//! * every later message must agree with those tables: every id is in range and
//!   every column has the length its table says.
//!
//! None of this is left to GDScript.

use pax_protocol::{Direction, FIXED_ONE, FrameDecoder, PROTOCOL_MAJOR, ProtocolError, read_server_message, wire};

use crate::keys;

/// A `Fixed` raw value (value × 10⁶) as a display number. This is the bridge's only
/// float arithmetic. It is presentation (D3), and the lint stays on everywhere else,
/// so code that builds commands (simulation input) can never use floats.
#[allow(clippy::float_arithmetic)]
pub fn display(fixed: wire::Fixed) -> f64 {
    fixed.raw() as f64 / FIXED_ONE as f64
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

fn invalid(message: String) -> StreamError {
    StreamError::Invalid(message)
}

/// The session's tables, from `Welcome`. Every id elsewhere indexes into them (D22).
#[derive(Debug, Clone, PartialEq)]
pub struct WelcomeView {
    pub protocol_minor: u16,
    pub player: u16,
    /// Present in a later `Hello` to reclaim this seat after a drop (D24).
    pub resume_token: u64,
    pub nation: Option<u32>,
    pub day: u64,
    pub speed: wire::Speed,
    pub scenario: String,
    pub content_hash: u64,
    /// The hash the client's copy of the map files must have (`StaticData.map_hash`);
    /// `None` if the scenario has no map.
    pub map_hash: Option<u64>,
    /// Where the map's files are, relative to the scenario (`StaticData.map_dir`, 1.1).
    pub map_dir: Option<String>,
    pub goods: Vec<String>,
    pub professions: Vec<String>,
    pub producer_types: Vec<String>,
    pub provinces: Vec<String>,
    pub province_market: Vec<u32>,
    pub markets: Vec<String>,
    pub nations: Vec<String>,
    /// The markets each nation owns, by nation.
    pub nation_markets: Vec<Vec<u32>>,
}

/// Table sizes a session's messages are checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tables {
    goods: usize,
    professions: usize,
    producer_types: usize,
    provinces: usize,
    markets: usize,
    nations: usize,
}

impl Tables {
    fn of(w: &WelcomeView) -> Tables {
        Tables {
            goods: w.goods.len(),
            professions: w.professions.len(),
            producer_types: w.producer_types.len(),
            provinces: w.provinces.len(),
            markets: w.markets.len(),
            nations: w.nations.len(),
        }
    }
}

/// `WorldSummary`: the whole world's figures for the day. Money in currency units.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldSummaryView {
    pub population: u64,
    pub workforce: u64,
    pub unemployed: u64,
    pub household_spending: f64,
    pub government_spending: f64,
    pub input_spending: f64,
    pub wages: f64,
    pub dividends: f64,
    pub taxes: f64,
    pub transfers: f64,
    pub deprived: u64,
    pub life_needs: f64,
    pub militancy: f64,
}

/// `NationTable`, one entry per nation. Rates stay raw `Fixed` integers: a policy
/// slider edits them and sends them back as commands, so they never pass through a
/// float (D3).
#[derive(Debug, Clone, PartialEq)]
pub struct NationTableView {
    pub treasury: Vec<f64>,
    pub income_tax_rate_raw: Vec<i64>,
    pub transfer_rate_raw: Vec<i64>,
    pub consumption_rate_raw: Vec<i64>,
    pub population: Vec<u64>,
    /// Mean militancy per nation (protocol 1.2). `None` from an older server, which
    /// doesn't send it: an absent addition means no data (NETWORK_PROTOCOL §8).
    pub militancy: Option<Vec<f64>>,
}

/// `MapView`: one value per province, except in `Nation` mode, which has none.
#[derive(Debug, Clone, PartialEq)]
pub struct MapViewData {
    pub mode: wire::MapMode,
    pub good: u16,
    pub values: Vec<f64>,
}

/// `MarketDetail`: one entry per good.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketView {
    pub market: u32,
    pub price: Vec<f64>,
    pub supply: Vec<f64>,
    pub demand: Vec<f64>,
    pub traded: Vec<f64>,
}

/// `ProvinceDetail`: the province's POPs by profession, labour pools and producers.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvinceView {
    pub province: u32,
    pub pop_profession: Vec<u16>,
    pub pop_people: Vec<u32>,
    pub pop_cash: Vec<f64>,
    pub pop_life_needs: Vec<f64>,
    pub pop_militancy: Vec<f64>,
    pub labour_profession: Vec<u16>,
    pub labour_workforce: Vec<u64>,
    pub labour_jobs: Vec<u64>,
    pub labour_employed: Vec<u64>,
    pub producer_type: Vec<u16>,
    pub producer_capacity: Vec<u32>,
    pub producer_employed: Vec<u32>,
    pub producer_wage: Vec<f64>,
    pub producer_cash: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DayUpdateView {
    pub day: u64,
    pub speed: wire::Speed,
    pub skipped: u32,
    pub state_hash: u64,
    pub world: WorldSummaryView,
    pub nations: NationTableView,
    pub map: Option<MapViewData>,
    pub market: Option<MarketView>,
    pub province: Option<ProvinceView>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    Welcome(Box<WelcomeView>),
    Rejected {
        reason: String,
    },
    DayUpdate(Box<DayUpdateView>),
    CommandResult {
        client_seq: u32,
        error: wire::CommandError,
        applies_on_day: u64,
    },
    ServerState {
        day: u64,
        speed: wire::Speed,
        changed_by: u16,
        /// Players a fairness pause waits for (D24, protocol 1.5); empty when none,
        /// and from an older server.
        waiting_for: Vec<u16>,
    },
    Pong {
        nonce: u64,
    },
    SaveResult {
        /// The request it answers: a `SaveGame`, or a `LoadGame` that failed.
        request: SaveRequest,
        name: String,
        error: String,
    },
    SaveList {
        names: Vec<String>,
    },
    Goodbye {
        reason: String,
    },
    /// The lobby of a multiplayer server (D24, protocol 1.4).
    LobbyState {
        players: Vec<LobbyPlayerView>,
        started: bool,
        /// Why this client's last lobby request was refused.
        notice: Option<String>,
    },
    /// A message kind newer than this client: ignored (D22).
    Unknown(wire::ServerPayload),
}

/// One player in the lobby (`LobbyPlayer`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyPlayerView {
    pub player: u16,
    pub name: String,
    /// Checked against the session's nation table.
    pub nation: Option<u32>,
    pub sandbox: bool,
    pub ready: bool,
    pub host: bool,
    /// Left the started game; the seat waits for their resume token (protocol 1.5).
    pub away: bool,
}

impl ServerEvent {
    /// The tag GDScript matches on (`"welcome"`, `"day_update"`, …): the
    /// `snake_case` form of the schema's `ServerPayload` member, from one table.
    pub fn tag(&self) -> &'static str {
        use wire::ServerPayload as P;
        payload_tag(match self {
            ServerEvent::Welcome(_) => P::Welcome,
            ServerEvent::Rejected { .. } => P::Rejected,
            ServerEvent::DayUpdate(_) => P::DayUpdate,
            ServerEvent::CommandResult { .. } => P::CommandResult,
            ServerEvent::ServerState { .. } => P::ServerState,
            ServerEvent::Pong { .. } => P::Pong,
            ServerEvent::SaveResult { .. } => P::SaveResult,
            ServerEvent::SaveList { .. } => P::SaveList,
            ServerEvent::Goodbye { .. } => P::Goodbye,
            ServerEvent::LobbyState { .. } => P::LobbyState,
            ServerEvent::Unknown(kind) => *kind,
        })
    }
}

/// The tag for a `ServerPayload` member: the `keys` constant GDScript matches on, so
/// the two can't disagree.
fn payload_tag(kind: wire::ServerPayload) -> &'static str {
    match kind {
        wire::ServerPayload::Welcome => keys::WELCOME,
        wire::ServerPayload::Rejected => keys::REJECTED,
        wire::ServerPayload::DayUpdate => keys::DAY_UPDATE,
        wire::ServerPayload::CommandResult => keys::COMMAND_RESULT,
        wire::ServerPayload::ServerState => keys::SERVER_STATE,
        wire::ServerPayload::Pong => keys::PONG,
        wire::ServerPayload::SaveResult => keys::SAVE_RESULT,
        wire::ServerPayload::SaveList => keys::SAVE_LIST,
        wire::ServerPayload::Goodbye => keys::GOODBYE,
        wire::ServerPayload::LobbyState => keys::LOBBY_STATE,
        _ => keys::UNKNOWN,
    }
}

fn required<T>(value: Option<T>, what: &str) -> Result<T, StreamError> {
    value.ok_or_else(|| invalid(format!("missing {what}")))
}

type StrVector<'a> = flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<&'a str>>;

fn strings(v: Option<StrVector<'_>>, what: &str) -> Result<Vec<String>, StreamError> {
    Ok(required(v, what)?.iter().map(str::to_owned).collect())
}

/// A required column of `len` values.
fn column<'a, T: flatbuffers::Follow<'a> + 'a, U>(
    v: Option<flatbuffers::Vector<'a, T>>,
    len: usize,
    what: &str,
    f: impl Fn(T::Inner) -> U,
) -> Result<Vec<U>, StreamError> {
    let v = required(v, what)?;
    if v.len() != len {
        return Err(invalid(format!("{what} has {} entries, expected {len}", v.len())));
    }
    Ok(v.iter().map(f).collect())
}

fn fixeds<'a>(
    v: Option<flatbuffers::Vector<'a, wire::Fixed>>,
    len: usize,
    what: &str,
) -> Result<Vec<f64>, StreamError> {
    column(v, len, what, |f: &wire::Fixed| display(*f))
}

fn raws<'a>(v: Option<flatbuffers::Vector<'a, wire::Fixed>>, len: usize, what: &str) -> Result<Vec<i64>, StreamError> {
    column(v, len, what, |f: &wire::Fixed| f.raw())
}

/// An id that must index a table of `len` entries.
fn id_in<T: Copy + Into<u64>>(ids: &[T], len: usize, what: &str) -> Result<(), StreamError> {
    match ids.iter().map(|&i| i.into()).find(|&i| i >= len as u64) {
        Some(i) => Err(invalid(format!("{what} {i} is not in the {len}-entry table"))),
        None => Ok(()),
    }
}

fn welcome(w: wire::Welcome<'_>) -> Result<WelcomeView, StreamError> {
    if w.protocol_major() != PROTOCOL_MAJOR {
        return Err(StreamError::IncompatibleVersion { server_major: w.protocol_major() });
    }
    let defs = required(w.defs(), "Welcome StaticData")?;
    let nation_defs = required(defs.nations(), "Welcome nations")?;
    let view = WelcomeView {
        protocol_minor: w.protocol_minor(),
        player: w.player(),
        resume_token: w.resume_token(),
        nation: w.nation(),
        day: w.day(),
        speed: w.speed(),
        scenario: required(w.scenario(), "Welcome scenario name")?.to_owned(),
        content_hash: w.content_hash(),
        map_hash: defs.map_hash(),
        map_dir: defs.map_dir().map(str::to_owned),
        goods: strings(defs.goods(), "Welcome goods")?,
        professions: strings(defs.professions(), "Welcome professions")?,
        producer_types: strings(defs.producer_types(), "Welcome producer_types")?,
        provinces: strings(defs.provinces(), "Welcome provinces")?,
        province_market: required(defs.province_market(), "Welcome province_market")?.iter().collect(),
        markets: strings(defs.markets(), "Welcome markets")?,
        nations: nation_defs
            .iter()
            .map(|n| required(n.key(), "a nation key").map(str::to_owned))
            .collect::<Result<_, _>>()?,
        nation_markets: nation_defs
            .iter()
            .map(|n| required(n.markets(), "a nation's markets").map(|m| m.iter().collect()))
            .collect::<Result<_, _>>()?,
    };
    // From protocol 1.1 the two travel together. A 1.0 server sends a hash without
    // the directory: the client then has no map to draw, which is not an error.
    if view.map_dir.is_some() && view.map_hash.is_none() {
        return Err(invalid("StaticData has a map_dir without a map_hash".to_owned()));
    }
    // A server can't point the client outside its scenario directory.
    if let Some(dir) = &view.map_dir {
        pax_map::check_map_dir(dir).map_err(invalid)?;
    }
    if view.province_market.len() != view.provinces.len() {
        return Err(invalid(format!(
            "{} provinces but {} province_market entries",
            view.provinces.len(),
            view.province_market.len()
        )));
    }
    id_in(&view.province_market, view.markets.len(), "province_market's market")?;
    for markets in &view.nation_markets {
        id_in(markets, view.markets.len(), "a nation's market")?;
    }
    if let Some(n) = view.nation {
        id_in(&[n], view.nations.len(), "this session's nation")?;
    }
    Ok(view)
}

fn world_summary(w: wire::WorldSummary<'_>) -> Result<WorldSummaryView, StreamError> {
    // Every Fixed field is required, like every column elsewhere.
    let fixed = |f: Option<&wire::Fixed>, what: &str| required(f, what).map(|f| display(*f));
    Ok(WorldSummaryView {
        population: w.population(),
        workforce: w.workforce(),
        unemployed: w.unemployed(),
        household_spending: fixed(w.household_spending(), "WorldSummary household_spending")?,
        government_spending: fixed(w.government_spending(), "WorldSummary government_spending")?,
        input_spending: fixed(w.input_spending(), "WorldSummary input_spending")?,
        wages: fixed(w.wages(), "WorldSummary wages")?,
        dividends: fixed(w.dividends(), "WorldSummary dividends")?,
        taxes: fixed(w.taxes(), "WorldSummary taxes")?,
        transfers: fixed(w.transfers(), "WorldSummary transfers")?,
        deprived: w.deprived(),
        life_needs: fixed(w.life_needs(), "WorldSummary life_needs")?,
        militancy: fixed(w.militancy(), "WorldSummary militancy")?,
    })
}

fn nation_table(n: wire::NationTable<'_>, t: Tables) -> Result<NationTableView, StreamError> {
    Ok(NationTableView {
        treasury: fixeds(n.treasury(), t.nations, "NationTable treasury")?,
        income_tax_rate_raw: raws(n.income_tax_rate(), t.nations, "NationTable income_tax_rate")?,
        transfer_rate_raw: raws(n.transfer_rate(), t.nations, "NationTable transfer_rate")?,
        consumption_rate_raw: raws(n.consumption_rate(), t.nations, "NationTable consumption_rate")?,
        population: column(n.population(), t.nations, "NationTable population", |p| p)?,
        militancy: n.militancy().map(|m| fixeds(Some(m), t.nations, "NationTable militancy")).transpose()?,
    })
}

fn map_view(m: wire::MapView<'_>, t: Tables) -> Result<MapViewData, StreamError> {
    let mode = m.mode();
    // Nation mode carries no values (drawn from StaticData); every other mode has one per province.
    let values = match mode {
        wire::MapMode::Nation | wire::MapMode::None => Vec::new(),
        _ => fixeds(m.values(), t.provinces, "MapView values")?,
    };
    if mode == wire::MapMode::Price {
        id_in(&[m.good()], t.goods, "MapView good")?;
    }
    Ok(MapViewData { mode, good: m.good(), values })
}

fn market_view(m: wire::MarketDetail<'_>, t: Tables) -> Result<MarketView, StreamError> {
    id_in(&[m.market()], t.markets, "MarketDetail market")?;
    Ok(MarketView {
        market: m.market(),
        price: fixeds(m.price(), t.goods, "MarketDetail price")?,
        supply: fixeds(m.supply(), t.goods, "MarketDetail supply")?,
        demand: fixeds(m.demand(), t.goods, "MarketDetail demand")?,
        traded: fixeds(m.traded(), t.goods, "MarketDetail traded")?,
    })
}

fn province_view(p: wire::ProvinceDetail<'_>, t: Tables) -> Result<ProvinceView, StreamError> {
    id_in(&[p.province()], t.provinces, "ProvinceDetail province")?;
    let pops = required(p.pops(), "ProvinceDetail pops")?;
    let labour = required(p.labour(), "ProvinceDetail labour")?;
    let producers = required(p.producers(), "ProvinceDetail producers")?;
    let n = required(pops.profession(), "PopRows profession")?.len();
    let l = required(labour.profession(), "LabourRows profession")?.len();
    let f = required(producers.producer_type(), "ProducerRows producer_type")?.len();
    let view = ProvinceView {
        province: p.province(),
        pop_profession: column(pops.profession(), n, "PopRows profession", |c| c)?,
        pop_people: column(pops.people(), n, "PopRows people", |c| c)?,
        pop_cash: fixeds(pops.cash(), n, "PopRows cash")?,
        pop_life_needs: fixeds(pops.life_needs(), n, "PopRows life_needs")?,
        pop_militancy: fixeds(pops.militancy(), n, "PopRows militancy")?,
        labour_profession: column(labour.profession(), l, "LabourRows profession", |c| c)?,
        labour_workforce: column(labour.workforce(), l, "LabourRows workforce", |c| c)?,
        labour_jobs: column(labour.jobs(), l, "LabourRows jobs", |c| c)?,
        labour_employed: column(labour.employed(), l, "LabourRows employed", |c| c)?,
        producer_type: column(producers.producer_type(), f, "ProducerRows producer_type", |c| c)?,
        producer_capacity: column(producers.capacity(), f, "ProducerRows capacity", |c| c)?,
        producer_employed: column(producers.employed(), f, "ProducerRows employed", |c| c)?,
        producer_wage: fixeds(producers.wage(), f, "ProducerRows wage")?,
        producer_cash: fixeds(producers.cash(), f, "ProducerRows cash")?,
    };
    id_in(&view.pop_profession, t.professions, "a POP's profession")?;
    id_in(&view.labour_profession, t.professions, "a labour pool's profession")?;
    id_in(&view.producer_type, t.producer_types, "a producer's type")?;
    Ok(view)
}

fn day_update(u: wire::DayUpdate<'_>, t: Tables) -> Result<DayUpdateView, StreamError> {
    Ok(DayUpdateView {
        day: u.day(),
        speed: u.speed(),
        skipped: u.skipped(),
        state_hash: u.state_hash(),
        world: world_summary(required(u.world(), "DayUpdate WorldSummary")?)?,
        nations: nation_table(required(u.nations(), "DayUpdate NationTable")?, t)?,
        map: u.map().map(|m| map_view(m, t)).transpose()?,
        market: u.market().map(|m| market_view(m, t)).transpose()?,
        province: u.province().map(|p| province_view(p, t)).transpose()?,
    })
}

/// Where the session is. The `Welcome` tables are fixed for a session (D22), so the
/// only way to replace them is a load the client asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Before the first `Welcome`: only `Welcome`, `Rejected` or `Goodbye` may arrive.
    AwaitingWelcome,
    /// After `Welcome`: every message is checked against its tables.
    Session(Tables),
}

/// A save request the server hasn't answered yet. It answers them in the order they
/// were sent: `SaveGame` with a `SaveResult`, and `LoadGame` with a new `Welcome`
/// (loaded) or a `SaveResult` (failed). So each answer pairs with the oldest one.
///
/// With several players (M4-1), a load sends *every* player the new game's
/// `Welcome` (D23), so a `Welcome` can also arrive unasked, after another player's
/// load. It answers this client's `LoadGame` only when one is the oldest
/// outstanding request; otherwise it is that broadcast, and leaves the requests
/// alone. D24 lets only the host save and load (M4-3), so a client's own load and
/// another's never race.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRequest {
    Save,
    Load,
}

impl SaveRequest {
    pub const ALL: [SaveRequest; 2] = [SaveRequest::Save, SaveRequest::Load];

    /// The name GDScript sees in a `SaveResult`'s `REQUEST` (`PaxKeys.REQUEST_*`).
    pub fn name(self) -> &'static str {
        match self {
            SaveRequest::Save => "save",
            SaveRequest::Load => "load",
        }
    }
}

/// The server's side of one connection, as seen by the client. Feed it bytes in any
/// chunking. It yields events until the first error, and then nothing ever again:
/// the caller must disconnect (D22).
#[derive(Debug)]
pub struct ServerStream {
    frames: FrameDecoder,
    phase: Phase,
    /// Save requests sent and not yet answered, oldest first.
    outstanding: std::collections::VecDeque<SaveRequest>,
    failed: Option<StreamError>,
}

impl Default for ServerStream {
    fn default() -> Self {
        ServerStream {
            frames: FrameDecoder::new(Direction::ServerToClient),
            phase: Phase::AwaitingWelcome,
            outstanding: std::collections::VecDeque::new(),
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

    /// Call when sending `SaveGame` or `LoadGame`, so their answers can be told apart
    /// (see [`SaveRequest`]): a new `Welcome` answers an outstanding `LoadGame` first.
    pub fn expect(&mut self, request: SaveRequest) {
        self.outstanding.push_back(request);
    }

    fn decode(&mut self, frame: &[u8]) -> Result<ServerEvent, StreamError> {
        use wire::ServerPayload as P;
        let msg = read_server_message(frame)?;
        let kind = msg.payload_type();
        let tables = match (self.phase, kind) {
            // A Welcome opens a session, or replaces it after a load: this client's
            // (the answer to its oldest request) or another player's (a broadcast).
            (Phase::AwaitingWelcome, P::Welcome) => None,
            (Phase::Session(_), P::Welcome) => {
                if self.outstanding.front() == Some(&SaveRequest::Load) {
                    self.outstanding.pop_front();
                }
                None
            }
            // Before the first Welcome, only a refusal can explain why there will be none.
            (Phase::AwaitingWelcome, P::Rejected | P::Goodbye) => None,
            (Phase::AwaitingWelcome, _) => return Err(invalid(format!("{} before Welcome", payload_tag(kind)))),
            (Phase::Session(t), _) => Some(t),
        };
        if kind == P::Welcome {
            let view = welcome(required(msg.payload_as_welcome(), "Welcome body")?)?;
            self.phase = Phase::Session(Tables::of(&view));
            return Ok(ServerEvent::Welcome(Box::new(view)));
        }
        Ok(match kind {
            P::Rejected => {
                let r = required(msg.payload_as_rejected(), "Rejected body")?;
                ServerEvent::Rejected { reason: required(r.reason(), "Rejected reason")?.to_owned() }
            }
            P::Goodbye => {
                let g = required(msg.payload_as_goodbye(), "Goodbye body")?;
                ServerEvent::Goodbye { reason: required(g.reason(), "Goodbye reason")?.to_owned() }
            }
            P::DayUpdate => {
                let u = required(msg.payload_as_day_update(), "DayUpdate body")?;
                ServerEvent::DayUpdate(Box::new(day_update(u, tables.expect("checked above"))?))
            }
            P::CommandResult => {
                let r = required(msg.payload_as_command_result(), "CommandResult body")?;
                ServerEvent::CommandResult {
                    client_seq: r.client_seq(),
                    error: r.error(),
                    applies_on_day: r.applies_on_day(),
                }
            }
            P::ServerState => {
                let s = required(msg.payload_as_server_state(), "ServerState body")?;
                ServerEvent::ServerState {
                    day: s.day(),
                    speed: s.speed(),
                    changed_by: s.changed_by(),
                    waiting_for: s.waiting_for().map(|w| w.iter().collect()).unwrap_or_default(),
                }
            }
            P::Pong => ServerEvent::Pong { nonce: required(msg.payload_as_pong(), "Pong body")?.nonce() },
            P::SaveResult => {
                let r = required(msg.payload_as_save_result(), "SaveResult body")?;
                // The answer to the oldest save request: a save, or a load that failed
                // (the session keeps its tables).
                let Some(request) = self.outstanding.pop_front() else {
                    return Err(invalid("a SaveResult without a SaveGame or LoadGame".to_owned()));
                };
                ServerEvent::SaveResult {
                    request,
                    name: required(r.name(), "SaveResult name")?.to_owned(),
                    // The server always sends it: empty means success.
                    error: required(r.error(), "SaveResult error")?.to_owned(),
                }
            }
            P::SaveList => {
                let l = required(msg.payload_as_save_list(), "SaveList body")?;
                ServerEvent::SaveList { names: strings(l.names(), "SaveList names")? }
            }
            P::LobbyState => {
                let l = required(msg.payload_as_lobby_state(), "LobbyState body")?;
                let nations = tables.expect("checked above").nations;
                let players = required(l.players(), "LobbyState players")?
                    .iter()
                    .map(|p| {
                        let nation = p.nation();
                        id_in(nation.as_slice(), nations, "a lobby player's nation")?;
                        Ok(LobbyPlayerView {
                            player: p.player(),
                            name: required(p.name(), "LobbyPlayer name")?.to_owned(),
                            nation,
                            sandbox: p.sandbox(),
                            ready: p.ready(),
                            host: p.host(),
                            away: p.away(),
                        })
                    })
                    .collect::<Result<_, StreamError>>()?;
                ServerEvent::LobbyState { players, started: l.started(), notice: l.notice().map(str::to_owned) }
            }
            _ => ServerEvent::Unknown(kind),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// A Welcome with `provinces` provinces in one market, one nation, whose
    /// province_market has `markets_listed` entries.
    fn welcome_frame(major: u16, provinces: usize, markets_listed: usize, with_defs: bool) -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();
        let names: Vec<_> = (0..provinces).map(|p| b.create_string(&format!("p{p}"))).collect();
        let names = b.create_vector(&names);
        let province_market = b.create_vector(&vec![0u32; markets_listed]);
        let market = b.create_string("m0");
        let markets = b.create_vector(&[market]);
        let empty_strings = b.create_vector::<flatbuffers::WIPOffset<&str>>(&[]);
        let key = b.create_string("n0");
        let nation_markets = b.create_vector(&[0u32]);
        let nation =
            wire::NationDef::create(&mut b, &wire::NationDefArgs { key: Some(key), markets: Some(nation_markets) });
        let nations = b.create_vector(&[nation]);
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
                    map_hash: None,
                    map_dir: None,
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

    /// A DayUpdate for one nation with a population map of `map_values` values, as a
    /// server from before `NationTable.militancy` sends it: no militancy column.
    fn day_update_frame(day: u64, map_values: usize) -> Vec<u8> {
        day_update_frame_with(day, map_values, None)
    }

    /// The same, with a NationTable militancy column of these raw `Fixed` values.
    fn day_update_frame_with(day: u64, map_values: usize, militancy: Option<&[i64]>) -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();
        let zero = wire::Fixed::new(0);
        let z = Some(&zero);
        let world = wire::WorldSummary::create(
            &mut b,
            &wire::WorldSummaryArgs {
                population: 10,
                household_spending: z,
                government_spending: z,
                input_spending: z,
                wages: z,
                dividends: z,
                taxes: z,
                transfers: z,
                life_needs: z,
                militancy: z,
                ..Default::default()
            },
        );
        let one = [wire::Fixed::new(1)];
        let (treasury, income, transfer, consumption) =
            (b.create_vector(&one), b.create_vector(&one), b.create_vector(&one), b.create_vector(&one));
        let population = b.create_vector(&[10u64]);
        let militancy = militancy.map(|m| b.create_vector(&m.iter().map(|&v| wire::Fixed::new(v)).collect::<Vec<_>>()));
        let nations = wire::NationTable::create(
            &mut b,
            &wire::NationTableArgs {
                treasury: Some(treasury),
                income_tax_rate: Some(income),
                transfer_rate: Some(transfer),
                consumption_rate: Some(consumption),
                population: Some(population),
                militancy,
            },
        );
        let values = b.create_vector(&vec![wire::Fixed::new(5_000_000); map_values]);
        let map = wire::MapView::create(
            &mut b,
            &wire::MapViewArgs { mode: wire::MapMode::Population, good: 0, values: Some(values) },
        );
        let u = wire::DayUpdate::create(
            &mut b,
            &wire::DayUpdateArgs {
                day,
                world: Some(world),
                nations: Some(nations),
                map: Some(map),
                ..Default::default()
            },
        );
        server_frame(&mut b, wire::ServerPayload::DayUpdate, u.as_union_value())
    }

    fn session(provinces: usize) -> Vec<u8> {
        let mut bytes = welcome_frame(PROTOCOL_MAJOR, provinces, provinces, true);
        bytes.extend(day_update_frame(1, provinces));
        bytes
    }

    #[test]
    fn decodes_a_session() {
        let mut s = ServerStream::default();
        let events = s.push(&session(12));
        assert_eq!(s.error(), None);
        let [ServerEvent::Welcome(w), ServerEvent::DayUpdate(u)] = events.as_slice() else { panic!("got {events:?}") };
        assert_eq!((w.provinces.len(), w.province_market.len(), w.nations.len()), (12, 12, 1));
        let map = u.map.as_ref().unwrap();
        assert_eq!((map.mode, map.values.len(), map.values[0]), (wire::MapMode::Population, 12, 5.0));
        assert_eq!((u.world.population, u.nations.income_tax_rate_raw[0]), (10, 1));
        assert_eq!(u.nations.militancy, None, "an older server sends no militancy; that is no data, not an error");
    }

    /// Protocol 1.2's militancy column, when sent, is checked like any other column.
    #[test]
    fn decodes_the_militancy_column_and_checks_its_length() {
        let mut s = ServerStream::default();
        let mut bytes = welcome_frame(PROTOCOL_MAJOR, 2, 2, true);
        bytes.extend(day_update_frame_with(1, 2, Some(&[250_000])));
        let events = s.push(&bytes);
        assert!(matches!(events.as_slice(), [_, ServerEvent::DayUpdate(u)] if u.nations.militancy == Some(vec![0.25])));

        let mut s = ServerStream::default();
        let mut bytes = welcome_frame(PROTOCOL_MAJOR, 2, 2, true);
        bytes.extend(day_update_frame_with(1, 2, Some(&[1, 2])));
        assert_eq!(s.push(&bytes).len(), 1);
        assert!(
            matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("NationTable militancy has 2 entries, expected 1"))
        );
    }

    fn lobby_frame(nation: Option<u32>, notice: Option<&str>) -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();
        let name = b.create_string("ada");
        let p = wire::LobbyPlayer::create(
            &mut b,
            &wire::LobbyPlayerArgs {
                player: 3,
                name: Some(name),
                nation,
                sandbox: false,
                ready: true,
                host: true,
                away: false,
            },
        );
        let players = b.create_vector(&[p]);
        let notice = notice.map(|n| b.create_string(n));
        let l =
            wire::LobbyState::create(&mut b, &wire::LobbyStateArgs { players: Some(players), started: false, notice });
        server_frame(&mut b, wire::ServerPayload::LobbyState, l.as_union_value())
    }

    /// M4-2: the lobby decodes, and its nations are checked like every other id.
    #[test]
    fn a_lobby_decodes_and_its_nations_are_checked() {
        let mut s = ServerStream::default();
        let mut bytes = welcome_frame(PROTOCOL_MAJOR, 2, 2, true);
        bytes.extend(lobby_frame(Some(0), Some("not every player is ready")));
        let events = s.push(&bytes);
        let [_, ServerEvent::LobbyState { players, started: false, notice: Some(n) }] = events.as_slice() else {
            panic!("{events:?}")
        };
        assert_eq!(
            (players[0].player, players[0].nation, players[0].host, n.as_str()),
            (3, Some(0), true, "not every player is ready")
        );
        assert!(s.push(&lobby_frame(Some(9), None)).is_empty());
        assert!(matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("a lobby player's nation 9")));
    }

    #[test]
    fn any_chunking_gives_the_same_events() {
        let bytes = session(30);
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
        let mut bytes = session(5);
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
        assert_eq!(s.error(), Some(&StreamError::Invalid("missing Welcome StaticData".into())));

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
    fn a_later_welcome_replaces_the_tables() {
        // Unasked: another player's load (M4-1). The new tables apply.
        let mut s = ServerStream::default();
        let mut bytes = welcome_frame(PROTOCOL_MAJOR, 2, 2, true);
        bytes.extend(welcome_frame(PROTOCOL_MAJOR, 3, 3, true));
        bytes.extend(day_update_frame(1, 3));
        assert_eq!(s.push(&bytes).len(), 3);
        assert_eq!(s.error(), None);

        // After a LoadGame, the new Welcome's tables replace the old ones.
        let mut s = ServerStream::default();
        assert_eq!(s.push(&welcome_frame(PROTOCOL_MAJOR, 2, 2, true)).len(), 1);
        s.expect(SaveRequest::Load);
        assert_eq!(s.push(&welcome_frame(PROTOCOL_MAJOR, 5, 5, true)).len(), 1);
        assert_eq!(s.push(&day_update_frame(2, 5)).len(), 1, "5 map values: matches the new tables");
        assert_eq!(s.error(), None);
    }

    fn save_result_frame(name: &str, error: &str) -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();
        let (name, error) = (b.create_string(name), b.create_string(error));
        let r = wire::SaveResult::create(&mut b, &wire::SaveResultArgs { name: Some(name), error: Some(error) });
        server_frame(&mut b, wire::ServerPayload::SaveResult, r.as_union_value())
    }

    #[test]
    fn a_failed_load_keeps_the_old_tables() {
        let mut s = ServerStream::default();
        s.push(&welcome_frame(PROTOCOL_MAJOR, 2, 2, true));
        s.expect(SaveRequest::Load);
        let failed = save_result_frame("x", "there is no save");
        assert!(
            matches!(s.push(&failed).as_slice(), [ServerEvent::SaveResult { request: SaveRequest::Load, error, .. }] if error.contains("no save"))
        );
        // Nothing is outstanding now: a SaveResult would be a protocol error.
        assert!(s.push(&save_result_frame("x", "")).is_empty());
        assert!(matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("without a SaveGame")));
    }

    /// Save then load before either is answered: the save's SaveResult answers the
    /// save, and the Welcome that follows answers the load (critic #44).
    #[test]
    fn answers_pair_with_requests_in_order() {
        let mut s = ServerStream::default();
        s.push(&welcome_frame(PROTOCOL_MAJOR, 2, 2, true));
        s.expect(SaveRequest::Save);
        s.expect(SaveRequest::Load);
        let mut bytes = save_result_frame("a", "");
        bytes.extend(welcome_frame(PROTOCOL_MAJOR, 3, 3, true));
        let events = s.push(&bytes);
        assert!(
            matches!(events.as_slice(), [ServerEvent::SaveResult { request: SaveRequest::Save, .. }, ServerEvent::Welcome(w)] if w.provinces.len() == 3)
        );
        assert_eq!(s.error(), None);
        // Nothing is outstanding now, so another SaveResult is a protocol error.
        assert!(s.push(&save_result_frame("a", "")).is_empty());
        assert!(matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("without a SaveGame")));
    }

    /// Another player's load while this client's save is outstanding: the Welcome is
    /// the broadcast, and the save's SaveResult still pairs with the save.
    #[test]
    fn a_welcome_never_answers_a_save() {
        let mut s = ServerStream::default();
        s.push(&welcome_frame(PROTOCOL_MAJOR, 2, 2, true));
        s.expect(SaveRequest::Save);
        let mut bytes = welcome_frame(PROTOCOL_MAJOR, 2, 2, true);
        bytes.extend(save_result_frame("a", ""));
        let events = s.push(&bytes);
        assert!(
            matches!(
                events.as_slice(),
                [ServerEvent::Welcome(_), ServerEvent::SaveResult { request: SaveRequest::Save, .. }]
            ),
            "{events:?}"
        );
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
        bytes.extend(day_update_frame(1, 5));
        let events = s.push(&bytes);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(s.error(), Some(StreamError::Invalid(e)) if e.contains("MapView values has 5 entries, expected 4"))
        );
    }

    #[test]
    fn tags_are_snake_case_for_every_message() {
        let mut s = ServerStream::default();
        let mut bytes = session(2);
        bytes.extend(pong());
        let tags: Vec<_> = s.push(&bytes).iter().map(ServerEvent::tag).collect();
        assert_eq!(tags, ["welcome", "day_update", "pong"]);
    }

    /// Every message this client decodes has its own tag, and it is a `keys` constant.
    #[test]
    fn every_message_has_a_generated_tag() {
        for &kind in wire::ServerPayload::ENUM_VALUES.iter().filter(|&&k| k != wire::ServerPayload::NONE) {
            let tag = payload_tag(kind);
            assert_ne!(tag, keys::UNKNOWN, "{kind:?} has no tag");
            assert!(keys::ALL.iter().any(|(_, v)| *v == tag), "{tag} is not in keys::ALL");
        }
    }

    #[test]
    fn display_converts_fixed_raw_values() {
        assert_eq!(display(wire::Fixed::new(1_500_000)), 1.5);
        assert_eq!(display(wire::Fixed::new(-250_000)), -0.25);
    }
}
