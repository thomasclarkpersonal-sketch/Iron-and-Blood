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
//!
//! State: handshake, single session, keep-alive, timeouts, protocol errors, and
//! subscriptions with views (M3-2, M3-3). Ticking, commands and saves arrive with
//! M3-4 to M3-6 (`docs/MILESTONE_3.md`).

mod clock;
mod commands;
mod encode;
mod game;
mod net;
mod queue;
mod request;
mod sim;
mod view;
mod window;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::Duration;

/// How to run a server.
#[derive(Clone, Debug)]
pub struct Config {
    /// Scenario directory (containing `scenario.toml`).
    pub scenario: PathBuf,
    /// Address to listen on. Single player binds `127.0.0.1:0`, a free local port
    /// (NETWORK_PROTOCOL §6).
    pub bind: SocketAddr,
    /// Close a session that sends nothing for this long (the client pings at least
    /// every 2 s, NETWORK_PROTOCOL §3).
    pub idle_timeout: Duration,
    /// Stop when the welcomed client leaves: the client launched this server.
    pub exit_when_idle: bool,
    /// Where `SaveGame` writes and `LoadGame` reads `<name>.toml` (D23).
    pub saves_dir: PathBuf,
}

impl Config {
    /// A local single-player server for `scenario`, with the protocol's 10 s timeout.
    pub fn local(scenario: impl Into<PathBuf>) -> Self {
        Config {
            scenario: scenario.into(),
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            idle_timeout: Duration::from_secs(10),
            exit_when_idle: false,
            saves_dir: PathBuf::from("saves"),
        }
    }
}

/// Why a server couldn't start.
#[derive(Debug)]
pub enum StartError {
    Scenario(pax_data::LoadError),
    Io(std::io::Error),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
        runtime.spawn(net::accept_loop(listener, to_sim.clone(), config.idle_timeout));
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
