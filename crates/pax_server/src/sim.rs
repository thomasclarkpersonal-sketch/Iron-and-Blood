//! The sim thread (D23): the only owner of the `World`.
//!
//! It handles session requests in arrival order and decides everything that needs game
//! state:
//! * the handshake (`Welcome` or `Rejected`);
//! * subscriptions and their views;
//! * commands, checked, queued and applied at the start of the next tick;
//! * the log of every command that applied (D21, D23);
//! * pacing: when ticks happen, at the session's chosen speed;
//! * flow control: which updates each session is sent, kept in [`crate::window`].
//!
//! Saves arrive with M3-6.

use flume::{Receiver, RecvTimeoutError, TryRecvError};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use pax_data::Scenario;
use pax_engine::Command;
use pax_protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, wire};
use tracing::{debug, info};

use crate::commands;
use crate::encode::{self, WelcomeInfo};
use crate::game::Game;
use crate::net::{ConnHandle, Inbound, Outbound};
use crate::request::{Request, WireCommand};
use crate::view::{self, CheckedSubscription, DayViews, Subscription};
use crate::window::UpdateWindow;

/// How a speed paces the clock (D23).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pace {
    Paused,
    /// One day per interval; a zero interval for `Fastest`, which ticks as fast as
    /// the engine allows.
    Every(Duration),
    /// A speed newer than this server.
    Unknown,
}

/// Days per real second at each speed (D23).
fn pace(speed: wire::Speed) -> Pace {
    use wire::Speed as S;
    match speed {
        S::Paused => Pace::Paused,
        S::Slowest => Pace::Every(Duration::from_millis(2_000)), // 0.5 days/s
        S::Slow => Pace::Every(Duration::from_millis(1_000)),    // 1
        S::Normal => Pace::Every(Duration::from_millis(500)),    // 2
        S::Fast => Pace::Every(Duration::from_millis(200)),      // 5
        S::Fastest => Pace::Every(Duration::ZERO),
        _ => Pace::Unknown,
    }
}

/// The game clock: one value, so "paused" and "a tick is due" can't disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clock {
    Paused,
    Running { speed: wire::Speed, interval: Duration, next: Instant },
}

impl Clock {
    /// The clock at `speed`, its first tick one interval after `now`, so unpausing
    /// never fires a burst of catch-up ticks. `None` for an unknown speed.
    fn at(speed: wire::Speed, now: Instant) -> Option<Clock> {
        match pace(speed) {
            Pace::Paused => Some(Clock::Paused),
            Pace::Every(interval) => Some(Clock::Running { speed, interval, next: now + interval }),
            Pace::Unknown => None,
        }
    }

    fn speed(self) -> wire::Speed {
        match self {
            Clock::Paused => wire::Speed::Paused,
            Clock::Running { speed, .. } => speed,
        }
    }

    /// When the next tick is due; `None` while paused.
    fn due(self) -> Option<Instant> {
        match self {
            Clock::Paused => None,
            Clock::Running { next, .. } => Some(next),
        }
    }

    /// A tick ran at `now`: the next is one interval after the one that was due,
    /// keeping the cadence, but never in the past after a slow tick.
    fn ticked(&mut self, now: Instant) {
        if let Clock::Running { interval, next, .. } = self {
            *next = (*next + *interval).max(now);
        }
    }
}

struct Session {
    conn: ConnHandle,
    subscription: CheckedSubscription,
    /// Set by `Welcome`. Until then the session may only say `Hello`: one choke
    /// point in [`Sim::handle`] drops anything else, so no request handler has to
    /// remember to check (a rejected session's queued requests never act).
    seat: Option<Seat>,
    /// Which days' updates it is sent (D23 flow control).
    window: UpdateWindow,
}

/// Who a welcomed session plays.
#[derive(Clone, Copy, Debug)]
struct Seat {
    player: u16,
    /// `None`: sandbox, may command every nation (M3 only, D24).
    nation: Option<u32>,
}

/// A command accepted on arrival, waiting for the start of the next tick.
#[derive(Clone, Copy, Debug)]
struct Pending {
    session: u64,
    /// The stamp, assigned by the server on arrival (D10, D22): commands apply in
    /// `(player, sequence)` order within their day, never in raw arrival order.
    player: u16,
    sequence: u64,
    client_seq: u32,
    command: Command,
}

/// A command that applied: the game's history (D21). Saves are the scenario plus
/// this log (D23).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Logged {
    /// The day at whose start it applied.
    pub day: u64,
    /// `None` for the scenario's own scripted commands (`commands.toml`).
    pub player: Option<u16>,
    pub command: Command,
}

pub(crate) struct Sim {
    game: Game,
    sessions: BTreeMap<u64, Session>,
    /// The single welcomed session (M3 is single player, D10).
    active: Option<u64>,
    /// Stop when the welcomed session leaves (the client launched this server).
    exit_when_idle: bool,
    /// The speed and the next tick. Only [`Sim::set_clock`] changes the speed.
    clock: Clock,
    /// Commands accepted for the next tick, each stamped `(player, sequence)` on
    /// arrival. `tick` applies them sorted by that stamp (D10's `(day, player,
    /// sequence)`; all of them are for the same day).
    pending: Vec<Pending>,
    /// The next command's `sequence`: increases for the whole game, so it orders a
    /// player's commands as the server received them.
    next_sequence: u64,
    /// Every command that applied, in application order.
    log: Vec<Logged>,
}

impl Sim {
    pub(crate) fn new(scenario: Scenario, exit_when_idle: bool) -> Self {
        Sim {
            game: Game::new(scenario),
            sessions: BTreeMap::new(),
            active: None,
            exit_when_idle,
            clock: Clock::Paused,
            pending: Vec::new(),
            next_sequence: 0,
            log: Vec::new(),
        }
    }

    fn send(&self, session: u64, out: Outbound) {
        if let Some(s) = self.sessions.get(&session) {
            s.conn.send(out);
        }
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
        if self.active.is_some() {
            return self.reject(session, "server full: a single-player server accepts one client");
        }
        if let Some(n) = requested_nation
            && n as usize >= nations
        {
            return self.reject(session, &format!("unknown nation {n}: the scenario has {nations}"));
        }
        // A connection's events arrive in order on one channel: Connected, its
        // requests, then Closed. So a session that sent Hello is always known here.
        let s = self.sessions.get_mut(&session).expect("Connected precedes every request of a session");
        s.seat = Some(Seat { player: 0, nation: requested_nation });
        self.active = Some(session);
        info!(session, name, ?requested_nation, "welcomed");
        let info = WelcomeInfo {
            player: 0,
            resume_token: 0, // resuming a dropped session is M4 (D24)
            nation: requested_nation,
            scenario: &self.game.scenario().name,
            content_hash: self.game.scenario().content_hash,
            speed: self.clock.speed(),
        };
        self.send(session, Outbound::Frame(encode::welcome(self.game.world(), &info)));
    }

    /// Replaces the session's subscription and answers with a `DayUpdate` for the
    /// current day, so a newly opened panel fills at once, even when paused (D22).
    /// The refresh bypasses the flow-control window: the client asked for it.
    fn subscribe(&mut self, session: u64, requested: Subscription) {
        let subscription = match requested.checked(self.game.world()) {
            Ok(s) => s,
            Err(reason) => return self.goodbye(session, &reason),
        };
        let Some(s) = self.sessions.get_mut(&session) else { return };
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
        let Some(seat) = self.sessions.get(&session).and_then(|s| s.seat) else { return };
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
        self.stamp(session, seat.player, client_seq, command);
        self.command_result(session, client_seq, wire::CommandError::None);
    }

    /// Stamps an accepted command `(player, sequence)` on arrival (D10, D22) and
    /// queues it for the next tick.
    fn stamp(&mut self, session: u64, player: u16, client_seq: u32, command: Command) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.pending.push(Pending { session, player, sequence, client_seq, command });
    }

    /// A session asked for a new speed. An unknown speed is a protocol error.
    fn set_speed(&mut self, session: u64, speed: wire::Speed) {
        let Some(seat) = self.sessions.get(&session).and_then(|s| s.seat) else { return };
        match Clock::at(speed, Instant::now()) {
            Some(clock) => self.set_clock(clock, seat.player),
            None => self.goodbye(session, &format!("unknown speed {}", speed.0)),
        }
    }

    /// The only place the speed changes: sets the clock and tells every welcomed
    /// session (`ServerState`, D23).
    fn set_clock(&mut self, clock: Clock, changed_by: u16) {
        self.clock = clock;
        let speed = clock.speed();
        info!(?speed, "speed changed");
        let frame = encode::server_state(self.game.world().day, speed, changed_by);
        for (&id, s) in &self.sessions {
            if s.seat.is_some() {
                self.send(id, Outbound::Frame(frame.clone()));
            }
        }
    }

    /// The session processed the update for `day`: everything up to it leaves the
    /// window. If days ran while the window was full, the latest day goes out now.
    fn ack(&mut self, session: u64, day: u64) {
        let Some(s) = self.sessions.get_mut(&session) else { return };
        if s.window.ack(day) {
            send_update(s, &self.game.views(), self.clock.speed());
        }
    }

    /// Runs one day. The scenario's scripted commands for the day apply first, then
    /// the players' commands in stamp order (D10, D23). Everything that applies is
    /// logged. `step_with` re-validates each command; one that fails there (none can
    /// today) gets a late error `CommandResult` and is not logged.
    pub(crate) fn tick(&mut self) {
        let day = self.game.world().day;
        let scripted = self.game.scenario().commands.for_day(day).to_vec();
        // Stamp order, by construction: arrival order between players is a network
        // race, so it must never decide the order commands apply in.
        self.pending.sort_by_key(|p| (p.player, p.sequence));
        let mut commands = scripted.clone();
        commands.extend(self.pending.iter().map(|p| p.command));
        let results = self.game.step(&commands);
        let (scripted_results, player_results) = results.split_at(scripted.len());
        for (command, result) in scripted.iter().zip(scripted_results) {
            match result {
                Ok(()) => self.log.push(Logged { day, player: None, command: *command }),
                // The loader validated the scenario's log against the initial world.
                Err(e) => tracing::warn!(day, ?command, %e, "scripted command rejected"),
            }
        }
        for (p, result) in std::mem::take(&mut self.pending).into_iter().zip(player_results) {
            match result {
                Ok(()) => self.log.push(Logged { day, player: Some(p.player), command: p.command }),
                Err(e) => self.command_result(p.session, p.client_seq, commands::error_to_wire(e)),
            }
        }
    }

    /// Ticks, then sends each welcomed session the new day if its window has room
    /// (D23). The views are built once and shared by all sessions.
    fn advance(&mut self) {
        self.tick();
        let mut due = Vec::new();
        for (&id, s) in &mut self.sessions {
            if s.seat.is_none() {
                continue;
            }
            if s.window.day_ran() {
                due.push(id);
            }
        }
        if !due.is_empty() {
            // Build the views only when someone will see them.
            let views = self.game.views();
            for id in due {
                if let Some(s) = self.sessions.get_mut(&id) {
                    send_update(s, &views, self.clock.speed());
                }
            }
        }
    }

    /// Every command applied so far, in application order.
    #[cfg(test)]
    pub(crate) fn log(&self) -> &[Logged] {
        &self.log
    }

    /// Handles one inbound event. Returns `false` when the server should stop.
    fn handle(&mut self, event: Inbound) -> bool {
        match event {
            Inbound::Connected { session, conn } => {
                let session_state = Session {
                    conn,
                    subscription: CheckedSubscription::default(),
                    seat: None,
                    window: UpdateWindow::default(),
                };
                self.sessions.insert(session, session_state);
            }
            Inbound::Request { session, request: Request::Hello { major, minor, name, requested_nation, .. } } => {
                self.hello(session, major, minor, name.as_deref(), requested_nation);
            }
            // The admission choke point: only welcomed sessions get past here.
            Inbound::Request { session, request } if !self.sessions.get(&session).is_some_and(|s| s.seat.is_some()) => {
                debug!(session, ?request, "ignored: the session was not welcomed");
            }
            Inbound::Request { session, request } => match request {
                Request::Subscribe { map_mode, map_good, market, province } => {
                    self.subscribe(session, Subscription { map_mode, map_good, market, province });
                }
                Request::SubmitCommand { client_seq, command } => self.submit(session, client_seq, command),
                Request::SetSpeed { speed } => self.set_speed(session, speed),
                Request::Ack { day } => self.ack(session, day),
                other => debug!(session, ?other, "not handled until M3-6"),
            },
            Inbound::Closed { session } => {
                let seat = self.sessions.remove(&session).and_then(|s| s.seat);
                if self.active == Some(session) {
                    self.active = None;
                    // Nobody is watching: stop the clock (D23 never runs a game unobserved).
                    let player = seat.map_or(0, |s| s.player);
                    self.set_clock(Clock::Paused, player);
                    if self.exit_when_idle {
                        info!("the client left; exiting (--exit-when-idle)");
                        return false;
                    }
                }
            }
            Inbound::Shutdown => return false,
            #[cfg(test)]
            Inbound::Crash => panic!("injected crash for a test"),
        }
        true
    }
}

impl Sim {
    /// After the sim thread panicked: tell every connection why it is being closed.
    pub(crate) fn fail(&self, message: &str) {
        let reason = format!("the server failed: {message}");
        for session in self.sessions.values() {
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
    for session in sim.sessions.values() {
        session.conn.send(Outbound::Frame(encode::goodbye("the server is shutting down")));
        session.conn.send(Outbound::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pax_engine::Fixed;
    use pax_protocol::read_server_message;
    use tokio::sync::mpsc::Receiver;

    const SESSION: u64 = 1;

    /// What the sim thread sent a session, decoded.
    #[derive(Debug, PartialEq)]
    enum Sent {
        Welcome,
        Rejected,
        Goodbye(String),
        Result { seq: u32, error: wire::CommandError, day: u64 },
        Update { day: u64, skipped: u32 },
        State(wire::Speed),
        Close,
    }

    fn drain(rx: &mut Receiver<Outbound>) -> Vec<Sent> {
        let mut out = Vec::new();
        while let Ok(o) = rx.try_recv() {
            out.push(match o {
                Outbound::Close => Sent::Close,
                Outbound::Frame(f) => {
                    let m = read_server_message(&f).unwrap();
                    if m.payload_as_welcome().is_some() {
                        Sent::Welcome
                    } else if m.payload_as_rejected().is_some() {
                        Sent::Rejected
                    } else if let Some(g) = m.payload_as_goodbye() {
                        Sent::Goodbye(g.reason().unwrap_or_default().to_owned())
                    } else if let Some(r) = m.payload_as_command_result() {
                        Sent::Result { seq: r.client_seq(), error: r.error(), day: r.applies_on_day() }
                    } else if let Some(u) = m.payload_as_day_update() {
                        Sent::Update { day: u.day(), skipped: u.skipped() }
                    } else if let Some(s) = m.payload_as_server_state() {
                        Sent::State(s.speed())
                    } else {
                        panic!("unexpected {:?}", m.payload_type())
                    }
                }
            });
        }
        out
    }

    /// `two_states` with one welcomed session playing `nation` (`None`: sandbox).
    fn welcomed(nation: Option<u32>) -> (Sim, Receiver<Outbound>) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let mut sim = Sim::new(pax_data::load_scenario(&dir).unwrap(), false);
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
        assert_eq!(drain(&mut rx), [Sent::Welcome]);
        (sim, rx)
    }

    fn submit(sim: &mut Sim, seq: u32, command: Option<WireCommand>) {
        sim.handle(Inbound::Request { session: SESSION, request: Request::SubmitCommand { client_seq: seq, command } });
    }

    fn tax(nation: u32, raw: i64) -> Option<WireCommand> {
        Some(WireCommand::SetIncomeTax { nation, rate_raw: Some(raw) })
    }

    #[test]
    fn an_accepted_command_applies_at_the_next_tick_and_is_logged() {
        let (mut sim, mut rx) = welcomed(None);
        submit(&mut sim, 7, tax(1, 150_000));
        assert_eq!(drain(&mut rx), [Sent::Result { seq: 7, error: wire::CommandError::None, day: 0 }]);
        assert_ne!(sim.game.world().nations.income_tax_rate[1], Fixed::from_raw(150_000), "not before the tick");
        sim.tick();
        assert_eq!(sim.game.world().nations.income_tax_rate[1], Fixed::from_raw(150_000));
        let applied = Command::SetIncomeTax { nation: 1, rate: Fixed::from_raw(150_000) };
        assert_eq!(sim.log(), [Logged { day: 0, player: Some(0), command: applied }]);
    }

    #[test]
    fn commands_are_checked_in_order_well_formed_permitted_valid() {
        let (mut sim, mut rx) = welcomed(Some(0));
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
        let (mut sim, _rx) = welcomed(None);
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
        let (mut sim, mut rx) = welcomed(None);
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
        let (mut sim, mut rx) = welcomed(None);
        assert_eq!(sim.clock, Clock::Paused, "a new game starts paused");
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed::Normal } });
        assert_eq!(drain(&mut rx), [Sent::State(wire::Speed::Normal)]);
        assert!(sim.clock.due().is_some());
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed::Paused } });
        assert_eq!(drain(&mut rx), [Sent::State(wire::Speed::Paused)]);
        assert_eq!(sim.clock, Clock::Paused);
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed(9) } });
        assert!(matches!(drain(&mut rx).as_slice(), [Sent::Goodbye(r), Sent::Close] if r.contains("unknown speed")));
    }

    #[test]
    fn the_clock_stops_when_the_player_leaves() {
        let (mut sim, _rx) = welcomed(None);
        sim.handle(Inbound::Request { session: SESSION, request: Request::SetSpeed { speed: wire::Speed::Fast } });
        sim.handle(Inbound::Closed { session: SESSION });
        assert_eq!(sim.clock, Clock::Paused);
    }

    #[test]
    fn the_clock_keeps_its_cadence_but_never_schedules_in_the_past() {
        let start = Instant::now();
        let mut clock = Clock::at(wire::Speed::Normal, start).expect("a known speed");
        assert_eq!(clock.due(), Some(start + Duration::from_millis(500)));
        clock.ticked(start + Duration::from_millis(600));
        assert_eq!(clock.due(), Some(start + Duration::from_millis(1_000)), "a late tick keeps the cadence");
        clock.ticked(start + Duration::from_millis(5_000));
        assert_eq!(clock.due(), Some(start + Duration::from_millis(5_000)), "a slow tick never queues catch-up");
        assert_eq!(Clock::at(wire::Speed(9), start), None);
        assert_eq!(Clock::at(wire::Speed::Paused, start), Some(Clock::Paused));
    }

    /// D10: within a day, commands apply in `(player, sequence)` order, whatever
    /// order they arrived in (arrival order between players is a network race).
    #[test]
    fn commands_apply_in_stamp_order_not_arrival_order() {
        let (mut sim, _rx) = welcomed(None);
        let rate = |raw| Command::SetIncomeTax { nation: 0, rate: Fixed::from_raw(raw) };
        // Player 1's command arrives first; player 0's two follow.
        for (player, raw) in [(1, 100_000), (0, 110_000), (0, 120_000)] {
            sim.stamp(SESSION, player, 0, rate(raw));
        }
        sim.tick();
        let applied: Vec<_> = sim.log().iter().map(|l| (l.player, l.command)).collect();
        assert_eq!(applied, [(Some(0), rate(110_000)), (Some(0), rate(120_000)), (Some(1), rate(100_000))]);
        // The last command in stamp order wins: player 1's.
        assert_eq!(sim.game.world().nations.income_tax_rate[0], Fixed::from_raw(100_000));
    }
}
