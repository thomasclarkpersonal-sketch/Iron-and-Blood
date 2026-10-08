//! The sim thread (D23): the only owner of the `World`.
//!
//! It handles session requests in arrival order and decides everything that needs game
//! state. In M3-2 that's the handshake: `Welcome` or `Rejected`. Ticking, views,
//! commands and saves arrive with M3-3 to M3-6.

use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;

use pax_data::Scenario;
use pax_protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, wire};
use tracing::{debug, info};

use crate::encode::{self, WelcomeInfo};
use crate::net::{ConnHandle, Inbound, Outbound};
use crate::request::Request;

struct Session {
    conn: ConnHandle,
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
}

impl Sim {
    pub(crate) fn new(scenario: Scenario, exit_when_idle: bool) -> Self {
        Sim { scenario, sessions: BTreeMap::new(), active: None, exit_when_idle, speed: wire::Speed::Paused }
    }

    fn send(&self, session: u64, out: Outbound) {
        if let Some(s) = self.sessions.get(&session) {
            s.conn.send(out);
        }
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
        self.active = Some(session);
        if let Some(s) = self.sessions.get_mut(&session) {
            s.seat = Some(Seat { player: 0, nation: requested_nation });
        }
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
                self.sessions.insert(session, Session { conn, seat: None });
            }
            Inbound::Request { session, request: Request::Hello { major, minor, name, requested_nation, .. } } => {
                self.hello(session, major, minor, name.as_deref(), requested_nation);
            }
            // The admission choke point: only welcomed sessions get past here.
            Inbound::Request { session, request } if !self.sessions.get(&session).is_some_and(|s| s.seat.is_some()) => {
                debug!(session, ?request, "ignored: the session was not welcomed");
            }
            Inbound::Request { session, request } => {
                debug!(session, ?request, "not handled until M3-3 to M3-6");
            }
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
        }
        true
    }
}

pub(crate) fn run(mut sim: Sim, inbound: Receiver<Inbound>) {
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
