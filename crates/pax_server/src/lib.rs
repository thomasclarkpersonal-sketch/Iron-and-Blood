//! `pax_server`: the authoritative game server (D10, D22, D23).
//!
//! * **Sim thread** (`sim`): owns the `World` exclusively and decides everything that
//!   needs game state. There are no locks around world state.
//! * **Network** (`net`): one tokio task per connection, on a small runtime that
//!   never runs simulation code. Connections exchange owned messages with the sim
//!   thread over channels.
//! * **Wire:** `request` turns verified client frames into owned values, and
//!   `encode` builds server frames. This crate is the only one that sees both the
//!   engine and the wire types (`pax_protocol` has no engine dependency).
//!
//! * **Views** (`view`): what each session sees of the world each day (M3-3).
//! * **Sessions** (`session`): the sim thread's table of connections and the seats
//!   of the players among them (M4-1, D24).
//!
//! Built so far: M3 (single player, `docs/MILESTONE_3.md`) and several players at
//! once (M4-1). The lobby, authority and lag rules follow in `docs/MILESTONE_4.md`.

mod clock;
mod commands;
mod encode;
mod game;
#[cfg(test)]
mod hostile;
mod net;
#[cfg(test)]
mod noise;
mod queue;
mod request;
mod secret;
mod session;
mod sim;
mod throttle;
pub use secret::Secret;
pub use throttle::Bandwidth;
mod view;
mod window;

/// A connection task's input side, for the cargo-fuzz target only (`fuzz/`, M3-10):
/// it fuzzes exactly what a connection runs on its socket's bytes.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub mod fuzzing {
    pub use crate::net::{ReadError, RequestReader};
    pub use crate::request::Request;
}

use std::net::SocketAddr;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::Duration;

/// D24's default fairness pause: a multiplayer game pauses for everyone when a
/// player has sent nothing for this long (`--pause-after`).
pub const PAUSE_AFTER: Duration = Duration::from_secs(5);

/// D24's default drop: a multiplayer session silent this long is closed, and its
/// seat waits for its resume token (`--drop-after`). It replaces D22's 10 s rule.
pub const DROP_AFTER: Duration = Duration::from_secs(30);

/// D24's default update rate for a remote session: at most this many `DayUpdate`s a
/// second (`--updates-per-second`, M4-7).
pub const UPDATES_PER_SECOND: u32 = 4;

/// D24's default map refresh for a remote session: its `MapView` goes out with every
/// this-many-th update, and whenever its subscription changes (`--map-every`, M4-7).
pub const MAP_EVERY: u32 = 5;
/// D24's default per-session command rate limit (`--commands-per-second`).
pub const COMMANDS_PER_SECOND: u32 = 20;

/// A dedicated server's admin (D24, M4-6): the client name that is host, and the
/// password that proves it. One value, so a name can't exist without its proof: a
/// name alone proves nothing. The password also admits the admin to a server with
/// a server password.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admin {
    pub name: String,
    pub password: Secret,
}

/// How to run a server.
#[derive(Clone, Debug)]
pub struct Config {
    /// Scenario directory (containing `scenario.toml`).
    pub scenario: PathBuf,
    /// Address to listen on. Single player binds `127.0.0.1:0`, a free local port
    /// (NETWORK_PROTOCOL §6).
    pub bind: SocketAddr,
    /// Close a session that sends nothing for this long (the client pings at least
    /// every 2 s, NETWORK_PROTOCOL §3): D22's 10 s in single player, D24's drop
    /// (`--drop-after`, 30 s) in multiplayer.
    pub idle_timeout: Duration,
    /// In multiplayer, pause the game for everyone when a player has sent nothing for
    /// this long, and resume when they speak again (D24's fairness pause,
    /// `--pause-after`, 5 s). `None`: no fairness pause (single player).
    pub pause_after: Option<Duration>,
    /// Stop when the last player leaves: the client launched this server.
    pub exit_when_idle: bool,
    /// How many sessions may play at once; one more is refused with "server full".
    /// Single player is 1 (M3); `--players N` sets it for multiplayer (M4-1).
    pub max_players: u16,
    /// Accept sandbox sessions (`Hello` without a nation), which may command every
    /// nation (D24): `--sandbox`. Single player runs this way.
    pub sandbox: bool,
    /// On a dedicated server, the host's client name and the password that proves
    /// it (`--admin NAME --admin-password-file PATH`, D24, M4-6). When unset, the
    /// first player is host, and when the host leaves, the remaining player with the
    /// lowest id.
    pub admin: Option<Admin>,
    /// Players must present this in `Hello` (`--password-file`, D24). `None`: no
    /// password.
    pub password: Option<Secret>,
    /// At most this many commands per second per session; more get `RateLimited`
    /// (`--commands-per-second`, D24's default 20).
    pub commands_per_second: u32,
    /// Where `SaveGame` writes and `LoadGame` reads `<name>.toml` (D23).
    pub saves_dir: PathBuf,
    /// How often a remote session gets an update, and its map (D24, M4-7):
    /// `--updates-per-second` and `--map-every`, by default [`UPDATES_PER_SECOND`]
    /// and [`MAP_EVERY`]. Local sessions are never throttled.
    pub bandwidth: Bandwidth,
}

impl Config {
    /// The rules a configuration must keep, however it was built (the CLI, tests, a
    /// launcher):
    /// * the fairness pause is multiplayer only, and comes before the drop (D24), or
    ///   the connection task would drop a client when it should pause the game;
    /// * a remote session gets updates, and a map, at some rate (M4-7);
    /// * D24's TLS rule: several players bind loopback only until TLS (M4-6b);
    /// * a password never crosses the network in clear: a server with a password or
    ///   an admin binds loopback only until TLS (M4-6b);
    /// * a rate limit of at least one command a second (D24).
    pub fn validate(&self) -> Result<(), ConfigError> {
        if let Some(pause) = self.pause_after {
            if self.max_players <= 1 {
                return Err(ConfigError::PauseInSinglePlayer);
            }
            if pause >= self.idle_timeout {
                return Err(ConfigError::PauseNotBeforeDrop);
            }
        }
        if self.bandwidth.updates_per_second == 0 || self.bandwidth.map_every == 0 {
            return Err(ConfigError::NoUpdates);
        }
        if self.max_players > 1 && !self.bind.ip().is_loopback() {
            return Err(ConfigError::MultiplayerNeedsTls { players: self.max_players, bind: self.bind });
        }
        if (self.password.is_some() || self.admin.is_some()) && !self.bind.ip().is_loopback() {
            return Err(ConfigError::PasswordNeedsTls { bind: self.bind });
        }
        if self.commands_per_second == 0 {
            return Err(ConfigError::NoCommandsAllowed);
        }
        Ok(())
    }

    /// A multiplayer server for `scenario` with room for `players`, with D24's
    /// defaults: no sandbox, and its lag rules ([`Self::use_multiplayer_lag`]).
    pub fn multiplayer(scenario: impl Into<PathBuf>, players: u16) -> Self {
        let mut config = Config { max_players: players, sandbox: false, ..Config::local(scenario) };
        config.use_multiplayer_lag();
        config
    }

    /// D24's lag rules in place of D22's 10 s: a fairness pause after
    /// [`PAUSE_AFTER`] of silence and a drop after [`DROP_AFTER`]. The one place those
    /// defaults are applied: [`Self::multiplayer`] and the CLI both call it, and the
    /// CLI's `--pause-after` and `--drop-after` override it.
    pub fn use_multiplayer_lag(&mut self) {
        self.idle_timeout = DROP_AFTER;
        self.pause_after = Some(PAUSE_AFTER);
    }

    /// A local single-player server for `scenario`, as the client launches it: one
    /// player, sandbox allowed, the protocol's 10 s timeout.
    pub fn local(scenario: impl Into<PathBuf>) -> Self {
        Config {
            scenario: scenario.into(),
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            idle_timeout: pax_protocol::IDLE_TIMEOUT,
            pause_after: None,
            exit_when_idle: false,
            saves_dir: PathBuf::from("saves"),
            max_players: 1,
            sandbox: true,
            admin: None,
            bandwidth: Bandwidth::default(),
            password: None,
            commands_per_second: COMMANDS_PER_SECOND,
        }
    }
}

/// A rule a [`Config`] breaks (`Config::validate`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// A fairness pause is for multiplayer (D24).
    PauseInSinglePlayer,
    /// The pause must come before the drop, or clients are dropped instead.
    PauseNotBeforeDrop,
    /// `--updates-per-second` or `--map-every` is 0: a remote session would never
    /// get an update, or never a map.
    NoUpdates,
    /// D24: TLS off localhost, which arrives with M4-6b.
    MultiplayerNeedsTls { players: u16, bind: SocketAddr },
    /// A password in `Hello` would cross the network in clear: off localhost, a
    /// server with a password or an admin needs TLS (D24, M4-6b).
    PasswordNeedsTls { bind: SocketAddr },
    /// A rate limit of 0 commands per second would refuse every command.
    NoCommandsAllowed,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::PauseInSinglePlayer => {
                write!(f, "a fairness pause (--pause-after) is for multiplayer (--players above 1)")
            }
            ConfigError::PauseNotBeforeDrop => write!(f, "--pause-after must be shorter than --drop-after"),
            ConfigError::NoUpdates => write!(f, "--updates-per-second and --map-every must be at least 1"),
            ConfigError::MultiplayerNeedsTls { players, bind } => write!(
                f,
                "--players {players} on {bind} needs TLS (D24), which arrives with M4-6b: until then, bind 127.0.0.1"
            ),
            ConfigError::PasswordNeedsTls { bind } => write!(
                f,
                "a password on {bind} would cross the network in clear: it needs TLS (D24), which arrives with M4-6b; until then, bind 127.0.0.1"
            ),
            ConfigError::NoCommandsAllowed => write!(f, "--commands-per-second must be at least 1"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Why a server couldn't start.
#[derive(Debug)]
pub enum StartError {
    /// The configuration breaks a rule (`Config::validate`).
    Config(ConfigError),
    Scenario(pax_data::LoadError),
    Io(std::io::Error),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::Config(e) => write!(f, "{e}"),
            StartError::Scenario(e) => write!(f, "{e}"),
            StartError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StartError {}

/// Why a running server stopped abnormally: the simulation thread panicked. The
/// engine panics on purpose when an invariant breaks (overflow, money conservation;
/// AGENTS.md §4), so this is the server's most important failure signal.
#[derive(Debug)]
pub struct ServerFailure(pub String);

impl std::fmt::Display for ServerFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the simulation thread panicked: {}", self.0)
    }
}

impl std::error::Error for ServerFailure {}

/// The message a panic carried (`panic!` with a literal or a formatted string).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

impl From<std::io::Error> for StartError {
    fn from(e: std::io::Error) -> Self {
        StartError::Io(e)
    }
}

/// A running server.
pub struct Server {
    local_addr: SocketAddr,
    to_sim: flume::Sender<net::Inbound>,
    sim: JoinHandle<Result<(), ServerFailure>>,
    runtime: tokio::runtime::Runtime,
}

impl Server {
    /// Loads the scenario, binds the listener and starts the sim thread. Returns once
    /// the server accepts connections.
    pub fn start(config: Config) -> Result<Server, StartError> {
        config.validate().map_err(StartError::Config)?;
        let scenario = pax_data::load_scenario(&config.scenario).map_err(StartError::Scenario)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("pax-net")
            .enable_all()
            .build()?;
        let listener = runtime.block_on(tokio::net::TcpListener::bind(config.bind))?;
        let local_addr = listener.local_addr()?;
        // Bounded: a connection whose requests pile up stops being read (backpressure),
        // as a connection whose replies pile up is closed (net.rs).
        let (to_sim, inbound) = flume::bounded(net::INBOUND_QUEUE);
        let timing = net::Timing { idle: config.idle_timeout, stall_after: config.pause_after };
        runtime.spawn(net::accept_loop(listener, to_sim.clone(), timing));
        let mut sim = sim::Sim::new(scenario, &config);
        let sim = std::thread::Builder::new().name("pax-sim".to_owned()).spawn(move || {
            // A panic is caught only to tell the clients and the caller; the default
            // hook has already printed it with its location.
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sim::run(&mut sim, inbound))) {
                Ok(()) => Ok(()),
                Err(payload) => {
                    let message = panic_message(&*payload);
                    sim.fail(&message);
                    Err(ServerFailure(message))
                }
            }
        })?;
        tracing::info!(%local_addr, "listening");
        Ok(Server { local_addr, to_sim, sim, runtime })
    }

    /// The address the server listens on (the actual port when bound to port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Blocks until the server stops: by itself (`exit_when_idle`), or because the
    /// simulation failed, which is the `Err`.
    pub fn wait(self) -> Result<(), ServerFailure> {
        let outcome = self.sim.join().unwrap_or_else(|payload| Err(ServerFailure(panic_message(&*payload))));
        // Give connections a moment to flush their final Goodbye.
        self.runtime.shutdown_timeout(Duration::from_secs(1));
        outcome
    }

    /// Stops the server and waits for it.
    pub fn shutdown(self) -> Result<(), ServerFailure> {
        let _ = self.to_sim.send(net::Inbound::Shutdown);
        self.wait()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flatbuffers::FlatBufferBuilder;
    use pax_protocol::{Direction, FrameDecoder, read_server_message, wire};
    use std::io::{Read, Write};

    fn hello_frame() -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();
        let h = wire::Hello::create(
            &mut b,
            &wire::HelloArgs { protocol_major: pax_protocol::PROTOCOL_MAJOR, ..Default::default() },
        );
        let m = wire::ClientMessage::create(
            &mut b,
            &wire::ClientMessageArgs { payload_type: wire::ClientPayload::Hello, payload: Some(h.as_union_value()) },
        );
        wire::finish_size_prefixed_client_message_buffer(&mut b, m);
        b.finished_data().to_vec()
    }

    /// `Server::start` checks the configuration however it was built, not just the
    /// CLI: a fairness pause no shorter than the drop would drop clients instead.
    #[test]
    fn a_server_refuses_a_configuration_that_breaks_the_rules() {
        let scenario = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let mut config = Config::local(&scenario);
        config.max_players = 2;
        config.pause_after = Some(Duration::from_secs(40));
        config.idle_timeout = Duration::from_secs(30);
        assert!(matches!(Server::start(config.clone()), Err(StartError::Config(ConfigError::PauseNotBeforeDrop))));
        config.pause_after = Some(Duration::from_secs(5));
        config.bind = SocketAddr::from(([0, 0, 0, 0], 0));
        assert!(matches!(
            Server::start(config.clone()),
            Err(StartError::Config(ConfigError::MultiplayerNeedsTls { .. }))
        ));
        // One player, but a password: it would cross the network in clear.
        config.max_players = 1;
        config.pause_after = None;
        config.password = Some(Secret::new("pw"));
        assert!(matches!(config.validate(), Err(ConfigError::PasswordNeedsTls { .. })));
        config.password = None;
        config.admin = Some(Admin { name: "ada".into(), password: Secret::new("pw") });
        assert!(matches!(config.validate(), Err(ConfigError::PasswordNeedsTls { .. })));
        config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
        assert_eq!(config.validate(), Ok(()), "on loopback, a password is fine");
    }

    /// A panic on the sim thread reaches the client (Goodbye) and the caller (Err),
    /// instead of looking like a clean exit.
    #[test]
    fn a_simulation_panic_reaches_clients_and_the_caller() {
        let scenario = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let server = Server::start(Config::local(scenario)).unwrap();
        let mut stream = std::net::TcpStream::connect(server.local_addr()).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.write_all(&hello_frame()).unwrap();

        let mut frames = FrameDecoder::new(Direction::ServerToClient);
        let mut next = || -> Option<Vec<u8>> {
            let mut buf = [0u8; 65_536];
            loop {
                if let Some(frame) = frames.next_frame().unwrap() {
                    return Some(frame);
                }
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => return None,
                    Ok(n) => frames.push(&buf[..n]),
                }
            }
        };
        assert!(read_server_message(&next().unwrap()).unwrap().payload_as_welcome().is_some());

        server.to_sim.send(net::Inbound::Crash).unwrap();
        let goodbye = next().expect("a Goodbye before the connection closes");
        let reason = read_server_message(&goodbye).unwrap().payload_as_goodbye().unwrap().reason().unwrap().to_owned();
        assert!(reason.contains("the server failed: injected crash"), "{reason}");
        let failure = server.wait().expect_err("a panic is not a clean exit");
        assert!(failure.0.contains("injected crash"), "{failure}");
    }
}
