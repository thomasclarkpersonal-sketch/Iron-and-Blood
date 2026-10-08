//! The sim thread (D23): the only owner of the `World`.
//!
//! It handles session requests in arrival order and decides everything that needs game
//! state: the handshake (`Welcome` or `Rejected`) and subscriptions with their views.
//! Ticking, commands and saves arrive with M3-4 to M3-6.

use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;

use pax_data::Scenario;
use pax_engine::DayReport;
use pax_protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, wire};
use tracing::{debug, info};

use crate::encode::{self, WelcomeInfo};
use crate::net::{ConnHandle, Inbound, Outbound};
use crate::request::Request;
use crate::view::{self, DayViews, Subscription};

struct Session {
    conn: ConnHandle,
    subscription: Subscription,
    /// Set by `Welcome`. Until then the session may only say `Hello`: one choke
    /// point in [`Sim::handle`] drops anything else, so no request handler has to
    /// remember to check (a rejected session's queued requests never act).
    seat: Option<Seat>,
}

/// Who a welcomed session plays.
#[derive(Clone, Copy, Debug)]
struct Seat {
    #[allow(dead_code)] // read by command handling (M3-4)
    player: u16,
    /// `None`: sandbox, may command every nation (M3 only, D24).
    #[allow(dead_code)] // read by command handling (M3-4)
    nation: Option<u32>,
}

pub(crate) struct Sim {
    scenario: Scenario,
    sessions: BTreeMap<u64, Session>,
    /// The single welcomed session (M3 is single player, D10).
    active: Option<u64>,
    /// Stop when the welcomed session leaves (the client launched this server).
    exit_when_idle: bool,
    speed: wire::Speed,
    /// The report of the last day that ran; `None` before the first tick.
    last_report: Option<DayReport>,
}

impl Sim {
    pub(crate) fn new(scenario: Scenario, exit_when_idle: bool) -> Self {
        Sim {
            scenario,
            sessions: BTreeMap::new(),
            active: None,
            exit_when_idle,
            speed: wire::Speed::Paused,
            last_report: None,
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

    /// Replaces the session's subscription and answers with a `DayUpdate` for the
    /// current day, so a newly opened panel fills at once, even when paused (D22).
    fn subscribe(&mut self, session: u64, requested: Subscription) {
        let subscription = match requested.checked(&self.scenario.world) {
            Ok(s) => s,
            Err(reason) => return self.goodbye(session, &reason),
        };
        let Some(s) = self.sessions.get_mut(&session) else { return };
        s.subscription = subscription;
        let views = DayViews::new(&self.scenario.world, self.last_report.as_ref());
        self.send(session, Outbound::Frame(view::day_update(&views, &subscription, self.speed, 0)));
    }

    fn reject(&self, session: u64, reason: &str) {
        info!(session, reason, "rejected");
        self.send(session, Outbound::Frame(encode::rejected(reason)));
        self.send(session, Outbound::Close);
    }

    fn hello(&mut self, session: u64, major: u16, minor: u16, name: Option<&str>, requested_nation: Option<u32>) {
        let nations = self.scenario.world.nations.key.len();
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
            scenario: &self.scenario.name,
            content_hash: self.scenario.content_hash,
            speed: self.speed,
        };
        self.send(session, Outbound::Frame(encode::welcome(&self.scenario.world, &info)));
    }

    /// Handles one inbound event. Returns `false` when the server should stop.
    fn handle(&mut self, event: Inbound) -> bool {
        match event {
            Inbound::Connected { session, conn } => {
                self.sessions.insert(session, Session { conn, subscription: Subscription::default(), seat: None });
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
                other => debug!(session, ?other, "not handled until M3-4 to M3-6"),
            },
            Inbound::Closed { session } => {
                self.sessions.remove(&session);
                if self.active == Some(session) {
                    self.active = None;
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

pub(crate) fn run(sim: &mut Sim, inbound: Receiver<Inbound>) {
    while let Ok(event) = inbound.recv() {
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
