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
//! M3-2 state: handshake, single session, keep-alive, timeouts and protocol errors.
//! Ticking, views, commands and saves arrive with M3-3 to M3-6 (`docs/MILESTONE_3.md`).

mod encode;
mod net;
mod request;
mod sim;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc;
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
}

impl Config {
    /// A local single-player server for `scenario`, with the protocol's 10 s timeout.
    pub fn local(scenario: impl Into<PathBuf>) -> Self {
        Config {
            scenario: scenario.into(),
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            idle_timeout: Duration::from_secs(10),
            exit_when_idle: false,
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

impl From<std::io::Error> for StartError {
    fn from(e: std::io::Error) -> Self {
        StartError::Io(e)
    }
}

/// A running server.
pub struct Server {
    local_addr: SocketAddr,
    to_sim: mpsc::Sender<net::Inbound>,
    sim: JoinHandle<()>,
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
        let (to_sim, inbound) = mpsc::channel();
        runtime.spawn(net::accept_loop(listener, to_sim.clone(), config.idle_timeout));
        let sim = sim::Sim::new(scenario, config.exit_when_idle);
        let sim = std::thread::Builder::new().name("pax-sim".to_owned()).spawn(move || sim::run(sim, inbound))?;
        tracing::info!(%local_addr, "listening");
        Ok(Server { local_addr, to_sim, sim, runtime })
    }

    /// The address the server listens on (the actual port when bound to port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Blocks until the server stops by itself (`exit_when_idle`).
    pub fn wait(self) {
        let _ = self.sim.join();
        // Give connections a moment to flush their final Goodbye.
        self.runtime.shutdown_timeout(Duration::from_secs(1));
    }

    /// Stops the server and waits for it.
    pub fn shutdown(self) {
        let _ = self.to_sim.send(net::Inbound::Shutdown);
        self.wait();
    }
}
