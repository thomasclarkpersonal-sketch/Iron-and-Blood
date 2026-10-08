//! The sim thread (D23): the only owner of the `World`.
//!
//! It handles session requests in arrival order and decides everything that needs game
//! state:
//! * the handshake (`Welcome` or `Rejected`);
//! * subscriptions and their views;
//! * commands, checked, queued and applied at the start of the next tick;
//! * the log of every command that applied (D21, D23), kept by [`Game`];
//! * pacing: when ticks happen, at the session's chosen speed;
//! * flow control: which updates each session is sent, kept in [`crate::window`];
//! * saves and loads (D23).
//!
//! Its sessions are rows in a [`SessionTable`]: up to `Config::max_players` play at
//! once, each with a distinct player id and nation (M4-1, D24). One of them is the
//! host, who alone sets the speed (anyone may pause), saves, loads and kicks (M4-3).

use flume::{Receiver, RecvTimeoutError, TryRecvError};
use std::path::{Path, PathBuf};
use std::time::Instant;

use pax_data::Scenario;
use pax_data::save;
use pax_protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, wire};
use tracing::{debug, info};

use crate::clock::Clock;
use crate::commands;
use crate::encode::{self, WelcomeInfo};
use crate::game::Game;
use crate::net::{Inbound, Outbound};
use crate::queue::CommandQueue;
use crate::request::{Request, WireCommand};
use crate::session::{Refusal, Seat, Session, SessionTable};
use crate::view::{self, CheckedSubscription, DayViews, Subscription};
use crate::window::UpdateWindow;

pub(crate) struct Sim {
    game: Game,
    /// The scenario directory as the server was started with it (or as the loaded
    /// save names it): what a save records so it can be loaded again.
    scenario_dir: PathBuf,
    saves_dir: PathBuf,
    sessions: SessionTable,
    /// How many sessions may play at once (`Config::max_players`).
    max_players: usize,
    /// Whether sessions without a nation are accepted (`Config::sandbox`, D24).
    sandbox: bool,
    /// The host's client name on a dedicated server (`Config::admin`, D24).
    admin: Option<String>,
    /// Stop when the last player leaves (the client launched this server).
    exit_when_idle: bool,
    /// The speed and the next tick. [`Sim::set_clock`] changes the speed and announces
    /// it; the one exception is a load, whose new `Welcome` carries the speed.
    clock: Clock,
    /// Commands accepted for the next tick, stamped on arrival.
    queue: CommandQueue,
}

impl Sim {
    pub(crate) fn new(scenario: Scenario, config: &crate::Config) -> Self {
        Sim {
            game: Game::new(scenario),
            scenario_dir: config.scenario.clone(),
            saves_dir: config.saves_dir.clone(),
            sessions: SessionTable::default(),
            max_players: usize::from(config.max_players),
            sandbox: config.sandbox,
            admin: config.admin.clone(),
            exit_when_idle: config.exit_when_idle,
            clock: Clock::Paused,
            queue: CommandQueue::default(),
        }
    }

    fn send(&self, session: u64, out: Outbound) {
        self.sessions.send(session, out);
    }

    /// A protocol error the sim thread detected: `Goodbye` with the reason, then close (D22).
    fn goodbye(&self, session: u64, reason: &str) {
        info!(session, reason, "closing session");
        self.send(session, Outbound::Frame(encode::goodbye(reason)));
        self.send(session, Outbound::Close);
    }

    fn reject(&self, session: u64, reason: &str) {
        info!(session, reason, "rejected");
        self.send(session, Outbound::Frame(encode::rejected(reason)));
        self.send(session, Outbound::Close);
    }

    fn hello(&mut self, session: u64, major: u16, minor: u16, name: Option<&str>, requested_nation: Option<u32>) {
        let nations = self.game.world().nations.key.len();
        if major != PROTOCOL_MAJOR {
            let reason = format!(
                "protocol {major}.{minor} is not supported; this server speaks {PROTOCOL_MAJOR}.{PROTOCOL_MINOR}"
            );
            return self.reject(session, &reason);
        }
        // `net` lets a session say Hello only once; the sim thread doesn't rely on it.
        if self.sessions.seat(session).is_some() {
            return self.goodbye(session, "Hello sent twice");
        }
        if requested_nation.is_none() && !self.sandbox {
            return self.reject(session, "this server has no sandbox (it runs without --sandbox): ask for a nation");
        }
        if let Some(n) = requested_nation
            && n as usize >= nations
        {
            return self.reject(session, &format!("unknown nation {n}: the scenario has {nations}"));
        }
        // The table checks the limit and that the nation is free (D24), and picks the
        // player id. A connection's events arrive in order on one channel (Connected,
        // its requests, then Closed), so a session that sent Hello is always known.
        let seat = match self.sessions.sit(session, requested_nation, self.max_players) {
            Ok(seat) => seat,
            Err(Refusal::Full) => {
                let reason = match self.max_players {
                    1 => "server full: a single-player server accepts one client".to_owned(),
                    n => format!("server full: all {n} players are connected"),
                };
                return self.reject(session, &reason);
            }
            Err(Refusal::NationTaken { player }) => {
                let n = requested_nation.expect("only a nation can be taken");
                let key = &self.game.world().nations.key[n as usize];
                return self.reject(session, &format!("nation {n} ({key}) is taken by player {player}"));
            }
        };
        info!(session, name, player = seat.player, ?requested_nation, "welcomed");
        // The host (D24): the admin by name on a dedicated server, else the first player.
        let host = match &self.admin {
            Some(admin) => name == Some(admin.as_str()),
            None => true,
        };
        if host && self.sessions.host().is_none() {
            self.sessions.set_host(session);
            info!(session, player = seat.player, "host");
        }
        self.welcome(session, seat);
    }

    /// Replaces the session's subscription and answers with a `DayUpdate` for the
    /// current day, so a newly opened panel fills at once, even when paused (D22).
    /// The refresh bypasses the flow-control window: the client asked for it.
    fn subscribe(&mut self, session: u64, requested: Subscription) {
        let subscription = match requested.checked(self.game.world()) {
            Ok(s) => s,
            Err(reason) => return self.goodbye(session, &reason),
        };
        let Some(s) = self.sessions.get_mut(session) else { return };
        s.subscription = subscription;
        s.conn.send(Outbound::Frame(view::day_update(&self.game.views(), &subscription, self.clock.speed(), 0)));
    }

    fn command_result(&self, session: u64, client_seq: u32, error: wire::CommandError) {
        let day = if error == wire::CommandError::None { self.game.world().day } else { 0 };
        self.send(session, Outbound::Frame(encode::command_result(client_seq, error, day)));
    }

    /// Checks a command on arrival and queues it for the start of the next tick
    /// (NETWORK_PROTOCOL §5). The checks run in this order:
    /// 1. well-formed;
    /// 2. permission: the session's nation (D24; sandbox allows all);
    /// 3. `World::validate`, the single validity rule (D21).
    ///
    /// Every submission gets exactly one `CommandResult` now. A second one, an error,
    /// follows only if the command fails when it is applied.
    fn submit(&mut self, session: u64, client_seq: u32, command: Option<WireCommand>) {
        let Some(seat) = self.sessions.seat(session) else { return };
        let command = match command.ok_or(wire::CommandError::Malformed).and_then(commands::to_engine) {
            Ok(c) => c,
            Err(e) => return self.command_result(session, client_seq, e),
        };
        if seat.nation.is_some_and(|n| n as usize != commands::nation_of(&command)) {
            return self.command_result(session, client_seq, wire::CommandError::NotPermitted);
        }
        if let Err(e) = self.game.world().validate(command) {
            return self.command_result(session, client_seq, commands::error_to_wire(&e));
        }
        self.queue.stamp(session, seat.player, client_seq, command);
        self.command_result(session, client_seq, wire::CommandError::None);
    }

    /// A session asked for a new speed. A speed newer than this server is ignored, as
    /// D22 requires of unknown enum values: the clock is unchanged, and the session is
    /// told the current state so it can resynchronise.
    ///
    /// Only the host changes the speed, but any player may pause (D24). A refused
    /// change gets the same resynchronising `ServerState`.
    fn set_speed(&mut self, session: u64, speed: wire::Speed) {
        let Some(seat) = self.sessions.seat(session) else { return };
        let permitted = speed == wire::Speed::Paused || self.sessions.is_host(session);
        match Clock::at(speed, Instant::now()).filter(|_| permitted) {
            Some(clock) => self.set_clock(clock, seat.player),
            None => {
                debug!(session, speed = speed.0, permitted, "ignored: an unknown speed, or not the host");
                let frame = encode::server_state(self.game.world().day, self.clock.speed(), seat.player);
                self.send(session, Outbound::Frame(frame));
            }
        }
    }

    /// The only place the speed changes: sets the clock and tells every welcomed
    /// session who changed it (`ServerState`, D23).
    fn set_clock(&mut self, clock: Clock, changed_by: u16) {
        self.clock = clock;
        let speed = clock.speed();
        info!(?speed, changed_by, "speed changed");
        self.sessions.broadcast(&encode::server_state(self.game.world().day, speed, changed_by));
    }

    /// The session processed the update for `day`: everything up to it leaves the
    /// window. If days ran while the window was full, the latest day goes out now.
    fn ack(&mut self, session: u64, day: u64) {
        let Some(s) = self.sessions.get_mut(session) else { return };
        if s.window.ack(day) {
            send_update(s, &self.game.views(), self.clock.speed());
        }
    }

    /// Runs one day: the queued commands in stamp order (D10), through
    /// [`Game::step`], which logs what applied. A player's command that fails when
    /// applied gets a late error `CommandResult`.
    pub(crate) fn tick(&mut self) {
        for (p, e) in self.game.step(self.queue.take()) {
            self.command_result(p.session, p.client_seq, commands::error_to_wire(&e));
        }
    }

    /// Ticks, then sends each welcomed session the new day if its window has room
    /// (D23). The views are built once and shared by all sessions.
    fn advance(&mut self) {
        self.tick();
        let due: Vec<u64> =
            self.sessions.welcomed_mut().filter_map(|(id, s)| s.window.day_ran().then_some(id)).collect();
        if !due.is_empty() {
            // Build the views only when someone will see them.
            let views = self.game.views();
            for id in due {
                if let Some(s) = self.sessions.get_mut(id) {
                    send_update(s, &views, self.clock.speed());
                }
            }
        }
    }

    /// `saves/<name>.toml` for a valid save name. A name is 1 to 64 characters of
    /// `[A-Za-z0-9_-]`, so it can never escape the saves directory (D23).
    fn save_path(&self, name: Option<&str>) -> Result<PathBuf, String> {
        let name = name.ok_or("a save needs a name")?;
        let valid =
            (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if !valid {
            return Err(format!("'{name}' is not a save name: use 1 to 64 letters, digits, '_' or '-'"));
        }
        Ok(self.saves_dir.join(format!("{name}.toml")))
    }

    /// Writes the game as the scenario plus every applied command (D23). Commands
    /// queued for the next tick haven't applied yet, so they aren't saved.
    fn save_game(&self, session: u64, name: Option<String>) {
        let label = name.clone().unwrap_or_default();
        if !self.sessions.is_host(session) {
            return self.send(session, Outbound::Frame(encode::save_result(&label, NOT_HOST_SAVE)));
        }
        let world = self.game.world();
        let result = self.save_path(name.as_deref()).and_then(|path| {
            let data = self.game.save_data(&self.scenario_dir);
            data.write(&path, world).map_err(|e| format!("could not write {}: {e}", path.display()))
        });
        info!(session, name = %label, ?result, "save");
        let error = result.err().unwrap_or_default();
        self.send(session, Outbound::Frame(encode::save_result(&label, &error)));
    }

    /// The saves in the saves directory. A missing directory means no saves yet; any
    /// other IO error is logged (`SaveList` has no error field) and lists none.
    fn list_saves(&self, session: u64) {
        let entries = match std::fs::read_dir(&self.saves_dir) {
            Ok(entries) => Some(entries),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                tracing::warn!(dir = %self.saves_dir.display(), error = %e, "cannot list saves");
                None
            }
        };
        let mut names: Vec<String> = entries
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let name = path.file_stem()?.to_str()?.to_owned();
                (path.extension()? == "toml" && self.save_path(Some(&name)).is_ok()).then_some(name)
            })
            .collect();
        names.sort();
        self.send(session, Outbound::Frame(encode::save_list(&names)));
    }

    /// Replaces the running game with a saved one (D23). `save::load` checks the
    /// content hash and the history's consistency, then reads the snapshot, without
    /// replaying; `save::load_by_replay` (`pax_cli replay`) is the full check. The game then pauses and every
    /// session gets a new `Welcome`, because the scenario and its tables may differ
    /// (NETWORK_PROTOCOL §3). Commands queued for the next tick are discarded. On any
    /// error the running game is left untouched.
    fn load_game(&mut self, session: u64, name: Option<String>) {
        let label = name.clone().unwrap_or_default();
        if !self.sessions.is_host(session) {
            return self.send(session, Outbound::Frame(encode::save_result(&label, NOT_HOST_LOAD)));
        }
        let loaded = self.save_path(name.as_deref()).and_then(|path| load_from(&path));
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                info!(session, name = %label, %error, "load failed");
                return self.send(session, Outbound::Frame(encode::save_result(&label, &error)));
            }
        };
        info!(session, name = %label, day = loaded.save.day, "loaded");
        let save::LoadedSave { scenario, save, last_report } = loaded;
        self.scenario_dir = save.scenario.clone();
        // The world, its history and its derived views are replaced together (game.rs).
        self.game = Game::resume(scenario, save, last_report);
        self.queue.discard();
        // Not `set_clock`: the `Welcome` below announces the speed with the new game.
        self.clock = Clock::Paused;
        let nations = self.game.world().nations.key.len();
        // Every player gets the new game's Welcome, in session order (NETWORK_PROTOCOL §3).
        for id in self.sessions.welcomed() {
            let seat = self.sessions.seat(id).expect("welcomed sessions have a seat");
            // A seat is never widened: a player whose nation the loaded game lacks
            // leaves, rather than becoming a sandbox seat that commands every nation.
            if let Some(n) = seat.nation.filter(|&n| n as usize >= nations) {
                let was_host = self.sessions.is_host(id);
                self.goodbye(id, &format!("the loaded game has no nation {n}, which you played"));
                self.sessions.unseat(id);
                self.player_left(id, seat, was_host);
                continue;
            }
            let Some(s) = self.sessions.get_mut(id) else { continue };
            s.subscription = CheckedSubscription::default();
            s.window = UpdateWindow::default();
            self.welcome(id, seat);
        }
    }

    /// The host ends another player's session (D24). Anything else is ignored: a
    /// non-host's kick, the host kicking itself, a player who isn't connected.
    fn kick(&mut self, session: u64, player: u16) {
        let target = self.sessions.session_of(player).filter(|&t| t != session);
        let Some(target) = target.filter(|_| self.sessions.is_host(session)) else {
            return debug!(session, player, "ignored kick");
        };
        info!(session, target, player, "kicked by the host");
        self.goodbye(target, "kicked by the host");
        // Unseated now, so whatever it still sends is dropped; its row goes when its
        // connection closes. The host stays, so the game goes on.
        if let Some(seat) = self.sessions.unseat(target) {
            self.player_left(target, seat, false);
        }
    }

    /// A player stopped playing: left, or was kicked. When the host leaves a
    /// player-hosted server, the remaining player with the lowest id becomes host;
    /// on a dedicated server, the host's role waits for the admin to return (D24).
    /// When the last player leaves, nobody is watching, so the clock stops (D23).
    /// Returns `false` when the server should stop.
    fn player_left(&mut self, session: u64, seat: Seat, was_host: bool) -> bool {
        if self.sessions.players() == 0 {
            info!(session, player = seat.player, "the last player left");
            self.set_clock(Clock::Paused, seat.player);
            if self.exit_when_idle {
                info!("the client left; exiting (--exit-when-idle)");
                return false;
            }
        } else if was_host
            && self.admin.is_none()
            && let Some(next) = self.sessions.lowest_player()
        {
            self.sessions.set_host(next);
            info!(session = next, "the host left; the host is now this session");
        }
        true
    }

    fn welcome(&self, session: u64, seat: Seat) {
        let info = WelcomeInfo {
            player: seat.player,
            resume_token: 0, // resuming a dropped session is M4 (D24)
            nation: seat.nation,
            scenario: &self.game.scenario().name,
            content_hash: self.game.scenario().content_hash,
            map: self.game.scenario().map.as_ref(),
            speed: self.clock.speed(),
        };
        self.send(session, Outbound::Frame(encode::welcome(self.game.world(), &info)));
    }

    #[cfg(test)]
    pub(crate) fn world_day(&self) -> u64 {
        self.game.world().day
    }

    /// Every command applied so far, in application order.
    #[cfg(test)]
    pub(crate) fn log(&self) -> &[save::SavedCommand] {
        self.game.log()
    }

    /// Handles one inbound event. Returns `false` when the server should stop.
    pub(crate) fn handle(&mut self, event: Inbound) -> bool {
        match event {
            Inbound::Connected { session, conn } => self.sessions.connect(session, conn),
            Inbound::Request { session, request: Request::Hello { major, minor, name, requested_nation, .. } } => {
                self.hello(session, major, minor, name.as_deref(), requested_nation);
            }
            // The admission choke point: only welcomed sessions get past here.
            Inbound::Request { session, request } if self.sessions.seat(session).is_none() => {
                debug!(session, ?request, "ignored: the session was not welcomed");
            }
            Inbound::Request { session, request } => match request {
                Request::Subscribe { map_mode, map_good, market, province } => {
                    self.subscribe(session, Subscription { map_mode, map_good, market, province });
                }
                Request::SubmitCommand { client_seq, command } => self.submit(session, client_seq, command),
                Request::SetSpeed { speed } => self.set_speed(session, speed),
                Request::Ack { day } => self.ack(session, day),
                Request::SaveGame { name } => self.save_game(session, name),
                Request::LoadGame { name } => self.load_game(session, name),
                Request::ListSaves => self.list_saves(session),
                Request::Kick { player } => self.kick(session, player),
                // Hello is handled before the admission choke point; Ping by the network task.
                Request::Hello { .. } | Request::Ping { .. } => {}
            },
            Inbound::Closed { session } => {
                let was_host = self.sessions.is_host(session);
                // The others play on (D24); see `player_left`.
                if let Some(seat) = self.sessions.remove(session).and_then(|s| s.seat()) {
                    return self.player_left(session, seat, was_host);
                }
            }
            Inbound::Shutdown => return false,
            #[cfg(test)]
            Inbound::Crash => panic!("injected crash for a test"),
        }
        true
    }
}

/// What a non-host is told when it asks to save or load (D24).
const NOT_HOST_SAVE: &str = "only the host can save the game";
const NOT_HOST_LOAD: &str = "only the host can load a game";

/// Loads a save, as a message for the client on failure.
fn load_from(path: &Path) -> Result<save::LoadedSave, String> {
    if !path.exists() {
        return Err(format!("there is no save {}", path.display()));
    }
    save::load(path).map_err(|e| e.messages.join("; "))
}

impl Sim {
    /// After the sim thread panicked: tell every connection why it is being closed.
    pub(crate) fn fail(&self, message: &str) {
        let reason = format!("the server failed: {message}");
        for session in self.sessions.all() {
            session.conn.send(Outbound::Frame(encode::goodbye(&reason)));
            session.conn.send(Outbound::Close);
        }
    }
}

/// Sends `s` an update for the current day and puts it in the session's window.
fn send_update(s: &mut Session, views: &DayViews<'_>, speed: wire::Speed) {
    let skipped = s.window.sent(views.world.day);
    s.conn.send(Outbound::Frame(view::day_update(views, &s.subscription, speed, skipped)));
}

/// The sim thread's loop: handle everything already waiting, tick if due, otherwise
/// sleep until the next event or the next tick. Draining first means input is never
/// starved, even at `Fastest`.
pub(crate) fn run(sim: &mut Sim, inbound: Receiver<Inbound>) {
    loop {
        let event = match inbound.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Disconnected) => break,
            // Nothing waiting: tick if due, otherwise sleep until an event or the tick.
            Err(TryRecvError::Empty) => match sim.clock.due() {
                Some(due) if Instant::now() >= due => {
                    sim.advance();
                    sim.clock.ticked(Instant::now());
                    continue;
                }
                Some(due) => match inbound.recv_deadline(due) {
                    Ok(event) => event,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None => match inbound.recv() {
                    Ok(event) => event,
                    Err(_) => break,
                },
            },
        };
        if !sim.handle(event) {
            break;
        }
    }
    // Close every remaining connection (e.g. a rejected client that stayed connected).
    for session in sim.sessions.all() {
        session.conn.send(Outbound::Frame(encode::goodbye("the server is shutting down")));
        session.conn.send(Outbound::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::ConnHandle;
    use pax_engine::{Command, Fixed};
    use pax_protocol::read_server_message;
    use tokio::sync::mpsc::Receiver;

    const SESSION: u64 = 1;

    /// What the sim thread sent a session, decoded.
    #[derive(Debug, PartialEq)]
    enum Sent {
        Welcome {
            player: u16,
        },
        Rejected,
        Goodbye(String),
        Result {
            seq: u32,
            error: wire::CommandError,
            day: u64,
        },
        Update {
            day: u64,
            skipped: u32,
        },
        /// The speed, and the player who set it.
        State(wire::Speed, u16),
        Saved {
            name: String,
            error: String,
        },
        Saves(Vec<String>),
        Close,
    }

    fn drain(rx: &mut Receiver<Outbound>) -> Vec<Sent> {
        let mut out = Vec::new();
        while let Ok(o) = rx.try_recv() {
            out.push(match o {
                Outbound::Close => Sent::Close,
                Outbound::Frame(f) => {
                    let m = read_server_message(&f).unwrap();
                    if let Some(w) = m.payload_as_welcome() {
                        Sent::Welcome { player: w.player() }
                    } else if m.payload_as_rejected().is_some() {
                        Sent::Rejected
                    } else if let Some(g) = m.payload_as_goodbye() {
                        Sent::Goodbye(g.reason().unwrap_or_default().to_owned())
                    } else if let Some(r) = m.payload_as_command_result() {
                        Sent::Result { seq: r.client_seq(), error: r.error(), day: r.applies_on_day() }
                    } else if let Some(u) = m.payload_as_day_update() {
                        Sent::Update { day: u.day(), skipped: u.skipped() }
                    } else if let Some(s) = m.payload_as_server_state() {
                        Sent::State(s.speed(), s.changed_by())
                    } else if let Some(r) = m.payload_as_save_result() {
                        Sent::Saved {
                            name: r.name().unwrap_or_default().into(),
                            error: r.error().unwrap_or_default().into(),
                        }
                    } else if let Some(l) = m.payload_as_save_list() {
                        Sent::Saves(l.names().unwrap().iter().map(str::to_owned).collect())
                    } else {
                        panic!("unexpected {:?}", m.payload_type())
                    }
                }
            });
        }
        out
    }

    /// `two_states` with one welcomed session playing `nation` (`None`: sandbox).
    /// A fresh saves directory for one test, removed when dropped.
    struct SavesDir(PathBuf);

    impl Drop for SavesDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_saves_dir() -> SavesDir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("pax-sim-saves-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        SavesDir(dir)
    }

    fn welcomed(nation: Option<u32>) -> (Sim, Receiver<Outbound>, SavesDir) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let saves = test_saves_dir();
        let mut config = crate::Config::local(&dir);
        config.saves_dir = saves.0.clone();
        let mut sim = Sim::new(pax_data::load_scenario(&dir).unwrap(), &config);
        let (conn, mut rx) = ConnHandle::for_test();
        sim.handle(Inbound::Connected { session: SESSION, conn });
        let hello = Request::Hello {
            major: PROTOCOL_MAJOR,
            minor: 0,
            name: Some("t".into()),
            requested_nation: nation,
            resume_token: 0,
        };
        sim.handle(Inbound::Request { session: SESSION, request: hello });
        assert_eq!(drain(&mut rx), [Sent::Welcome { player: 0 }]);
        (sim, rx, saves)
    }

    fn submit(sim: &mut Sim, seq: u32, command: Option<WireCommand>) {
        sim.handle(Inbound::Request { session: SESSION, request: Request::SubmitCommand { client_seq: seq, command } });
    }

    fn tax(nation: u32, raw: i64) -> Option<WireCommand> {
        Some(WireCommand::SetIncomeTax { nation, rate_raw: Some(raw) })
    }

    #[test]
    fn an_accepted_command_applies_at_the_next_tick_and_is_logged() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        submit(&mut sim, 7, tax(1, 150_000));
        assert_eq!(drain(&mut rx), [Sent::Result { seq: 7, error: wire::CommandError::None, day: 0 }]);
        assert_ne!(sim.game.world().nations.income_tax_rate[1], Fixed::from_raw(150_000), "not before the tick");
        sim.tick();
        assert_eq!(sim.game.world().nations.income_tax_rate[1], Fixed::from_raw(150_000));
        let applied = Command::SetIncomeTax { nation: 1, rate: Fixed::from_raw(150_000) };
        assert_eq!(sim.log(), [save::SavedCommand { day: 0, player: Some(0), command: applied }]);
    }

    #[test]
    fn commands_are_checked_in_order_well_formed_permitted_valid() {
        let (mut sim, mut rx, _saves) = welcomed(Some(0));
        submit(&mut sim, 1, None);
        submit(&mut sim, 2, Some(WireCommand::SetIncomeTax { nation: 0, rate_raw: None }));
        // Out of range *and* another nation's: permission is checked first (D24).
        submit(&mut sim, 3, tax(1, 2_000_000));
        submit(&mut sim, 4, tax(0, 2_000_000));
        submit(&mut sim, 5, tax(0, 100_000));
        use wire::CommandError as E;
        let results: Vec<_> = drain(&mut rx)
            .into_iter()
            .map(|s| match s {
                Sent::Result { seq, error, .. } => (seq, error),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            results,
            [(1, E::Malformed), (2, E::Malformed), (3, E::NotPermitted), (4, E::RateOutOfRange), (5, E::None)]
        );
        sim.tick();
        assert_eq!(sim.log().len(), 1, "only the valid command applied");
    }

    #[test]
    fn scripted_commands_apply_first_on_their_day_and_are_logged_as_scripted() {
        let (mut sim, _rx, _saves) = welcomed(None);
        for _ in 0..360 {
            sim.tick();
        }
        // Day 360: the scenario raises Lowland's tax to 12%; the player's command follows.
        submit(&mut sim, 1, tax(0, 130_000));
        sim.tick();
        let log = sim.log();
        assert_eq!(log.len(), 2);
        assert_eq!((log[0].day, log[0].player), (360, None));
        assert_eq!((log[1].day, log[1].player), (360, Some(0)));
        assert_eq!(sim.game.world().nations.income_tax_rate[0], Fixed::from_raw(130_000), "the player's command wins");
    }

    #[test]
    fn a_session_that_stops_acknowledging_gets_at_most_the_window() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        for _ in 0..10 {
            sim.advance();
        }
        let updates: Vec<_> = drain(&mut rx).into_iter().filter(|s| matches!(s, Sent::Update { .. })).collect();
        assert_eq!(
            updates,
            [
                Sent::Update { day: 1, skipped: 0 },
                Sent::Update { day: 2, skipped: 0 },
                Sent::Update { day: 3, skipped: 0 }
            ]
        );
        // Acknowledging day 1 frees one slot: the latest day goes out, counting days 4–9 as skipped.
        sim.handle(Inbound::Request { session: SESSION, request: Request::Ack { day: 1 } });
        assert_eq!(drain(&mut rx), [Sent::Update { day: 10, skipped: 6 }]);
        // Nothing more is owed until another day runs.
        sim.handle(Inbound::Request { session: SESSION, request: Request::Ack { day: 10 } });
        assert_eq!(drain(&mut rx), []);
        sim.advance();
        assert_eq!(drain(&mut rx), [Sent::Update { day: 11, skipped: 0 }]);
    }

    #[test]
    fn speed_changes_are_announced_and_schedule_ticks() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        assert_eq!(sim.clock, Clock::Paused, "a new game starts paused");
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed::Normal } });
        assert_eq!(drain(&mut rx), [Sent::State(wire::Speed::Normal, 0)]);
        assert!(sim.clock.due().is_some());
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed::Paused } });
        assert_eq!(drain(&mut rx), [Sent::State(wire::Speed::Paused, 0)]);
        assert_eq!(sim.clock, Clock::Paused);
        // D22: a speed newer than this server is ignored, never a protocol error. The
        // session is told the unchanged state.
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed::Fast } });
        drain(&mut rx);
        let before = sim.clock;
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed(9) } });
        assert_eq!(drain(&mut rx), [Sent::State(wire::Speed::Fast, 0)]);
        assert_eq!(sim.clock, before);
    }

    #[test]
    fn the_clock_stops_when_the_player_leaves() {
        let (mut sim, _rx, _saves) = welcomed(None);
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed::Fast } });
        sim.handle(Inbound::Closed { session: SESSION });
        assert_eq!(sim.clock, Clock::Paused);
    }

    /// A multiplayer sim with room for `players`, nobody connected yet, and no
    /// sandbox (D24): every player holds a nation.
    fn multiplayer(players: u16) -> (Sim, SavesDir) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let saves = test_saves_dir();
        let mut config = crate::Config::local(&dir);
        config.saves_dir = saves.0.clone();
        config.max_players = players;
        config.sandbox = false;
        (Sim::new(pax_data::load_scenario(&dir).unwrap(), &config), saves)
    }

    /// Connects session `id` and says Hello for `nation`. Returns what it was sent.
    fn join(sim: &mut Sim, id: u64, nation: Option<u32>) -> (Receiver<Outbound>, Vec<Sent>) {
        let (conn, mut rx) = ConnHandle::for_test();
        sim.handle(Inbound::Connected { session: id, conn });
        let hello =
            Request::Hello { major: PROTOCOL_MAJOR, minor: 0, name: None, requested_nation: nation, resume_token: 0 };
        sim.handle(Inbound::Request { session: id, request: hello });
        let sent = drain(&mut rx);
        (rx, sent)
    }

    /// M4-1: players up to the limit, each with their own id and nation.
    #[test]
    fn players_join_up_to_the_limit_with_distinct_ids_and_nations() {
        let (mut sim, _saves) = multiplayer(2);
        let (_a, sent) = join(&mut sim, 1, Some(0));
        assert_eq!(sent, [Sent::Welcome { player: 0 }]);
        let (_b, sent) = join(&mut sim, 2, Some(0));
        assert_eq!(sent, [Sent::Rejected, Sent::Close], "nation 0 is taken");
        let (_b, sent) = join(&mut sim, 3, Some(1));
        assert_eq!(sent, [Sent::Welcome { player: 1 }]);
        sim.max_players = 3;
        let (_c, sent) = join(&mut sim, 4, None);
        assert_eq!(sent, [Sent::Rejected, Sent::Close], "no sandbox without --sandbox (D24)");
        sim.max_players = 2;
        let (_d, sent) = join(&mut sim, 5, Some(1));
        assert_eq!(sent, [Sent::Rejected, Sent::Close], "two players: the server is full");
        // Player 1 leaves; the next player takes the free id and nation.
        sim.handle(Inbound::Closed { session: 3 });
        let (_e, sent) = join(&mut sim, 6, Some(1));
        assert_eq!(sent, [Sent::Welcome { player: 1 }]);
    }

    /// M4-1: every player hears a speed change and who made it; each gets the days
    /// through their own window; one leaving doesn't stop the others' game, and the
    /// last one leaving does.
    #[test]
    fn players_share_the_clock_but_not_their_windows() {
        let (mut sim, _saves) = multiplayer(2);
        let (mut a, _) = join(&mut sim, 1, Some(0));
        let (mut b, _) = join(&mut sim, 2, Some(1));
        let speed = Request::SetSpeed { speed: wire::Speed::Fast };
        sim.handle(Inbound::Request { session: 1, request: speed });
        assert_eq!(drain(&mut a), [Sent::State(wire::Speed::Fast, 0)], "player 0 set it");
        assert_eq!(drain(&mut b), [Sent::State(wire::Speed::Fast, 0)]);
        for _ in 0..4 {
            sim.advance();
        }
        // A acknowledges everything, B nothing: B's window holds it at three updates.
        let updates = |sent: Vec<Sent>| sent.into_iter().filter(|s| matches!(s, Sent::Update { .. })).count();
        assert_eq!((updates(drain(&mut a)), updates(drain(&mut b))), (3, 3));
        sim.handle(Inbound::Request { session: 1, request: Request::Ack { day: 3 } });
        assert_eq!(drain(&mut a), [Sent::Update { day: 4, skipped: 0 }]);
        assert_eq!(drain(&mut b), []);
        // B leaves: A plays on.
        sim.handle(Inbound::Closed { session: 2 });
        assert_eq!(sim.clock.speed(), wire::Speed::Fast, "the others play on (D24)");
        assert_eq!(drain(&mut a), []);
        // A leaves too: nobody is watching, so the clock stops.
        sim.handle(Inbound::Closed { session: 1 });
        assert_eq!(sim.clock, Clock::Paused);
    }

    /// A load sends every player the new game's Welcome, each with their own seat.
    #[test]
    fn a_load_welcomes_every_player_again() {
        let (mut sim, _saves) = multiplayer(2);
        let (mut a, _) = join(&mut sim, 1, Some(0));
        let (mut b, _) = join(&mut sim, 2, Some(1));
        sim.handle(Inbound::Request { session: 1, request: Request::SaveGame { name: Some("s".into()) } });
        sim.handle(Inbound::Request { session: 1, request: Request::LoadGame { name: Some("s".into()) } });
        let a_sent = drain(&mut a);
        assert!(matches!(a_sent.as_slice(), [Sent::Saved { .. }, Sent::Welcome { player: 0 }]), "{a_sent:?}");
        assert_eq!(drain(&mut b), [Sent::Welcome { player: 1 }]);
    }

    fn speed(sim: &mut Sim, session: u64, speed: wire::Speed) {
        sim.handle(Inbound::Request { session, request: Request::SetSpeed { speed } });
    }

    /// M4-3: the first player is host. Only the host sets the speed, but anyone
    /// may pause; only the host saves and loads.
    #[test]
    fn only_the_host_sets_the_speed_saves_and_loads_but_anyone_may_pause() {
        let (mut sim, _saves) = multiplayer(2);
        let (mut host, _) = join(&mut sim, 1, Some(0));
        let (mut guest, _) = join(&mut sim, 2, Some(1));
        speed(&mut sim, 2, wire::Speed::Fast);
        assert_eq!(drain(&mut guest), [Sent::State(wire::Speed::Paused, 1)], "refused: told the unchanged speed");
        assert_eq!(drain(&mut host), [], "nothing changed for anyone else");
        speed(&mut sim, 1, wire::Speed::Fast);
        assert_eq!(drain(&mut guest), [Sent::State(wire::Speed::Fast, 0)]);
        drain(&mut host);
        speed(&mut sim, 2, wire::Speed::Paused);
        assert_eq!(drain(&mut host), [Sent::State(wire::Speed::Paused, 1)], "any player may pause");
        drain(&mut guest);
        for request in [Request::SaveGame { name: Some("g".into()) }, Request::LoadGame { name: Some("g".into()) }] {
            sim.handle(Inbound::Request { session: 2, request });
        }
        let errors: Vec<String> = drain(&mut guest)
            .into_iter()
            .map(|s| match s {
                Sent::Saved { error, .. } => error,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(errors, [NOT_HOST_SAVE, NOT_HOST_LOAD]);
        assert!(!sim.saves_dir.exists(), "a guest's save writes nothing");
    }

    /// The host leaving hands the role to the lowest remaining player id.
    #[test]
    fn the_host_role_passes_to_the_lowest_player_when_the_host_leaves() {
        let (mut sim, _saves) = multiplayer(2);
        let (_h, _) = join(&mut sim, 1, Some(0));
        let (mut b, _) = join(&mut sim, 2, Some(1));
        sim.handle(Inbound::Closed { session: 1 });
        assert_eq!(sim.sessions.host(), Some(2), "player 1 is now the lowest");
        speed(&mut sim, 2, wire::Speed::Fast);
        assert_eq!(drain(&mut b), [Sent::State(wire::Speed::Fast, 1)]);
    }

    /// On a dedicated server the admin is host by name, whenever they join.
    #[test]
    fn an_admin_name_makes_that_player_the_host() {
        let (mut sim, _saves) = multiplayer(2);
        sim.admin = Some("ada".into());
        let (_a, _) = join(&mut sim, 1, Some(0));
        assert_eq!(sim.sessions.host(), None, "the first player isn't host on a dedicated server");
        let (conn, mut rx) = ConnHandle::for_test();
        sim.handle(Inbound::Connected { session: 2, conn });
        let hello = Request::Hello {
            major: PROTOCOL_MAJOR,
            minor: 0,
            name: Some("ada".into()),
            requested_nation: Some(1),
            resume_token: 0,
        };
        sim.handle(Inbound::Request { session: 2, request: hello });
        assert_eq!(drain(&mut rx), [Sent::Welcome { player: 1 }]);
        assert_eq!(sim.sessions.host(), Some(2));
        // The admin leaving doesn't hand the role to someone else.
        sim.handle(Inbound::Closed { session: 2 });
        assert_eq!(sim.sessions.host(), None);
    }

    /// The host kicks a player: it gets a Goodbye, and whatever it still sends is
    /// dropped. Anyone else's kick is ignored.
    #[test]
    fn the_host_kicks_and_nobody_else_can() {
        let (mut sim, _saves) = multiplayer(2);
        let (mut host, _) = join(&mut sim, 1, Some(0));
        let (mut b, _) = join(&mut sim, 2, Some(1));
        sim.handle(Inbound::Request { session: 2, request: Request::Kick { player: 0 } });
        sim.handle(Inbound::Request { session: 1, request: Request::Kick { player: 0 } });
        assert_eq!((drain(&mut b), drain(&mut host)), (vec![], vec![]), "a guest can't kick, nor the host itself");
        sim.handle(Inbound::Request { session: 1, request: Request::Kick { player: 1 } });
        assert_eq!(drain(&mut b), [Sent::Goodbye("kicked by the host".into()), Sent::Close]);
        submit(&mut sim, 1, tax(1, 100_000));
        sim.handle(Inbound::Request {
            session: 2,
            request: Request::SubmitCommand { client_seq: 9, command: tax(1, 1) },
        });
        assert_eq!(drain(&mut b), [], "a kicked session can't act");
        // Its nation is free again, and so is its player id.
        sim.handle(Inbound::Closed { session: 2 });
        let (_d, sent) = join(&mut sim, 4, Some(1));
        assert_eq!(sent, [Sent::Welcome { player: 1 }]);
        drain(&mut host);
    }

    /// A load never widens a seat: a player whose nation the loaded game lacks is
    /// told why and leaves, instead of becoming a sandbox seat (D24).
    #[test]
    fn a_load_drops_a_player_whose_nation_is_gone() {
        let (mut sim, saves) = multiplayer(2);
        let (mut a, _) = join(&mut sim, 1, Some(0));
        let (mut b, _) = join(&mut sim, 2, Some(1));
        // A save of mini_valley, which has no nations, in the same saves directory.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mini_valley");
        let mut config = crate::Config::local(&dir);
        config.saves_dir = saves.0.clone();
        let mut other = Sim::new(pax_data::load_scenario(&dir).unwrap(), &config);
        let (mut o, _) = join(&mut other, 1, None);
        other.handle(Inbound::Request { session: 1, request: Request::SaveGame { name: Some("mv".into()) } });
        assert!(matches!(drain(&mut o).as_slice(), [Sent::Saved { error, .. }] if error.is_empty()));

        sim.handle(Inbound::Request { session: 1, request: Request::LoadGame { name: Some("mv".into()) } });
        for (rx, n) in [(&mut a, 0), (&mut b, 1)] {
            match drain(rx).as_slice() {
                [Sent::Goodbye(reason), Sent::Close] => assert!(reason.contains(&format!("no nation {n}")), "{reason}"),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!((sim.sessions.players(), sim.clock), (0, Clock::Paused), "nobody plays a sandbox seat now");
    }

    #[test]
    fn sandbox_needs_the_sandbox_flag() {
        let (mut sim, _saves) = multiplayer(2);
        sim.sandbox = false;
        assert_eq!(join(&mut sim, 1, None).1, [Sent::Rejected, Sent::Close]);
        assert_eq!(join(&mut sim, 2, Some(0)).1, [Sent::Welcome { player: 0 }]);
    }

    /// D11 pins the server's day to the harness's: with no players, `Sim::tick`
    /// reproduces each scenario's `golden.hashes` (`pax_cli verify`) day by day.
    #[test]
    fn the_server_reproduces_the_golden_hashes() {
        for name in ["mini_valley", "two_states"] {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios").join(name);
            let golden = pax_data::golden::read(&dir.join("golden.hashes")).unwrap();
            let saves = test_saves_dir();
            let mut config = crate::Config::local(&dir);
            config.saves_dir = saves.0.clone();
            let mut sim = Sim::new(pax_data::load_scenario(&dir).unwrap(), &config);
            for (day, &expected) in golden.iter().enumerate() {
                sim.tick();
                assert_eq!(sim.game.views().state_hash, expected, "{name}: day {} differs from golden", day + 1);
            }
            assert!(sim.log().iter().all(|l| l.player.is_none()), "{name}: only scripted commands applied");
            for c in sim.game.checkpoints() {
                assert_eq!(
                    c.state_hash,
                    golden[c.day as usize - 1],
                    "{name}: the day {} checkpoint differs from golden",
                    c.day
                );
            }
        }
    }

    /// D10: within a day, commands apply in `(player, sequence)` order, whatever
    /// order they arrived in (arrival order between players is a network race).
    #[test]
    fn commands_apply_in_stamp_order_not_arrival_order() {
        let (mut sim, _rx, _saves) = welcomed(None);
        let rate = |raw| Command::SetIncomeTax { nation: 0, rate: Fixed::from_raw(raw) };
        // Player 1's command arrives first; player 0's two follow.
        for (player, raw) in [(1, 100_000), (0, 110_000), (0, 120_000)] {
            sim.queue.stamp(SESSION, player, 0, rate(raw));
        }
        sim.tick();
        let applied: Vec<_> = sim.log().iter().map(|l| (l.player, l.command)).collect();
        assert_eq!(applied, [(Some(0), rate(110_000)), (Some(0), rate(120_000)), (Some(1), rate(100_000))]);
        // The last command in stamp order wins: player 1's.
        assert_eq!(sim.game.world().nations.income_tax_rate[0], Fixed::from_raw(100_000));
    }

    fn request(sim: &mut Sim, request: Request) {
        sim.handle(Inbound::Request { session: SESSION, request });
    }

    #[test]
    fn a_loaded_save_continues_exactly_like_the_original_game() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        submit(&mut sim, 1, tax(1, 175_000));
        for _ in 0..65 {
            sim.tick();
        }
        submit(&mut sim, 2, tax(0, 90_000));
        sim.tick();
        let (saved_day, saved_hash, saved_log) =
            (sim.game.world().day, sim.game.world().state_hash(), sim.log().to_vec());
        request(&mut sim, Request::SaveGame { name: Some("autumn".into()) });
        drain(&mut rx);
        // The original game runs on 20 more days.
        for _ in 0..20 {
            sim.tick();
        }
        let later_hash = sim.game.world().state_hash();

        request(&mut sim, Request::LoadGame { name: Some("autumn".into()) });
        assert_eq!(drain(&mut rx), [Sent::Welcome { player: 0 }], "a load answers with a new Welcome");
        assert_eq!((sim.game.world().day, sim.game.world().state_hash()), (saved_day, saved_hash));
        assert_eq!(sim.log(), saved_log.as_slice());
        assert_eq!(sim.clock, Clock::Paused, "a loaded game starts paused");
        // Continuing from the save reaches the same state as the original game did.
        for _ in 0..20 {
            sim.tick();
        }
        assert_eq!(sim.game.world().state_hash(), later_hash);
    }

    #[test]
    fn bad_save_names_are_refused_and_nothing_is_written() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        for name in [None, Some(""), Some("../escape"), Some("a b"), Some(&*"x".repeat(65))] {
            request(&mut sim, Request::SaveGame { name: name.map(str::to_owned) });
            match drain(&mut rx).as_slice() {
                [Sent::Saved { error, .. }] => assert!(!error.is_empty(), "{name:?} should be refused"),
                other => panic!("{other:?}"),
            }
        }
        assert!(!sim.saves_dir.exists(), "no file may be written for a refused name");
    }

    #[test]
    fn saves_are_listed_by_name() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        for name in ["zeta", "alpha", "mid_1"] {
            request(&mut sim, Request::SaveGame { name: Some(name.into()) });
        }
        drain(&mut rx);
        request(&mut sim, Request::ListSaves);
        assert_eq!(drain(&mut rx), [Sent::Saves(vec!["alpha".into(), "mid_1".into(), "zeta".into()])]);
    }

    #[test]
    fn a_damaged_or_missing_save_is_refused_and_the_game_is_untouched() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        for _ in 0..31 {
            sim.tick();
        }
        request(&mut sim, Request::SaveGame { name: Some("t".into()) });
        drain(&mut rx);
        // Damage the snapshot: flip one byte near the end (a nation's basket value).
        let snapshot = sim.saves_dir.join("t.world");
        let mut bytes = std::fs::read(&snapshot).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x40;
        std::fs::write(&snapshot, bytes).unwrap();
        let before = sim.game.world().state_hash();
        request(&mut sim, Request::LoadGame { name: Some("t".into()) });
        match drain(&mut rx).as_slice() {
            // Caught by the engine's table rules (the basket no longer sums to 1).
            [Sent::Saved { error, .. }] => assert!(error.contains("table rule"), "{error}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(sim.game.world().state_hash(), before);
        // A save whose snapshot is gone is refused too: no silent fallback.
        std::fs::remove_file(&snapshot).unwrap();
        request(&mut sim, Request::LoadGame { name: Some("t".into()) });
        assert!(matches!(drain(&mut rx).as_slice(), [Sent::Saved { error, .. }] if error.contains("t.world")));
        request(&mut sim, Request::LoadGame { name: Some("nope".into()) });
        assert!(matches!(drain(&mut rx).as_slice(), [Sent::Saved { error, .. }] if error.contains("there is no save")));
    }
}
