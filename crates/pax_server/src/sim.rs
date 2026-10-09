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
//! A multiplayer server starts in a lobby, where players claim nations and mark
//! themselves ready until the host starts the game (M4-2); single player starts at
//! once.

use flume::{Receiver, RecvTimeoutError, TryRecvError};
use std::path::{Path, PathBuf};
use std::time::Instant;

use pax_data::Scenario;
use pax_data::save;
use pax_protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, wire};
use tracing::{debug, info};

use crate::answer_limit::AnswerLimit;
use crate::clock::Clock;
use crate::commands;
use crate::encode::{self, WelcomeInfo};
use crate::game::Game;
use crate::net::{Inbound, Outbound};
use crate::queue::CommandQueue;
use crate::request::{Request, WireCommand};
use crate::session::{Claim, HostRule, LobbyEntry, OnLeave, Refusal, Seat, Session, SessionTable, Vacated};
use crate::view::{self, DayViews, MapPart, Subscription};

/// The lobby as players last saw it (`LobbyState` without a notice).
#[derive(Debug, PartialEq)]
struct LobbyView {
    players: Vec<LobbyEntry>,
    started: bool,
}

/// A `Hello`'s fields, borrowed from the request (D22, D24).
struct HelloFields<'a> {
    major: u16,
    minor: u16,
    name: Option<&'a str>,
    requested_nation: Option<u32>,
    resume_token: u64,
    password: Option<&'a crate::Secret>,
}

/// Where a server's game is (D24, M4-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// One player, no lobby: the game starts at once.
    SinglePlayer,
    /// Several players, before the host starts: claims and ready marks; no commands,
    /// no clock.
    Lobby,
    /// Several players, after the start. A load returns to the lobby (M4-5); an
    /// emptied server stays here, paused (D23), and players come back with their
    /// resume tokens (M4-4).
    Playing,
}

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
    /// How remote sessions are throttled (D24, M4-7).
    bandwidth: crate::Bandwidth,
    /// The temporary map-request limit (`Config::subscribe_answers_per_second`).
    subscribe_answers_per_second: u32,
    /// The server password (`Config::password`, D24, M4-6). The admin's is in the
    /// table's `HostRule::Admin`.
    password: Option<crate::Secret>,
    /// D24's command rate limit (`Config::commands_per_second`).
    commands_per_second: u32,
    /// Stop when the last player leaves (the client launched this server).
    exit_when_idle: bool,
    /// Set when the server should stop; [`Sim::handle`] returns it. A flag rather
    /// than a return value, so no path that ends a seat can drop it.
    stop: bool,
    /// Where the game is: single player, or a multiplayer lobby or game (M4-2).
    phase: Phase,
    /// The speed a fairness pause interrupted, restored when nobody is stalled any
    /// more. `None`: no fairness pause is holding the clock. Who is stalled is on the
    /// session rows.
    paused_for_fairness: Option<wire::Speed>,
    /// The player who last set the clock: `ServerState.changed_by`. A fairness pause
    /// and its resume are the server's doing and keep it (D24).
    last_changed_by: u16,
    /// The lobby as players last saw it; it is sent again only when it differs
    /// (M4-2). `None`: it must be sent (after a load's new Welcomes).
    last_lobby: Option<LobbyView>,
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
            // D24: the first player is host, or the admin on a dedicated server.
            sessions: SessionTable::new(config.admin.clone().map_or(HostRule::FirstPlayer, HostRule::Admin)),
            max_players: usize::from(config.max_players),
            sandbox: config.sandbox,
            bandwidth: config.bandwidth,
            subscribe_answers_per_second: config.subscribe_answers_per_second,
            password: config.password.clone(),
            commands_per_second: config.commands_per_second,
            exit_when_idle: config.exit_when_idle,
            stop: false,
            phase: if config.max_players > 1 { Phase::Lobby } else { Phase::SinglePlayer },
            paused_for_fairness: None,
            last_changed_by: 0,
            last_lobby: None,
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

    fn hello(&mut self, session: u64, hello: HelloFields<'_>) {
        let HelloFields { major, minor, name, requested_nation, resume_token, password } = hello;
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
        // The admin proves it with the admin password; it, or the server password,
        // admits a player to a server that has one (D24, M4-6).
        // `Secret`'s equality is constant-time.
        let matches = |expected: Option<&crate::Secret>| expected.is_some_and(|e| password == Some(e));
        let admin = self.sessions.admin();
        let admin_proved = matches(admin.map(|a| &a.password));
        // The admin's name without its password is an ordinary player: say so, or a
        // mistyped password shows only as `NotPermitted` later (NETWORK_PROTOCOL §6).
        if !admin_proved && admin.is_some_and(|a| name == Some(a.name.as_str())) {
            info!(session, "the admin's name without the admin password: not the host");
        }
        if self.password.is_some() && !admin_proved && !matches(self.password.as_ref()) {
            return self.reject(session, "wrong password");
        }
        // A player coming back to a started game with their token gets their seat (D24).
        if let Some(token) = std::num::NonZeroU64::new(resume_token) {
            match self.sessions.resume(session, token, admin_proved) {
                Ok(seat) => {
                    info!(session, player = seat.player, nation = ?seat.nation(), "resumed");
                    self.welcome(session, seat);
                }
                Err(refusal) => self.reject(session, &self.refusal(refusal)),
            }
            return;
        }
        // Without a nation: a sandbox seat if the server allows them, else, in the
        // lobby, a seat that claims a nation later (M4-2).
        let claim = match requested_nation {
            Some(n) => Claim::Nation(n),
            None if self.sandbox => Claim::Sandbox,
            None => Claim::Unclaimed,
        };
        if claim == Claim::Unclaimed && self.phase != Phase::Lobby {
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
        let seat = match self.sessions.sit(session, name.unwrap_or(""), claim, self.max_players, admin_proved) {
            Ok(seat) => seat,
            Err(refusal) => return self.reject(session, &self.refusal(refusal)),
        };
        let host = self.sessions.is_host(session);
        info!(session, name, player = seat.player, ?requested_nation, host, "welcomed");
        self.welcome(session, seat);
    }

    /// What a client is told when the table refuses it (`Rejected`, or a lobby
    /// notice). The table gives the reason; the sim knows the context.
    fn refusal(&self, refusal: Refusal) -> String {
        match refusal {
            Refusal::Full if self.max_players == 1 => {
                "server full: a single-player server accepts one client".to_owned()
            }
            Refusal::Full => format!("server full: all {} players are connected", self.max_players),
            Refusal::NationTaken { nation, player } => match self.game.world().nations.key.get(nation as usize) {
                Some(key) => format!("nation {nation} ({key}) is taken by player {player}"),
                None => format!("nation {nation} is taken by player {player}"),
            },
            Refusal::NoClaim => "claim a nation before you are ready".to_owned(),
            Refusal::UnknownToken => {
                "no seat is kept for this resume token (kicked, or the game was loaded)".to_owned()
            }
        }
    }

    /// Commands and the clock work: single player, or a multiplayer game the host
    /// started.
    fn started(&self) -> bool {
        self.phase != Phase::Lobby
    }

    /// Tells every player the lobby, if it differs from what they last saw (M4-2).
    /// Comparing with that snapshot, rather than flagging each change, means no path
    /// that changes the table can forget to. Only a multiplayer server has a lobby;
    /// it keeps sending the player list after the start.
    fn tell_lobby_if_changed(&mut self) {
        if self.phase == Phase::SinglePlayer {
            return;
        }
        let now = LobbyView { players: self.sessions.lobby(), started: self.started() };
        if self.last_lobby.as_ref() != Some(&now) {
            self.sessions.broadcast(&encode::lobby_state(&now.players, now.started, None));
            self.last_lobby = Some(now);
        }
    }

    /// Tells one player why their lobby request was refused, with the lobby as it is.
    fn lobby_notice(&self, session: u64, notice: &str) {
        debug!(session, notice, "lobby request refused");
        self.send(session, Outbound::Frame(encode::lobby_state(&self.sessions.lobby(), self.started(), Some(notice))));
    }

    /// The lobby is over once the game starts (M4-2): claims and ready marks only
    /// count before it. Returns whether the request may go on.
    fn admit_lobby_request(&self, session: u64) -> bool {
        match self.phase {
            Phase::Lobby => true,
            Phase::Playing => {
                self.lobby_notice(session, "the game has started");
                false
            }
            // Single player never sends LobbyState (NETWORK_PROTOCOL §3.8): a lobby
            // request is ignored there, as a refused Kick is (D24).
            Phase::SinglePlayer => {
                debug!(session, "ignored: single player has no lobby");
                false
            }
        }
    }

    fn claim_nation(&mut self, session: u64, nation: Option<u32>) {
        if !self.admit_lobby_request(session) {
            return;
        }
        let nations = self.game.world().nations.key.len();
        if let Some(n) = nation.filter(|&n| n as usize >= nations) {
            return self.lobby_notice(session, &format!("unknown nation {n}: the scenario has {nations}"));
        }
        match self.sessions.claim(session, nation) {
            Ok(()) => {}
            Err(refusal) => self.lobby_notice(session, &self.refusal(refusal)),
        }
    }

    fn set_ready(&mut self, session: u64, ready: bool) {
        if !self.admit_lobby_request(session) {
            return;
        }
        match self.sessions.set_ready(session, ready) {
            Ok(()) => {}
            Err(refusal) => self.lobby_notice(session, &self.refusal(refusal)),
        }
    }

    /// The host starts the game once every player is ready (D24). The clock stays
    /// paused: the host unpauses when everyone is looking.
    fn start_game(&mut self, session: u64) {
        if !self.admit_lobby_request(session) {
            return;
        }
        if !self.sessions.is_host(session) {
            return self.lobby_notice(session, "only the host can start the game");
        }
        if !self.sessions.all_ready() {
            return self.lobby_notice(session, "not every player is ready");
        }
        self.phase = Phase::Playing;
        info!(session, players = self.sessions.players(), "the game starts");
    }

    /// Replaces the session's subscription and answers with a `DayUpdate` for the
    /// current day, so a newly opened panel fills at once, even when paused (D22),
    /// or, within the temporary map-request limit's gap (`answer_limit`), a moment
    /// later through [`Self::flush`], for the latest subscription. The refresh
    /// bypasses the flow-control window: the client asked for it.
    fn subscribe(&mut self, session: u64, requested: Subscription) {
        self.subscribe_at(session, requested, Instant::now());
    }

    /// [`Self::subscribe`] at `now`, the time the limit sees (tests pass their own).
    /// The subscription changes at once; the answer, a full update with the map,
    /// goes out now, or, within the temporary map-request limit's gap
    /// (`answer_limit`), later through [`Self::flush`],
    /// for whatever the subscription is by then.
    fn subscribe_at(&mut self, session: u64, requested: Subscription, now: Instant) {
        let subscription = match requested.checked(self.game.world()) {
            Ok(s) => s,
            Err(reason) => return self.goodbye(session, &reason),
        };
        let Some(s) = self.sessions.get_mut(session) else { return };
        s.subscription = subscription;
        if s.answers.admit(now) {
            answer_subscription(s, &self.game.views(), self.clock.speed(), now);
        } else {
            debug!(session, "Subscribe answer deferred (the temporary map-request limit)");
        }
    }

    fn command_result(&self, session: u64, client_seq: u32, error: wire::CommandError) {
        let day = if error == wire::CommandError::None { self.game.world().day } else { 0 };
        self.send(session, Outbound::Frame(encode::command_result(client_seq, error, day)));
    }

    /// Checks a command on arrival and queues it for the start of the next tick
    /// (NETWORK_PROTOCOL §5). The checks run in this order:
    /// 1. well-formed;
    /// 2. the game has started (`NotStarted` in the lobby, M4-2);
    /// 3. permission: the session's nation (D24; sandbox allows all);
    /// 4. `World::validate`, the single validity rule (D21).
    ///
    /// Every submission gets exactly one `CommandResult` now. A second one, an error,
    /// follows only if the command fails when it is applied.
    fn submit(&mut self, session: u64, client_seq: u32, command: Option<WireCommand>) {
        let Some(seat) = self.sessions.seat(session) else { return };
        // D24's rate limit comes before anything else: a flood costs the server little.
        if !self.sessions.admit_command(session, Instant::now(), self.commands_per_second) {
            return self.command_result(session, client_seq, wire::CommandError::RateLimited);
        }
        let command = match command.ok_or(wire::CommandError::Malformed).and_then(commands::to_engine) {
            Ok(c) => c,
            Err(e) => return self.command_result(session, client_seq, e),
        };
        if !self.started() {
            return self.command_result(session, client_seq, wire::CommandError::NotStarted);
        }
        if !seat.commands(commands::nation_of(&command)) {
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
        // The clock waits for the game to start (M4-2).
        let permitted = speed == wire::Speed::Paused || (self.sessions.is_host(session) && self.started());
        match Clock::at(speed, Instant::now()).filter(|_| permitted) {
            // `set_clock` holds a running speed back while anyone is stalled (D24).
            Some(clock) => self.set_clock(clock, Some(seat.player)),
            None => {
                debug!(session, speed = speed.0, permitted, "ignored: an unknown speed, or not the host");
                self.send(session, Outbound::Frame(self.server_state()));
            }
        }
    }

    /// The clock as a `ServerState` frame: the speed, the player who last set it, and
    /// the players a fairness pause waits for (D24).
    fn server_state(&self) -> Vec<u8> {
        encode::server_state(self.game.world().day, self.clock.speed(), self.last_changed_by, &self.waiting_for())
    }

    /// Who the game waits for (D24): the stalled players, in a running game only. A
    /// silent player in the lobby holds up nothing until the game starts. The one
    /// definition; the clock and `ServerState` both use it.
    fn waiting_for(&self) -> Vec<u16> {
        if self.phase == Phase::Playing { self.sessions.waiting_for() } else { Vec::new() }
    }

    /// A player has been silent past the pause threshold (D24): a running game pauses
    /// for everyone, waiting for them. Only a started game with a seat counts.
    /// The stall is recorded whatever the phase (the network reports it only once);
    /// only a running game pauses for it. A game that starts, or is set going, while
    /// a player is stalled waits for them the same way (`set_speed`).
    fn stalled(&mut self, session: u64) {
        if !self.sessions.stall(session) || self.phase != Phase::Playing || self.sessions.seat(session).is_none() {
            return;
        }
        info!(session, "waiting for a silent player");
        let speed = self.clock.speed();
        if speed != wire::Speed::Paused && self.paused_for_fairness.is_none() {
            self.paused_for_fairness = Some(speed);
        }
        // The server's doing, not a player's: `changed_by` keeps the last player.
        self.set_clock(Clock::Paused, None);
    }

    /// Someone the fairness pause waited for is back, or their seat ended. When
    /// nobody is stalled any more, the clock gets back the speed it had (D24).
    fn fairness_changed(&mut self) {
        if self.phase != Phase::Playing {
            return;
        }
        if self.waiting_for().is_empty()
            && let Some(clock) = self.paused_for_fairness.take().and_then(|s| Clock::at(s, Instant::now()))
        {
            info!("nobody is silent any more; the game resumes");
            self.set_clock(clock, None);
        } else {
            self.sessions.broadcast(&self.server_state());
        }
    }

    /// The only place the speed changes: sets the clock and tells every welcomed
    /// session (`ServerState`, D23). `by`: the player who set it, or `None` when the
    /// server did (a fairness pause's resume, the last player leaving), which keeps
    /// `changed_by` at the last player who did.
    ///
    /// It also keeps D24's fairness rule: the clock never runs while anyone is
    /// stalled. A running speed asked for then is the one the game resumes at, and a
    /// player pausing means it stays paused when they're back.
    fn set_clock(&mut self, clock: Clock, by: Option<u16>) {
        if let Some(player) = by {
            self.last_changed_by = player;
        }
        let speed = clock.speed();
        if speed != wire::Speed::Paused && !self.waiting_for().is_empty() {
            self.paused_for_fairness = Some(speed);
            self.clock = Clock::Paused;
        } else {
            if speed == wire::Speed::Paused && by.is_some() {
                self.paused_for_fairness = None;
            }
            self.clock = clock;
        }
        info!(?speed, ?by, "speed changed");
        self.sessions.broadcast(&self.server_state());
    }

    /// The session processed the update for `day`: everything up to it leaves the
    /// window. If days ran while the window was full, the latest day goes out now.
    fn ack(&mut self, session: u64, day: u64) {
        self.ack_at(session, day, Instant::now());
    }

    /// [`Self::ack`] at `now`, the time the throttle sees (tests pass their own).
    fn ack_at(&mut self, session: u64, day: u64, now: Instant) {
        let Some(s) = self.sessions.get_mut(session) else { return };
        if s.window.ack(day) {
            send_update(s, &self.game.views(), self.clock.speed(), now);
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
        self.advance_at(Instant::now());
    }

    /// [`Self::advance`] at `now`, the time the throttle sees (tests pass their own).
    fn advance_at(&mut self, now: Instant) {
        self.tick();
        let due: Vec<u64> =
            self.sessions.welcomed_mut().filter_map(|(id, s)| s.window.day_ran().then_some(id)).collect();
        if !due.is_empty() {
            // Build the views only when someone will see them.
            let views = self.game.views();
            for id in due {
                if let Some(s) = self.sessions.get_mut(id) {
                    send_update(s, &views, self.clock.speed(), now);
                }
            }
        }
    }

    /// Changes the command rate limit mid-run: the hostile-input test fuzzes the
    /// refusal path with it. A running server's limit is fixed (`Config`).
    #[cfg(test)]
    pub(crate) fn set_commands_per_second(&mut self, limit: u32) {
        self.commands_per_second = limit;
    }

    /// When the next update a throttle held may go out (D24, M4-7); `None` if none
    /// is owed.
    pub(crate) fn next_flush(&self) -> Option<Instant> {
        self.sessions.held_updates().chain(self.sessions.pending_answers()).map(|(_, at)| at).min()
    }

    /// Sends every held update whose time has come: the latest day, which a pause or a
    /// slow speed would otherwise keep from a remote session.
    pub(crate) fn flush(&mut self, now: Instant) {
        let answers: Vec<u64> =
            self.sessions.pending_answers().filter(|&(_, at)| at <= now).map(|(id, _)| id).collect();
        let updates: Vec<u64> = self.sessions.held_updates().filter(|&(_, at)| at <= now).map(|(id, _)| id).collect();
        if answers.is_empty() && updates.is_empty() {
            return;
        }
        let views = self.game.views();
        // Deferred Subscribe answers first: each counts as an update, so a held day
        // for the same session then waits its own gap.
        for id in answers {
            if let Some(s) = self.sessions.get_mut(id) {
                answer_subscription(s, &views, self.clock.speed(), now);
            }
        }
        for id in updates {
            if let Some(s) = self.sessions.get_mut(id) {
                send_update(s, &views, self.clock.speed(), now);
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
        self.paused_for_fairness = None;
        // The new Welcomes reset every client: the lobby goes out again after them.
        self.last_lobby = None;
        // Not `set_clock`: the `Welcome` below announces the speed with the new game.
        self.clock = Clock::Paused;
        let nations = self.game.world().nations.key.len();
        // A load drops every kept seat, whatever the phase: they hold the old game's
        // nations, so an old resume token reclaims nothing in the new one (D24).
        self.sessions.forget_all();
        // A seat is never widened to sandbox, which commands every nation. Each phase
        // has its own rule for a seat whose nation the loaded game lacks:
        match self.phase {
            // A multiplayer load goes through the lobby (D24, M4-5): players keep the
            // claims the loaded game has and re-claim the others; nobody is dropped.
            Phase::Lobby | Phase::Playing => {
                self.phase = Phase::Lobby;
                self.sessions.load_into_lobby(nations);
            }
            // Single player has no lobby: that seat ends.
            Phase::SinglePlayer => {
                for id in self.sessions.welcomed() {
                    let seat = self.sessions.seat(id).expect("welcomed sessions have a seat");
                    if let Some(n) = seat.nation().filter(|&n| n as usize >= nations) {
                        self.goodbye(id, &format!("the loaded game has no nation {n}, which you played"));
                        if let Some(v) = self.sessions.unseat(id) {
                            self.seat_ended(id, v);
                        }
                    }
                }
            }
        }
        // Then every remaining player gets the new game's Welcome, in session order
        // (NETWORK_PROTOCOL §3), and once the table is final, the lobby.
        for id in self.sessions.welcomed() {
            let seat = self.sessions.seat(id).expect("welcomed sessions have a seat");
            let Some(s) = self.sessions.get_mut(id) else { continue };
            s.restart_updates();
            self.welcome(id, seat);
        }
    }

    /// The host ends another player's session (D24). Anything else is ignored: a
    /// non-host's kick, the host kicking itself, a player who isn't connected.
    fn kick(&mut self, session: u64, player: u16) {
        if !self.sessions.is_host(session) {
            return debug!(session, player, "ignored kick: not the host");
        }
        // An away player's kept seat is dropped, which frees their nation (D24).
        if self.sessions.forget(player) {
            info!(session, player, "kicked an away player");
            return;
        }
        let Some(target) = self.sessions.session_of(player).filter(|&t| t != session) else {
            return debug!(session, player, "ignored kick");
        };
        info!(session, target, player, "kicked by the host");
        self.goodbye(target, "kicked by the host");
        // Unseated now, so whatever it still sends is dropped; its row goes when its
        // connection closes.
        if let Some(v) = self.sessions.unseat(target) {
            self.seat_ended(target, v);
        }
    }

    /// A player's seat ended: they left, were kicked, or a load dropped them. The
    /// table has already passed the host role on (D24). When the last player
    /// leaves, nobody is watching, so the clock stops (D23), and a server the
    /// client launched stops too. Every path that ends a seat comes through here.
    /// It doesn't tell the lobby: `Sim::handle` does, once per event, after the
    /// table's last change, so a batch of changes (a load) never shows half done.
    fn seat_ended(&mut self, session: u64, v: Vacated) {
        if v.was_stalled {
            self.fairness_changed();
        }
        if let Some(next) = v.new_host {
            info!(session = next, "the host left; the host is now this session");
        }
        if v.last {
            info!(session, player = v.seat.player, "the last player left");
            self.set_clock(Clock::Paused, None);
            if self.exit_when_idle {
                info!("the client left; exiting (--exit-when-idle)");
                self.stop = true;
            }
        }
    }

    fn welcome(&self, session: u64, seat: Seat) {
        let info = WelcomeInfo {
            player: seat.player,
            // The wire uses 0 for "no token" (Hello: a new session).
            resume_token: self.sessions.token(session).map_or(0, std::num::NonZeroU64::get),
            nation: seat.nation(),
            scenario: &self.game.scenario().name,
            content_hash: self.game.scenario().content_hash,
            map: self.game.scenario().map.as_ref(),
            speed: self.clock.speed(),
        };
        self.send(session, Outbound::Frame(encode::welcome(self.game.world(), &info)));
    }

    /// Skips the lobby, for tests that exercise the game itself.
    #[cfg(test)]
    pub(crate) fn start_without_lobby(&mut self) {
        self.phase = Phase::Playing;
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
            Inbound::Connected { session, conn } => {
                let answers = AnswerLimit::new(self.subscribe_answers_per_second);
                self.sessions.connect(session, conn, self.bandwidth, answers);
            }
            Inbound::Request {
                session,
                request: Request::Hello { major, minor, name, requested_nation, resume_token, password },
            } => {
                let password = password.as_ref();
                let name = name.as_deref();
                self.hello(session, HelloFields { major, minor, name, requested_nation, resume_token, password });
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
                Request::ClaimNation { nation } => self.claim_nation(session, nation),
                Request::SetReady { ready } => self.set_ready(session, ready),
                Request::StartGame => self.start_game(session),
                // Hello is handled before the admission choke point; Ping by the network task.
                Request::Hello { .. } | Request::Ping { .. } => {}
            },
            Inbound::Closed { session } => {
                // The others play on (D24); see `seat_ended`.
                // In a started multiplayer game the seat waits for the resume token.
                let on_leave = if self.phase == Phase::Playing { OnLeave::KeepSeat } else { OnLeave::EndSeat };
                if let Some(v) = self.sessions.remove(session, on_leave) {
                    self.seat_ended(session, v);
                }
            }
            Inbound::Stalled { session } => self.stalled(session),
            Inbound::Resumed { session } => {
                if self.sessions.unstall(session) {
                    self.fairness_changed();
                }
            }
            Inbound::Shutdown => self.stop = true,
            #[cfg(test)]
            Inbound::Crash => panic!("injected crash for a test"),
        }
        // The lobby, once per event, after its last change, and only if it changed.
        self.tell_lobby_if_changed();
        !self.stop
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

/// Answers `s`'s `Subscribe` at `now`: a full update for its current subscription,
/// map included, outside the flow-control window (D23). It counts as an update for
/// the session's throttle, so the next day keeps its distance (D24, M4-7), and as an
/// answer for the temporary map-request limit.
fn answer_subscription(s: &mut Session, views: &DayViews<'_>, speed: wire::Speed, now: Instant) {
    s.throttle.resubscribed(now);
    s.answers.answered(now);
    s.conn.send(Outbound::Frame(view::day_update(views, &s.subscription, speed, 0, MapPart::Include)));
}

/// Sends `s` an update for the current day and puts it in the session's window.
/// Unless the session's throttle holds it until [`Sim::flush`] (a remote session,
/// D24), with or without the map as the throttle says.
fn send_update(s: &mut Session, views: &DayViews<'_>, speed: wire::Speed, now: Instant) {
    if !s.throttle.admit(now) {
        return;
    }
    let skipped = s.window.sent(views.world.day);
    let map = s.throttle.sent(now);
    s.conn.send(Outbound::Frame(view::day_update(views, &s.subscription, speed, skipped, map)));
}

/// The sim thread's loop: handle everything already waiting, tick if due, otherwise
/// sleep until the next event or the next tick. Draining first means input is never
/// starved, even at `Fastest`.
pub(crate) fn run(sim: &mut Sim, inbound: Receiver<Inbound>) {
    loop {
        let event = match inbound.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Disconnected) => break,
            // Nothing waiting: send held updates and tick if due, otherwise sleep until
            // an event, the tick or the next held update (M4-7).
            Err(TryRecvError::Empty) => {
                let now = Instant::now();
                if sim.next_flush().is_some_and(|at| at <= now) {
                    sim.flush(now);
                }
                let due = sim.clock.due();
                if due.is_some_and(|due| now >= due) {
                    sim.advance();
                    sim.clock.ticked(Instant::now());
                    continue;
                }
                // A flush sends everything due, so what's left is in the future; a
                // past one would only spin the loop, so it never sets the deadline.
                let flush = sim.next_flush().filter(|&at| at > now);
                debug_assert_eq!(flush, sim.next_flush(), "a flush leaves nothing due");
                match [due, flush].into_iter().flatten().min() {
                    Some(deadline) => match inbound.recv_deadline(deadline) {
                        Ok(event) => event,
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    },
                    None => match inbound.recv() {
                        Ok(event) => event,
                        Err(_) => break,
                    },
                }
            }
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
    #[derive(Clone, Debug, PartialEq)]
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
        /// A `ServerState` while a fairness pause waits for these players (D24).
        Waiting(wire::Speed, Vec<u16>),
        Saved {
            name: String,
            error: String,
        },
        Saves(Vec<String>),
        /// `LobbyState`: who is ready (in player order), whether started, the notice.
        Lobby {
            ready: Vec<bool>,
            started: bool,
            notice: Option<String>,
        },
        Close,
    }

    /// What the session was sent, apart from lobby updates (see [`drain_all`]).
    fn drain(rx: &mut Receiver<Outbound>) -> Vec<Sent> {
        drain_all(rx).into_iter().filter(|s| !matches!(s, Sent::Lobby { .. })).collect()
    }

    fn drain_all(rx: &mut Receiver<Outbound>) -> Vec<Sent> {
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
                        match s.waiting_for().map(|w| w.iter().collect::<Vec<u16>>()).unwrap_or_default() {
                            waiting if waiting.is_empty() => Sent::State(s.speed(), s.changed_by()),
                            waiting => Sent::Waiting(s.speed(), waiting),
                        }
                    } else if let Some(r) = m.payload_as_save_result() {
                        Sent::Saved {
                            name: r.name().unwrap_or_default().into(),
                            error: r.error().unwrap_or_default().into(),
                        }
                    } else if let Some(l) = m.payload_as_save_list() {
                        Sent::Saves(l.names().unwrap().iter().map(str::to_owned).collect())
                    } else if let Some(l) = m.payload_as_lobby_state() {
                        Sent::Lobby {
                            ready: l.players().unwrap().iter().map(|p| p.ready()).collect(),
                            started: l.started(),
                            notice: l.notice().map(str::to_owned),
                        }
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
            password: None,
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
        let hello = Request::Hello {
            major: PROTOCOL_MAJOR,
            minor: 0,
            name: None,
            requested_nation: nation,
            resume_token: 0,
            password: None,
        };
        sim.handle(Inbound::Request { session: id, request: hello });
        let sent = drain(&mut rx);
        (rx, sent)
    }

    /// Every listed session marks itself ready, and session 1 (the host) starts the
    /// game (M4-2).
    fn start(sim: &mut Sim, sessions: &[u64]) {
        for &id in sessions {
            sim.handle(Inbound::Request { session: id, request: Request::SetReady { ready: true } });
        }
        sim.handle(Inbound::Request { session: 1, request: Request::StartGame });
        assert_eq!(sim.phase, Phase::Playing, "every player was ready");
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
        start(&mut sim, &[1, 2]);
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
        start(&mut sim, &[1, 2]);
        speed(&mut sim, 2, wire::Speed::Fast);
        assert_eq!(
            drain(&mut guest),
            [Sent::State(wire::Speed::Paused, 0)],
            "refused: told the unchanged speed, and who last set it"
        );
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
        start(&mut sim, &[1, 2]);
        sim.handle(Inbound::Closed { session: 1 });
        assert_eq!(sim.sessions.host(), Some(2), "player 1 is now the lowest");
        speed(&mut sim, 2, wire::Speed::Fast);
        assert_eq!(drain(&mut b), [Sent::State(wire::Speed::Fast, 1)]);
    }

    /// On a dedicated server the admin is host by name, whenever they join.
    #[test]
    fn an_admin_name_makes_that_player_the_host() {
        let (mut sim, _saves) = multiplayer(2);
        // As `Sim::new` sets it for `--admin ada --admin-password-file …`.
        let admin = crate::Admin { name: "ada".into(), password: crate::Secret::new("s3cret") };
        sim.sessions = SessionTable::new(HostRule::Admin(admin));
        let (_a, _) = join(&mut sim, 1, Some(0));
        assert_eq!(sim.sessions.host(), None, "the first player isn't host on a dedicated server");
        let ada = |sim: &mut Sim, id: u64, password: Option<&str>| {
            let (conn, mut rx) = ConnHandle::for_test();
            sim.handle(Inbound::Connected { session: id, conn });
            let hello = Request::Hello {
                major: PROTOCOL_MAJOR,
                minor: 0,
                name: Some("ada".into()),
                requested_nation: Some(1),
                resume_token: 0,
                password: password.map(crate::Secret::new),
            };
            sim.handle(Inbound::Request { session: id, request: hello });
            drain(&mut rx)
        };
        // The name alone (M4-6): a player like any other, not the host.
        assert_eq!(ada(&mut sim, 2, None), [Sent::Welcome { player: 1 }]);
        assert_eq!(sim.sessions.host(), None, "a name is not proof");
        sim.handle(Inbound::Closed { session: 2 });
        assert_eq!(ada(&mut sim, 3, Some("s3cret")), [Sent::Welcome { player: 1 }]);
        assert_eq!(sim.sessions.host(), Some(3));
        // The admin leaving doesn't hand the role to someone else.
        sim.handle(Inbound::Closed { session: 3 });
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

    /// Saves mini_valley, which has no nations, into `saves`, for a load that every
    /// nation-holding seat lacks.
    fn save_mini_valley(saves: &SavesDir) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mini_valley");
        let mut config = crate::Config::local(&dir);
        config.saves_dir = saves.0.clone();
        let mut other = Sim::new(pax_data::load_scenario(&dir).unwrap(), &config);
        let (mut o, _) = join(&mut other, 1, None);
        other.handle(Inbound::Request { session: 1, request: Request::SaveGame { name: Some("mv".into()) } });
        assert!(matches!(drain(&mut o).as_slice(), [Sent::Saved { error, .. }] if error.is_empty()));
    }

    /// M4-5: a multiplayer load goes back through the lobby. Every player gets the new
    /// game's Welcome; a claim the loaded game lacks becomes unclaimed (never
    /// sandbox), nobody is ready, and nothing plays until the host starts again.
    #[test]
    fn a_multiplayer_load_goes_back_through_the_lobby() {
        let (mut sim, saves) = multiplayer(2);
        let (mut a, _) = join(&mut sim, 1, Some(0));
        let (mut b, _) = join(&mut sim, 2, Some(1));
        start(&mut sim, &[1, 2]);
        save_mini_valley(&saves);
        sim.handle(Inbound::Request { session: 1, request: Request::LoadGame { name: Some("mv".into()) } });
        for (rx, player) in [(&mut a, 0), (&mut b, 1)] {
            let sent = drain_all(rx);
            assert!(sent.contains(&Sent::Welcome { player }), "{sent:?}");
            assert!(
                sent.contains(&Sent::Lobby { ready: vec![false, false], started: false, notice: None }),
                "{sent:?}"
            );
        }
        assert_eq!(sim.phase, Phase::Lobby);
        for id in [1, 2] {
            assert_eq!(sim.sessions.seat(id).map(|s| s.claim), Some(Claim::Unclaimed), "never widened to sandbox");
        }
        submit(&mut sim, 3, tax(0, 100_000));
        assert!(matches!(drain(&mut a).as_slice(), [Sent::Result { error: wire::CommandError::NotStarted, .. }]));
    }

    /// A load drops every kept seat: an old resume token reclaims nothing in the
    /// loaded game (D24, NETWORK_PROTOCOL §3.9), and the seat's nation is free.
    #[test]
    fn a_load_drops_the_seats_kept_for_tokens() {
        let (mut sim, _saves) = multiplayer(2);
        let (mut a, _) = join(&mut sim, 1, Some(0));
        let (_b, _) = join(&mut sim, 2, Some(1));
        start(&mut sim, &[1, 2]);
        let token = sim.sessions.token(2).expect("a seated player has a token").get();
        sim.handle(Inbound::Request { session: 1, request: Request::SaveGame { name: Some("s".into()) } });
        sim.handle(Inbound::Closed { session: 2 });
        sim.handle(Inbound::Request { session: 1, request: Request::LoadGame { name: Some("s".into()) } });
        drain(&mut a);
        let (conn, mut rx) = ConnHandle::for_test();
        sim.handle(Inbound::Connected { session: 3, conn });
        let hello = Request::Hello {
            major: PROTOCOL_MAJOR,
            minor: 0,
            name: None,
            requested_nation: None,
            resume_token: token,
            password: None,
        };
        sim.handle(Inbound::Request { session: 3, request: hello });
        assert_eq!(drain(&mut rx), [Sent::Rejected, Sent::Close], "the kept seat went with the old game");
        assert_eq!(join(&mut sim, 4, Some(1)).1, [Sent::Welcome { player: 1 }], "its nation is free");
    }

    /// In single player there is no lobby: a seat whose nation the loaded game lacks
    /// ends, and a server the client launched stops with it (D23).
    #[test]
    fn a_single_player_load_without_the_nation_ends_the_seat() {
        let (mut sim, mut rx, saves) = welcomed(Some(0));
        save_mini_valley(&saves);
        sim.exit_when_idle = true;
        let keep_running =
            sim.handle(Inbound::Request { session: SESSION, request: Request::LoadGame { name: Some("mv".into()) } });
        assert!(!keep_running, "the load ended the only seat: the server stops");
        assert!(matches!(drain(&mut rx).as_slice(), [Sent::Goodbye(r), Sent::Close] if r.contains("no nation 0")));
    }

    /// A load tells each player the lobby once, after their new Welcome and with the
    /// table final, so nothing they decode mixes two games. Since M4-5 a multiplayer
    /// load drops nobody: the player whose nation is gone is unclaimed.
    #[test]
    fn a_load_tells_the_lobby_once_when_the_table_is_final() {
        let (mut sim, saves) = multiplayer(2);
        sim.sandbox = true;
        let (_a, _) = join(&mut sim, 1, Some(0));
        let (mut b, _) = join(&mut sim, 2, None); // a sandbox seat survives any load
        drain_all(&mut b);
        // A save of mini_valley, which has no nations.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mini_valley");
        let mut config = crate::Config::local(&dir);
        config.saves_dir = saves.0.clone();
        let mut other = Sim::new(pax_data::load_scenario(&dir).unwrap(), &config);
        let (mut o, _) = join(&mut other, 1, None);
        other.handle(Inbound::Request { session: 1, request: Request::SaveGame { name: Some("mv".into()) } });
        drain(&mut o);
        sim.handle(Inbound::Request { session: 1, request: Request::LoadGame { name: Some("mv".into()) } });
        let sent = drain_all(&mut b);
        assert_eq!(
            sent,
            [Sent::Welcome { player: 1 }, Sent::Lobby { ready: vec![false, false], started: false, notice: None }],
            "the new game's Welcome, then one lobby with every player"
        );
    }

    #[test]
    fn sandbox_needs_the_sandbox_flag() {
        // In single player, or once a game has started, a Hello without a nation is a
        // sandbox seat or nothing.
        let (mut sim, _saves) = multiplayer(2);
        sim.start_without_lobby();
        assert_eq!(join(&mut sim, 1, None).1, [Sent::Rejected, Sent::Close]);
        assert_eq!(join(&mut sim, 2, Some(0)).1, [Sent::Welcome { player: 0 }]);
        // In a lobby it is a seat that claims a nation later (M4-2), not a sandbox.
        let (mut sim, _saves) = multiplayer(2);
        assert_eq!(join(&mut sim, 1, None).1, [Sent::Welcome { player: 0 }]);
        let seat = sim.sessions.seat(1).unwrap();
        assert!(seat.claim == Claim::Unclaimed && !seat.commands(0) && !seat.commands(1), "it commands nothing");
    }

    /// M4-2: the lobby. Players claim nations and mark themselves ready; only the
    /// host starts, and only when everyone is ready. Until then nothing plays.
    #[test]
    fn the_lobby_holds_the_game_until_the_host_starts_it() {
        let (mut sim, _saves) = multiplayer(2);
        let (mut a, _) = join(&mut sim, 1, None);
        let (mut b, _) = join(&mut sim, 2, None);
        let lobby = |rx: &mut Receiver<Outbound>| {
            drain_all(rx).into_iter().filter(|s| matches!(s, Sent::Lobby { .. })).collect::<Vec<_>>()
        };
        lobby(&mut a);
        lobby(&mut b);
        let req = |sim: &mut Sim, session: u64, request: Request| sim.handle(Inbound::Request { session, request });
        // Before the start: commands are NotStarted, and the clock stays paused.
        req(&mut sim, 1, Request::SubmitCommand { client_seq: 1, command: tax(0, 100_000) });
        assert_eq!(drain(&mut a), [Sent::Result { seq: 1, error: wire::CommandError::NotStarted, day: 0 }]);
        req(&mut sim, 1, Request::SetSpeed { speed: wire::Speed::Fast });
        assert_eq!(drain(&mut a), [Sent::State(wire::Speed::Paused, 0)]);
        // Claims: a taken nation is refused with a notice to the asker only.
        req(&mut sim, 1, Request::ClaimNation { nation: Some(0) });
        req(&mut sim, 2, Request::ClaimNation { nation: Some(0) });
        assert!(
            matches!(lobby(&mut b).last(), Some(Sent::Lobby { notice: Some(n), .. }) if n.contains("taken by player 0"))
        );
        lobby(&mut a);
        // Not ready without a nation; the host can't start before everyone is ready.
        req(&mut sim, 2, Request::SetReady { ready: true });
        assert!(
            matches!(lobby(&mut b).as_slice(), [Sent::Lobby { notice: Some(n), .. }] if n.contains("claim a nation"))
        );
        req(&mut sim, 2, Request::ClaimNation { nation: Some(1) });
        req(&mut sim, 1, Request::SetReady { ready: true });
        req(&mut sim, 1, Request::StartGame);
        assert!(
            matches!(lobby(&mut a).last(), Some(Sent::Lobby { notice: Some(n), started: false, .. }) if n.contains("not every player is ready"))
        );
        req(&mut sim, 2, Request::SetReady { ready: true });
        req(&mut sim, 2, Request::StartGame);
        assert!(
            matches!(lobby(&mut b).last(), Some(Sent::Lobby { notice: Some(n), .. }) if n.contains("only the host"))
        );
        lobby(&mut a);
        req(&mut sim, 1, Request::StartGame);
        let started = Sent::Lobby { ready: vec![true, true], started: true, notice: None };
        assert_eq!((lobby(&mut a), lobby(&mut b)), (vec![started.clone()], vec![started]));
        // Now the game plays, each player commanding their claimed nation.
        req(&mut sim, 2, Request::SubmitCommand { client_seq: 2, command: tax(1, 100_000) });
        assert_eq!(drain(&mut b), [Sent::Result { seq: 2, error: wire::CommandError::None, day: 0 }]);
        req(&mut sim, 2, Request::ClaimNation { nation: Some(0) });
        assert!(matches!(lobby(&mut b).as_slice(), [Sent::Lobby { notice: Some(n), .. }] if n.contains("has started")));
    }

    fn event(sim: &mut Sim, event: Inbound) {
        sim.handle(event);
    }

    /// M4-4: a silent player pauses a running game for everyone, which resumes at
    /// its speed when they speak again (D24's fairness pause).
    #[test]
    fn a_silent_player_pauses_the_game_until_they_are_back() {
        let (mut sim, _saves) = multiplayer(2);
        let (mut a, _) = join(&mut sim, 1, Some(0));
        let (mut b, _) = join(&mut sim, 2, Some(1));
        start(&mut sim, &[1, 2]);
        speed(&mut sim, 1, wire::Speed::Fast);
        drain(&mut a);
        drain(&mut b);
        event(&mut sim, Inbound::Stalled { session: 2 });
        assert_eq!(sim.clock, Clock::Paused);
        assert_eq!(drain(&mut a), [Sent::Waiting(wire::Speed::Paused, vec![1])], "waiting for player 1");
        event(&mut sim, Inbound::Resumed { session: 2 });
        assert_eq!(sim.clock.speed(), wire::Speed::Fast, "back at its speed");
        assert_eq!(drain(&mut a), [Sent::State(wire::Speed::Fast, 0)], "the server resumed; player 0 set Fast");

        // A game the host paused stays paused when the silent player returns.
        speed(&mut sim, 1, wire::Speed::Paused);
        event(&mut sim, Inbound::Stalled { session: 2 });
        event(&mut sim, Inbound::Resumed { session: 2 });
        assert_eq!(sim.clock, Clock::Paused);

        // The host's own choice overrides a fairness pause.
        speed(&mut sim, 1, wire::Speed::Fast);
        event(&mut sim, Inbound::Stalled { session: 2 });
        speed(&mut sim, 1, wire::Speed::Normal);
        event(&mut sim, Inbound::Resumed { session: 2 });
        assert_eq!(sim.clock.speed(), wire::Speed::Normal, "the host's speed, not the interrupted one");

        // A silent player who is dropped stops holding the game up.
        speed(&mut sim, 1, wire::Speed::Fast);
        event(&mut sim, Inbound::Stalled { session: 2 });
        event(&mut sim, Inbound::Closed { session: 2 });
        assert_eq!(sim.clock.speed(), wire::Speed::Fast, "the others play on (D24)");
    }

    /// A player who went silent in the lobby is still waited for once the game
    /// starts: the network reports a stall only once, and the row keeps it (D24).
    #[test]
    fn a_player_silent_since_the_lobby_holds_up_the_started_game() {
        let (mut sim, _saves) = multiplayer(2);
        let (mut a, _) = join(&mut sim, 1, Some(0));
        let (_b, _) = join(&mut sim, 2, Some(1));
        sim.handle(Inbound::Request { session: 2, request: Request::SetReady { ready: true } });
        event(&mut sim, Inbound::Stalled { session: 2 });
        start(&mut sim, &[1]);
        drain(&mut a);
        speed(&mut sim, 1, wire::Speed::Fast);
        assert_eq!(sim.clock, Clock::Paused, "the game waits for player 1");
        assert_eq!(drain(&mut a), [Sent::Waiting(wire::Speed::Paused, vec![1])]);
        event(&mut sim, Inbound::Resumed { session: 2 });
        assert_eq!(sim.clock.speed(), wire::Speed::Fast, "the host's speed, once player 1 is back");
    }

    /// M4-4: a player who leaves a started game keeps their seat for their resume
    /// token; the nation stays theirs meanwhile. A kick drops the kept seat.
    #[test]
    fn a_dropped_player_reclaims_their_seat_with_their_token() {
        let (mut sim, _saves) = multiplayer(3);
        let (_a, _) = join(&mut sim, 1, Some(0));
        let (_b, _) = join(&mut sim, 2, Some(1));
        start(&mut sim, &[1, 2]);
        let token = sim.sessions.token(2).expect("a seated player has a token").get();
        event(&mut sim, Inbound::Closed { session: 2 });
        let (_c, sent) = join(&mut sim, 3, Some(1));
        assert_eq!(sent, [Sent::Rejected, Sent::Close], "nation 1 waits for its player");
        let rejoin = |sim: &mut Sim, id: u64, token: u64| {
            let (conn, mut rx) = ConnHandle::for_test();
            sim.handle(Inbound::Connected { session: id, conn });
            let hello = Request::Hello {
                major: PROTOCOL_MAJOR,
                minor: 0,
                name: None,
                requested_nation: None,
                resume_token: token,
                password: None,
            };
            sim.handle(Inbound::Request { session: id, request: hello });
            drain(&mut rx)
        };
        assert_eq!(rejoin(&mut sim, 4, token ^ 1), [Sent::Rejected, Sent::Close], "a forged token");
        assert_eq!(rejoin(&mut sim, 5, token), [Sent::Welcome { player: 1 }], "the same player id");
        assert_eq!(sim.sessions.seat(5).and_then(|s| s.nation()), Some(1), "and the same nation");
        // Away again, then kicked: the nation is free.
        event(&mut sim, Inbound::Closed { session: 5 });
        sim.handle(Inbound::Request { session: 1, request: Request::Kick { player: 1 } });
        assert_eq!(rejoin(&mut sim, 6, token), [Sent::Rejected, Sent::Close], "the kick dropped the kept seat");
        assert_eq!(join(&mut sim, 7, Some(1)).1, [Sent::Welcome { player: 1 }]);
    }

    /// The temporary map-request limit (MILESTONE_4, "Open follow-ups"): a burst of
    /// `Subscribe`s gets one answer at once and one more, for the latest
    /// subscription, when the gap has passed; the ones in between change the
    /// subscription without an answer of their own.
    #[test]
    fn a_burst_of_subscribes_is_answered_once_then_coalesced() {
        use std::time::Duration;
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
            name: None,
            requested_nation: None,
            resume_token: 0,
            password: None,
        };
        sim.handle(Inbound::Request { session: SESSION, request: hello });
        while rx.try_recv().is_ok() {}
        let subscribe = |province| Subscription {
            map_mode: wire::MapMode::Population,
            map_good: 0,
            market: None,
            province: Some(province),
        };
        // The provinces of the answers that came out, each with its map.
        let answers = |rx: &mut Receiver<Outbound>| {
            let mut provinces = Vec::new();
            while let Ok(Outbound::Frame(f)) = rx.try_recv() {
                if let Some(u) = read_server_message(&f).unwrap().payload_as_day_update() {
                    assert!(u.map().is_some(), "an answer carries the map");
                    provinces.push(u.province().map(|p| p.province()));
                }
            }
            provinces
        };
        let start = Instant::now();
        for (i, province) in [0, 1, 2, 3].into_iter().enumerate() {
            sim.subscribe_at(SESSION, subscribe(province), start + Duration::from_millis(20 * i as u64));
        }
        assert_eq!(answers(&mut rx), [Some(0)], "one answer at once");
        let due = sim.next_flush().expect("the burst's answer waits");
        assert!(due > start && due <= start + Duration::from_millis(250), "within a quarter second");
        sim.flush(due);
        assert_eq!(answers(&mut rx), [Some(3)], "one more, for the latest subscription");
        assert_eq!(sim.next_flush(), None, "nothing else waits");
        // After the gap, a Subscribe is answered at once again.
        sim.subscribe_at(SESSION, subscribe(1), due + Duration::from_millis(300));
        assert_eq!(answers(&mut rx), [Some(1)]);
    }

    /// M4-7: a remote session gets at most four updates a second, the days between
    /// coalesced, with the map every fifth update (counting from the Subscribe
    /// answer, which carries it); a day held by the cap still goes out after a pause.
    #[test]
    fn a_remote_session_is_throttled_and_gets_the_last_day() {
        use std::time::Duration;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let saves = test_saves_dir();
        let mut config = crate::Config::local(&dir);
        config.saves_dir = saves.0.clone();
        let mut sim = Sim::new(pax_data::load_scenario(&dir).unwrap(), &config);
        let (conn, mut rx) = ConnHandle::for_test_remote();
        sim.handle(Inbound::Connected { session: SESSION, conn });
        let hello = Request::Hello {
            major: PROTOCOL_MAJOR,
            minor: 0,
            name: None,
            requested_nation: None,
            resume_token: 0,
            password: None,
        };
        sim.handle(Inbound::Request { session: SESSION, request: hello });
        sim.handle(Inbound::Request {
            session: SESSION,
            request: Request::Subscribe {
                map_mode: wire::MapMode::Population,
                map_good: 0,
                market: None,
                province: None,
            },
        });
        while rx.try_recv().is_ok() {}
        // (day, skipped, carries the map) of each update, acknowledging each at once.
        let mut updates = Vec::new();
        let start = Instant::now();
        fn read(rx: &mut Receiver<Outbound>, sim: &mut Sim, now: Instant, updates: &mut Vec<(u64, u32, bool)>) {
            while let Ok(Outbound::Frame(f)) = rx.try_recv() {
                if let Some(u) = read_server_message(&f).unwrap().payload_as_day_update() {
                    updates.push((u.day(), u.skipped(), u.map().is_some()));
                    sim.ack_at(SESSION, u.day(), now);
                }
            }
        }
        // A day every 50 ms for a little over two seconds. The Subscribe answer
        // counted as an update, so the first day goes out a gap after it.
        for i in 1..=42u64 {
            let now = start + Duration::from_millis(50 * i);
            sim.advance_at(now);
            read(&mut rx, &mut sim, now, &mut updates);
        }
        assert_eq!(updates.len(), 8, "four a second: {updates:?}");
        let maps: Vec<u64> = updates.iter().filter(|u| u.2).map(|u| u.0).collect();
        // The Subscribe answer carried the map, so the next one is the fifth update.
        assert_eq!(maps, [updates[4].0], "the map every fifth update: {updates:?}");
        assert!(updates.iter().skip(1).all(|u| u.1 > 0), "the days between are coalesced: {updates:?}");
        // The game stops here: the held last day still goes out when its time comes.
        let at = sim.next_flush().expect("day 42 is held");
        sim.flush(at);
        read(&mut rx, &mut sim, at, &mut updates);
        assert_eq!(updates.last().map(|u| u.0), Some(42), "{updates:?}");
        assert_eq!(sim.next_flush(), None);

        // A seat that ends (a kick, a load without its nation) with a day held leaves
        // nothing due: its row stays until the connection closes, and a flush would
        // never send to it, so a held day there would spin the sim loop.
        let now = at + Duration::from_millis(10);
        sim.advance_at(now);
        assert!(sim.next_flush().is_some(), "the next day is held");
        let vacated = sim.sessions.unseat(SESSION).expect("it was seated");
        sim.seat_ended(SESSION, vacated);
        assert_eq!(sim.next_flush(), None, "an ended seat holds nothing");
    }

    /// M4-6: a server with a password admits only who gives it (or the admin
    /// password); a wrong one is refused with the same words as a missing one.
    #[test]
    fn a_password_admits_and_a_wrong_one_is_refused() {
        let (mut sim, _saves) = multiplayer(3);
        sim.password = Some(crate::Secret::new("open sesame"));
        let hello = |sim: &mut Sim, id: u64, password: Option<&str>| {
            let (conn, mut rx) = ConnHandle::for_test();
            sim.handle(Inbound::Connected { session: id, conn });
            let request = Request::Hello {
                major: PROTOCOL_MAJOR,
                minor: 0,
                name: None,
                requested_nation: None,
                resume_token: 0,
                password: password.map(crate::Secret::new),
            };
            sim.handle(Inbound::Request { session: id, request });
            drain(&mut rx)
        };
        assert_eq!(hello(&mut sim, 1, None), [Sent::Rejected, Sent::Close]);
        assert_eq!(hello(&mut sim, 2, Some("open sesamE")), [Sent::Rejected, Sent::Close]);
        assert_eq!(hello(&mut sim, 3, Some("open sesame")), [Sent::Welcome { player: 0 }]);
    }

    /// M4-6: D24's rate limit, 20 commands a second per session by default; more
    /// get `RateLimited`, and other sessions are unaffected.
    #[test]
    fn commands_past_the_rate_limit_are_refused() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        for seq in 0..25 {
            submit(&mut sim, seq, tax(0, 100_000));
        }
        let limited = drain(&mut rx)
            .into_iter()
            .filter(|s| matches!(s, Sent::Result { error: wire::CommandError::RateLimited, .. }))
            .count();
        assert_eq!(limited, 5, "twenty a second, then refused");
    }

    /// A single-player server has no lobby and never sends LobbyState: lobby
    /// requests are ignored.
    #[test]
    fn single_player_has_no_lobby() {
        let (mut sim, mut rx, _saves) = welcomed(None);
        for request in [Request::ClaimNation { nation: Some(0) }, Request::SetReady { ready: true }, Request::StartGame]
        {
            sim.handle(Inbound::Request { session: SESSION, request });
        }
        assert_eq!(drain_all(&mut rx), []);
        submit(&mut sim, 1, tax(0, 100_000));
        assert_eq!(drain_all(&mut rx), [Sent::Result { seq: 1, error: wire::CommandError::None, day: 0 }]);
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
