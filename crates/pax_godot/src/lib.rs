//! `pax_godot`: the client's side of the wire protocol, as a Godot GDExtension (D12).
//!
//! GDScript owns the UI. This library owns everything about the protocol: the
//! connection and the local server (`connection.rs`), framing, size limits,
//! verification, session rules and decoding (`decode.rs`), and building client
//! messages (`encode.rs`). All of that is plain Rust, tested without Godot (much of
//! it against a real `pax_server`). This file only converts between it and Godot
//! types, with every Dictionary key named once in `keys.rs`.
//!
//! It depends on `pax_protocol` and D12's side-neutral crates (`pax_map`,
//! `pax_content`), never on `pax_engine`, so the client can't simulate (D10). Float
//! arithmetic is linted everywhere except `decode::display`: code that builds
//! commands (simulation input) must use integers (D3).

pub mod args;
pub mod connection;
pub mod decode;
pub mod encode;
pub mod keys;
pub mod map;
pub mod rates;
pub mod transport;

use std::net::ToSocketAddrs;
use std::path::Path;
use std::time::Duration;

use connection::{Connection, LocalServer};
use decode::{
    DayUpdateView, MapViewData, MarketView, NationTableView, ProvinceView, ServerEvent, WelcomeView, WorldSummaryView,
};
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

/// One session with a game server, for GDScript.
///
/// Start one with `launch` (single player: starts `pax_server` on a free local port
/// and connects, NETWORK_PROTOCOL §6) or `connect_to`, then say `hello`. Call `poll`
/// once per frame: it returns every event since the last poll as a Dictionary whose
/// `PaxKeys.TYPE` is the message's `snake_case` tag (`PaxKeys.WELCOME`,
/// `PaxKeys.DAY_UPDATE`, …). When the connection ends, for any reason, one last
/// event of type `PaxKeys.CLOSED` carries the `PaxKeys.REASON`; nothing follows it.
///
/// Updates are acknowledged and the session kept alive automatically (see
/// `connection.rs`). An absent id is `null`, never a number (D22: no `-1` sentinels).
#[derive(GodotClass)]
#[class(base = RefCounted, init)]
pub struct PaxClient {
    /// The scenario's map, once `load_map` succeeded: picking reads it.
    map: Option<map::MapImage>,
    /// The latest `Welcome` (a load replaces it): `load_map` reads the session's
    /// map directory, hash and provinces from it, never from GDScript.
    welcome: Option<decode::WelcomeView>,
    /// Where this client's copy of the scenario's files is: `launch`'s scenario, or
    /// `set_scenario_dir` for a server started elsewhere.
    scenario_dir: Option<std::path::PathBuf>,
    connection: Option<Connection>,
    /// Declared after `connection`, so the connection closes first and the server
    /// can exit by itself (`--exit-when-idle`).
    server: Option<LocalServer>,
}

/// `null` for `None`.
fn optional<T: ToGodot>(v: Option<T>) -> Variant {
    v.map_or(Variant::nil(), |v| v.to_variant())
}

/// An id argument from GDScript: `null`, or an int checked by `args`.
fn optional_id(v: &Variant, what: &str) -> Result<Option<u32>, String> {
    let value = if v.is_nil() {
        None
    } else {
        Some(v.try_to::<i64>().map_err(|_| format!("{what} must be an int or null, not {v}"))?)
    };
    args::optional_id(value, what)
}

/// Reports a rejected call: an error for the caller and in Godot's log.
fn rejected(error: String) -> GString {
    godot_error!("PaxClient: {error}");
    GString::from(&error)
}

#[godot_api]
impl PaxClient {
    /// Starts `server_path` for `scenario_dir`, saving into `saves_dir`, and connects.
    /// Returns an error message, or `""` on success.
    #[func]
    fn launch(&mut self, server_path: GString, scenario_dir: GString, saves_dir: GString) -> GString {
        self.end_session();
        let (server, scenario, saves) = (server_path.to_string(), scenario_dir.to_string(), saves_dir.to_string());
        self.scenario_dir = Some(scenario.clone().into());
        match LocalServer::launch(Path::new(&server), Path::new(&scenario), Path::new(&saves)) {
            Ok(local) => {
                let addr = local.addr;
                self.server = Some(local);
                self.connect_addr(addr)
            }
            Err(e) => rejected(e),
        }
    }

    /// Hosts a multiplayer game (M4-9): starts `server_path` for `scenario_dir` with
    /// room for `players`, reachable from other machines over TLS (D24, M4-6), and
    /// connects to it. `hosted()` then gives what to share with the players. Returns
    /// an error message, or `""`.
    #[func]
    fn host_game(&mut self, server_path: GString, scenario_dir: GString, saves_dir: GString, players: i64) -> GString {
        let Ok(players) = u16::try_from(players).map_err(|_| ()).and_then(|p| if p >= 2 { Ok(p) } else { Err(()) })
        else {
            return rejected(format!("{players} players: a hosted game has at least 2"));
        };
        self.end_session();
        let (server, scenario, saves) = (server_path.to_string(), scenario_dir.to_string(), saves_dir.to_string());
        self.scenario_dir = Some(scenario.clone().into());
        let local = match LocalServer::host(Path::new(&server), Path::new(&scenario), Path::new(&saves), players) {
            Ok(local) => local,
            Err(e) => return rejected(e),
        };
        let (addr, fingerprint) = (local.addr, local.fingerprint.clone().unwrap_or_default());
        self.server = Some(local);
        // The host reaches its own server over TLS too, pinned like everyone else.
        match Connection::connect_tls(addr, Duration::from_secs(5), &fingerprint) {
            Ok(c) => {
                self.connection = Some(c);
                GString::new()
            }
            Err(e) => rejected(format!("cannot connect to the hosted game: {e}")),
        }
    }

    /// What the host shares so players can join (`PORT` and `FINGERPRINT`), or an
    /// empty Dictionary when this client isn't hosting.
    #[func]
    fn hosted(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        if let Some(local) = &self.server
            && let Some(fingerprint) = &local.fingerprint
        {
            d.set(keys::PORT, i64::from(local.addr.port()));
            d.set(keys::FINGERPRINT, &GString::from(fingerprint));
        }
        d
    }

    /// The player's name, sent with the next `hello` or `resume`; the lobby shows it.
    #[func]
    fn set_name(&mut self, name: GString) -> GString {
        let name = name.to_string();
        if name.is_empty() || name.len() > 32 {
            return rejected("a name is 1 to 32 bytes".to_owned());
        }
        self.with_connection(|c| c.set_name(name))
    }

    /// Where this client's copy of the scenario is, for a server it didn't launch
    /// (`launch` sets it itself; call this after `connect_to`, which clears it).
    /// `load_map` reads the map files from there.
    #[func]
    fn set_scenario_dir(&mut self, scenario_dir: GString) {
        self.scenario_dir = Some(scenario_dir.to_string().into());
    }

    /// Connects to a running server. Returns an error message, or `""`.
    #[func]
    fn connect_to(&mut self, host: GString, port: i64) -> GString {
        match self.new_session(&host, port) {
            Ok(addr) => self.connect_addr(addr),
            Err(e) => e,
        }
    }

    /// Ends the current session, if any, so nothing of it carries over into the next:
    /// its connection first, then a server it launched, so that server can exit by
    /// itself (`--exit-when-idle`); then its tables, map and scenario directory. The
    /// one place every connect path (`launch`, `host_game`, `connect_to`,
    /// `connect_secure`) clears per-session state.
    fn end_session(&mut self) {
        self.connection = None;
        self.server = None;
        self.welcome = None;
        self.map = None;
        self.scenario_dir = None;
    }

    /// Starts a new session to `host:port`: nothing of the previous one carries over,
    /// including a launched server and its scenario directory (call
    /// `set_scenario_dir` after). Resolves the address the same way for every
    /// connect: a host name, an IPv4 or an IPv6 address. The error is the message
    /// for the caller.
    fn new_session(&mut self, host: &GString, port: i64) -> Result<std::net::SocketAddr, GString> {
        let Ok(port) = u16::try_from(port) else { return Err(rejected(format!("port {port} out of range"))) };
        self.end_session();
        match (host.to_string(), port).to_socket_addrs().map(|mut a| a.next()) {
            Ok(Some(addr)) => Ok(addr),
            Ok(None) => Err(rejected(format!("{host} has no address"))),
            Err(e) => Err(rejected(format!("bad address {host}: {e}"))),
        }
    }

    /// Runs `send` on an open connection; an error message otherwise.
    fn with_connection(&mut self, send: impl FnOnce(&mut Connection)) -> GString {
        match &mut self.connection {
            Some(c) if c.is_open() => {
                send(c);
                GString::new()
            }
            _ => rejected("not connected".to_owned()),
        }
    }

    /// Connects to a multiplayer server elsewhere over TLS (D24, M4-6), trusting only
    /// the certificate whose SHA-256 is `fingerprint`: what the server printed, as
    /// the host shared it (any case, `:` and spaces allowed). Returns an error message
    /// (nothing connected), or `""`; a wrong certificate ends the connection with a
    /// reason as it is polled.
    #[func]
    fn connect_secure(&mut self, host: GString, port: i64, fingerprint: GString) -> GString {
        let addr = match self.new_session(&host, port) {
            Ok(addr) => addr,
            Err(e) => return e,
        };
        match Connection::connect_tls(addr, Duration::from_secs(5), &fingerprint.to_string()) {
            Ok(c) => {
                self.connection = Some(c);
                GString::new()
            }
            Err(e) => rejected(format!("cannot connect to {addr}: {e}")),
        }
    }

    fn connect_addr(&mut self, addr: std::net::SocketAddr) -> GString {
        match Connection::connect(addr, Duration::from_secs(5)) {
            Ok(c) => {
                self.connection = Some(c);
                GString::new()
            }
            Err(e) => rejected(format!("cannot connect to {addr}: {e}")),
        }
    }

    #[func]
    fn is_open(&self) -> bool {
        self.connection.as_ref().is_some_and(Connection::is_open)
    }

    /// Closes the connection (the launched server then exits by itself).
    #[func]
    fn disconnect_from_server(&mut self) {
        if let Some(c) = &mut self.connection {
            c.disconnect();
        }
    }

    /// Every event since the last poll. Never blocks.
    #[func]
    fn poll(&mut self) -> Array<VarDictionary> {
        let mut out = Array::new();
        let Some(c) = &mut self.connection else { return out };
        let polled = c.poll();
        for event in polled.events {
            if let ServerEvent::Welcome(w) = &event {
                // A new session or a load: its map has to be loaded again.
                self.welcome = Some((**w).clone());
                self.map = None;
            }
            out.push(&event_dictionary(event));
        }
        if let Some(reason) = polled.closed {
            let mut d = VarDictionary::new();
            d.set(keys::TYPE, keys::CLOSED);
            d.set(keys::REASON, &GString::from(&reason));
            out.push(&d);
        }
        out
    }

    /// Opens the session. `nation`: a nation index, or `null` for sandbox (M3).
    /// Returns an error message (the call sent nothing), or `""`.
    #[func]
    fn hello(&mut self, nation: Variant) -> GString {
        let nation = match optional_id(&nation, "nation") {
            Ok(n) => n,
            Err(e) => return rejected(e),
        };
        self.with_connection(|c| c.hello(nation))
    }

    /// `map_mode`: a `PaxKeys.MAP_MODE_*`. `market`, `province`: an index, or `null`
    /// for no panel. Returns an error message (the call sent nothing), or `""`.
    #[func]
    fn subscribe(&mut self, map_mode: i64, map_good: i64, market: Variant, province: Variant) -> GString {
        let checked = (|| {
            Ok::<_, String>((
                args::map_mode(map_mode)?,
                args::good(map_good)?,
                optional_id(&market, "market")?,
                optional_id(&province, "province")?,
            ))
        })();
        match checked {
            Ok((mode, good, market, province)) => self.with_connection(|c| c.subscribe(mode, good, market, province)),
            Err(e) => rejected(e),
        }
    }

    /// `speed`: a `PaxKeys.SPEED_*` (D23). Returns an error message, or `""`.
    #[func]
    fn set_speed(&mut self, speed: i64) -> GString {
        match args::speed(speed) {
            Ok(speed) => self.with_connection(|c| c.set_speed(speed)),
            Err(e) => rejected(e),
        }
    }

    /// Sends a policy command. Returns its `client_seq`, which its `CommandResult`
    /// will carry, or `null` if nothing was sent (the error is logged). `policy` is a
    /// `PaxKeys.POLICY_*`. `rate_raw` is the rate as a raw `Fixed` integer (0.15 is
    /// 150000): commands never carry floats (D3).
    #[func]
    fn submit_policy(&mut self, policy: GString, nation: i64, rate_raw: i64) -> Variant {
        let checked = args::policy(&policy.to_string()).and_then(|p| Ok((p, args::id(nation, "nation")?)));
        match (checked, &mut self.connection) {
            (Ok((policy, nation)), Some(c)) if c.is_open() => c.submit(policy, nation, rate_raw).to_variant(),
            (Ok(_), _) => {
                rejected("not connected".to_owned());
                Variant::nil()
            }
            (Err(e), _) => {
                rejected(e);
                Variant::nil()
            }
        }
    }

    /// Saves the game as `name` (D23); a `SaveResult` answers. Returns an error
    /// message (the call sent nothing), or `""`.
    #[func]
    fn save_game(&mut self, name: GString) -> GString {
        self.with_connection(|c| c.save_game(&name.to_string()))
    }

    /// Loads the save `name`: a new `Welcome` answers, or a `SaveResult` with the
    /// error. Returns an error message (the call sent nothing), or `""`.
    #[func]
    fn load_game(&mut self, name: GString) -> GString {
        self.with_connection(|c| c.load_game(&name.to_string()))
    }

    /// Loads this session's province map, ready to draw (see `map.rs`): from where
    /// the latest `Welcome` said it is, checked against its provinces and `map_hash`.
    /// The bridge keeps both; GDScript passes nothing. Returns a Dictionary with
    /// `PaxKeys.ERROR` (empty when it loaded) and, on success, `WIDTH`, `HEIGHT`, `IDS`
    /// (the RGB8 province-ID texels) and `LABELS` (one `Vector2i` anchor per province).
    /// A session without a map gives an empty Dictionary apart from `ERROR`.
    /// `province_at` then answers for this map.
    #[func]
    fn load_map(&mut self) -> VarDictionary {
        let mut d = VarDictionary::new();
        self.map = None;
        let (Some(welcome), Some(scenario_dir)) = (&self.welcome, &self.scenario_dir) else {
            d.set(keys::ERROR, &rejected("no session or no scenario directory yet".to_owned()));
            return d;
        };
        match map::load(scenario_dir, welcome) {
            Ok(Some((m, texels))) => {
                d.set(keys::ERROR, &GString::new());
                d.set(keys::WIDTH, i64::from(m.ids.width()));
                d.set(keys::HEIGHT, i64::from(m.ids.height()));
                d.set(keys::IDS, &PackedByteArray::from(texels.as_slice()));
                let mut labels: Array<Vector2i> = Array::new();
                for &[x, y] in &m.labels {
                    let coordinate = |v: u32| {
                        i32::try_from(v).expect("label inside the map, whose sides are at most pax_map::MAX_SIDE")
                    };
                    labels.push(Vector2i::new(coordinate(x), coordinate(y)));
                }
                d.set(keys::LABELS, &labels);
                self.map = Some(m);
            }
            Ok(None) => d.set(keys::ERROR, &GString::new()),
            Err(e) => d.set(keys::ERROR, &GString::from(&e)),
        }
        d
    }

    /// The province at map pixel `(x, y)` of the loaded map, or `null` for sea, off
    /// the map, or no map.
    #[func]
    fn province_at(&self, x: i64, y: i64) -> Variant {
        optional(self.map.as_ref().and_then(|m| m.province_at(x, y)).map(i64::from))
    }

    /// Asks for the saves; a `SaveList` answers. Returns an error message, or `""`.
    #[func]
    fn list_saves(&mut self) -> GString {
        self.with_connection(Connection::list_saves)
    }

    /// Lobby (M4-2): claims nation `nation`, or gives up the claim with `null`. A
    /// `LOBBY_STATE` answers. Returns an error message (nothing sent), or `""`.
    #[func]
    fn claim_nation(&mut self, nation: Variant) -> GString {
        match optional_id(&nation, "nation") {
            Ok(n) => self.with_connection(|c| c.claim_nation(n)),
            Err(e) => rejected(e),
        }
    }

    /// The server's password (or the admin's), sent with the next `hello` or
    /// `resume` (D24, protocol 1.6). `""` for none.
    #[func]
    fn set_password(&mut self, password: GString) -> GString {
        let password = Some(password.to_string()).filter(|p| !p.is_empty());
        self.with_connection(|c| c.set_password(password))
    }

    /// Reclaims the seat a dropped session kept (D24): `token` is the old `Welcome`'s
    /// `RESUME_TOKEN`. A `Welcome` answers, or `Rejected` if no seat is kept for it.
    #[func]
    fn resume(&mut self, token: i64) -> GString {
        // The token's bits, as the Welcome dictionary carried them.
        self.with_connection(|c| c.resume(token as u64))
    }

    /// Lobby: marks this player ready, or not.
    #[func]
    fn set_ready(&mut self, ready: bool) -> GString {
        self.with_connection(|c| c.set_ready(ready))
    }

    /// Lobby, host only: starts the game once everyone is ready.
    #[func]
    fn start_game(&mut self) -> GString {
        self.with_connection(Connection::start_game)
    }

    /// Host only (D24): ends player `player`'s session.
    #[func]
    fn kick(&mut self, player: i64) -> GString {
        match u16::try_from(player) {
            Ok(p) => self.with_connection(|c| c.kick(p)),
            Err(_) => rejected(format!("player {player} is not a valid player id")),
        }
    }

    /// A raw rate (`*_RATE_RAW`) as a fraction (0.15 for 15%), for display.
    #[func]
    fn rate_fraction(raw: i64) -> f64 {
        rates::fraction(raw)
    }

    /// A raw rate in whole per mille: a policy slider's position.
    #[func]
    fn rate_per_mille(raw: i64) -> i64 {
        rates::per_mille(raw)
    }

    /// A policy slider's per-mille position as the raw rate `submit_policy` takes,
    /// or `null` (logged) if out of range.
    #[func]
    fn rate_from_per_mille(per_mille: i64) -> Variant {
        match rates::from_per_mille(per_mille) {
            Ok(raw) => raw.to_variant(),
            Err(e) => {
                rejected(e);
                Variant::nil()
            }
        }
    }
}

fn strings(v: &[String]) -> PackedStringArray {
    v.iter().map(GString::from).collect()
}

fn floats(v: &[f64]) -> PackedFloat64Array {
    v.iter().copied().collect()
}

fn ints<T: Copy + Into<i64>>(v: &[T]) -> PackedInt64Array {
    v.iter().map(|&x| x.into()).collect()
}

/// Counts that may exceed `i64::MAX` in theory, saturated (display only).
fn counts(v: &[u64]) -> PackedInt64Array {
    v.iter().map(|&x| i64::try_from(x).unwrap_or(i64::MAX)).collect()
}

fn count(x: u64) -> i64 {
    i64::try_from(x).unwrap_or(i64::MAX)
}

fn event_dictionary(event: ServerEvent) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set(keys::TYPE, event.tag());
    match event {
        ServerEvent::Welcome(w) => welcome(&mut d, &w),
        ServerEvent::Rejected { reason } | ServerEvent::Goodbye { reason } => {
            d.set(keys::REASON, &GString::from(&reason))
        }
        ServerEvent::DayUpdate(u) => day_update(&mut d, &u),
        ServerEvent::CommandResult { client_seq, error, applies_on_day } => {
            d.set(keys::CLIENT_SEQ, i64::from(client_seq));
            d.set(keys::COMMAND_ERROR, i64::from(error.0));
            d.set(keys::APPLIES_ON_DAY, count(applies_on_day));
        }
        ServerEvent::ServerState { day, speed, changed_by, waiting_for } => {
            d.set(keys::DAY, count(day));
            d.set(keys::SPEED, i64::from(speed.0));
            d.set(keys::CHANGED_BY, i64::from(changed_by));
            d.set(keys::WAITING_FOR, &ints(&waiting_for));
        }
        // An identifier: its bits are kept as-is in Godot's signed 64-bit int.
        ServerEvent::Pong { nonce } => d.set(keys::NONCE, nonce as i64),
        ServerEvent::SaveResult { request, name, error } => {
            d.set(keys::REQUEST, request.name());
            d.set(keys::NAME, &GString::from(&name));
            d.set(keys::ERROR, &GString::from(&error));
        }
        ServerEvent::SaveList { names } => d.set(keys::NAMES, &strings(&names)),
        ServerEvent::LobbyState { players, started, notice } => {
            // A table: one column per field, one entry per player (keys.rs).
            let mut t = VarDictionary::new();
            t.set(keys::PLAYER, &ints(&players.iter().map(|p| p.player).collect::<Vec<_>>()));
            t.set(keys::NAME, &strings(&players.iter().map(|p| p.name.clone()).collect::<Vec<_>>()));
            let mut nations: Array<Variant> = Array::new();
            let (mut sandbox, mut ready, mut host, mut away): (Array<bool>, Array<bool>, Array<bool>, Array<bool>) =
                (Array::new(), Array::new(), Array::new(), Array::new());
            for p in &players {
                nations.push(&optional(p.nation.map(i64::from)));
                sandbox.push(p.sandbox);
                ready.push(p.ready);
                host.push(p.host);
                away.push(p.away);
            }
            t.set(keys::AWAY, &away);
            t.set(keys::NATION, &nations);
            t.set(keys::SANDBOX, &sandbox);
            t.set(keys::READY, &ready);
            t.set(keys::HOST, &host);
            d.set(keys::LOBBY_PLAYERS, &t);
            d.set(keys::STARTED, started);
            d.set(keys::NOTICE, &optional(notice.map(|n| GString::from(&n))));
        }
        ServerEvent::Unknown(_) => {}
    }
    d
}

fn welcome(d: &mut VarDictionary, w: &WelcomeView) {
    d.set(keys::PROTOCOL_MINOR, i64::from(w.protocol_minor));
    d.set(keys::PLAYER, i64::from(w.player));
    // An identifier: its bits are kept as-is in Godot's signed 64-bit int.
    d.set(keys::RESUME_TOKEN, w.resume_token as i64);
    d.set(keys::NATION, &optional(w.nation.map(i64::from)));
    d.set(keys::DAY, count(w.day));
    d.set(keys::SPEED, i64::from(w.speed.0));
    d.set(keys::SCENARIO, &GString::from(&w.scenario));
    // Identifiers: their bits are kept as-is in Godot's signed 64-bit int.
    d.set(keys::CONTENT_HASH, w.content_hash as i64);
    d.set(keys::MAP_HASH, &optional(w.map_hash.map(|h| h as i64)));
    d.set(keys::MAP_DIR, &optional(w.map_dir.as_deref().map(GString::from)));
    d.set(keys::GOODS, &strings(&w.goods));
    d.set(keys::PROFESSIONS, &strings(&w.professions));
    d.set(keys::PRODUCER_TYPES, &strings(&w.producer_types));
    d.set(keys::PROVINCES, &strings(&w.provinces));
    d.set(keys::PROVINCE_MARKET, &ints(&w.province_market));
    d.set(keys::MARKETS, &strings(&w.markets));
    d.set(keys::NATIONS, &strings(&w.nations));
    let mut nation_markets: Array<PackedInt64Array> = Array::new();
    for markets in &w.nation_markets {
        nation_markets.push(&ints(markets));
    }
    d.set(keys::NATION_MARKETS, &nation_markets);
}

fn day_update(d: &mut VarDictionary, u: &DayUpdateView) {
    d.set(keys::DAY, count(u.day));
    d.set(keys::SPEED, i64::from(u.speed.0));
    d.set(keys::SKIPPED, i64::from(u.skipped));
    d.set(keys::STATE_HASH, u.state_hash as i64);
    d.set(keys::WORLD, &world_summary(&u.world));
    d.set(keys::NATION_TABLE, &nation_table(&u.nations));
    d.set(keys::MAP, &optional(u.map.as_ref().map(map_view)));
    d.set(keys::MARKET, &optional(u.market.as_ref().map(market_view)));
    d.set(keys::PROVINCE, &optional(u.province.as_ref().map(province_view)));
}

fn world_summary(w: &WorldSummaryView) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set(keys::POPULATION, count(w.population));
    d.set(keys::WORKFORCE, count(w.workforce));
    d.set(keys::UNEMPLOYED, count(w.unemployed));
    d.set(keys::HOUSEHOLD_SPENDING, w.household_spending);
    d.set(keys::GOVERNMENT_SPENDING, w.government_spending);
    d.set(keys::INPUT_SPENDING, w.input_spending);
    d.set(keys::WAGES, w.wages);
    d.set(keys::DIVIDENDS, w.dividends);
    d.set(keys::TAXES, w.taxes);
    d.set(keys::TRANSFERS, w.transfers);
    d.set(keys::DEPRIVED, count(w.deprived));
    d.set(keys::LIFE_NEEDS, w.life_needs);
    d.set(keys::MILITANCY, w.militancy);
    d
}

fn nation_table(n: &NationTableView) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set(keys::TREASURY, &floats(&n.treasury));
    d.set(keys::INCOME_TAX_RATE_RAW, &ints(&n.income_tax_rate_raw));
    d.set(keys::TRANSFER_RATE_RAW, &ints(&n.transfer_rate_raw));
    d.set(keys::CONSUMPTION_RATE_RAW, &ints(&n.consumption_rate_raw));
    d.set(keys::POPULATION, &counts(&n.population));
    // null from a server older than protocol 1.2, which doesn't send it.
    d.set(keys::MILITANCY, &optional(n.militancy.as_deref().map(floats)));
    d
}

fn map_view(m: &MapViewData) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set(keys::MODE, i64::from(m.mode.0));
    d.set(keys::GOOD, i64::from(m.good));
    d.set(keys::VALUES, &floats(&m.values));
    d
}

fn market_view(m: &MarketView) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set(keys::MARKET_ID, i64::from(m.market));
    d.set(keys::PRICE, &floats(&m.price));
    d.set(keys::SUPPLY, &floats(&m.supply));
    d.set(keys::DEMAND, &floats(&m.demand));
    d.set(keys::TRADED, &floats(&m.traded));
    d
}

fn province_view(p: &ProvinceView) -> VarDictionary {
    let mut pops = VarDictionary::new();
    pops.set(keys::PROFESSION, &ints(&p.pop_profession));
    pops.set(keys::PEOPLE, &ints(&p.pop_people));
    pops.set(keys::CASH, &floats(&p.pop_cash));
    pops.set(keys::LIFE_NEEDS, &floats(&p.pop_life_needs));
    pops.set(keys::MILITANCY, &floats(&p.pop_militancy));
    let mut labour = VarDictionary::new();
    labour.set(keys::PROFESSION, &ints(&p.labour_profession));
    labour.set(keys::WORKFORCE, &counts(&p.labour_workforce));
    labour.set(keys::JOBS, &counts(&p.labour_jobs));
    labour.set(keys::EMPLOYED, &counts(&p.labour_employed));
    let mut producers = VarDictionary::new();
    producers.set(keys::PRODUCER_TYPE, &ints(&p.producer_type));
    producers.set(keys::CAPACITY, &ints(&p.producer_capacity));
    producers.set(keys::EMPLOYED, &ints(&p.producer_employed));
    producers.set(keys::WAGE, &floats(&p.producer_wage));
    producers.set(keys::CASH, &floats(&p.producer_cash));
    let mut d = VarDictionary::new();
    d.set(keys::PROVINCE_ID, i64::from(p.province));
    d.set(keys::POPS, &pops);
    d.set(keys::LABOUR, &labour);
    d.set(keys::PRODUCERS, &producers);
    d
}
