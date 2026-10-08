//! The client's TCP connection to `pax_server` (D22, NETWORK_PROTOCOL §2, §5), and
//! the local server it launches in single player (§6). Plain Rust, tested against a
//! real server. GDScript polls it once per frame and never blocks.
//!
//! The connection also does what every client must do, so GDScript can't forget:
//! * **Ack:** every `DayUpdate` returned by [`Connection::poll`] is acknowledged at
//!   the start of the next poll, by when GDScript has processed it (§5's flow
//!   control).
//! * **Keep-alive:** a `Ping` goes out whenever nothing else was sent for
//!   [`KEEP_ALIVE`], so a paused game never trips the server's idle timeout.
//! * **Closing:** a protocol error, a `Goodbye` or a closed socket ends the
//!   connection for good, with a reason for the connection-lost screen.

use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use pax_protocol::wire;

use crate::decode::{SaveRequest, ServerEvent, ServerStream};
use crate::encode;
use crate::transport::{self, Transport};

/// Send a `Ping` after this long without sending anything: a fifth of the server's
/// idle timeout (`pax_protocol::IDLE_TIMEOUT`), so a paused game is never dropped.
pub const KEEP_ALIVE: Duration = Duration::from_millis(pax_protocol::IDLE_TIMEOUT.as_millis() as u64 / 5);

/// What one poll found.
#[derive(Debug, Default, PartialEq)]
pub struct Polled {
    pub events: Vec<ServerEvent>,
    /// Set once, by the poll that found the connection ended: why it ended.
    pub closed: Option<String>,
}

/// A connection to a server: non-blocking, polled.
#[derive(Debug)]
pub struct Connection {
    /// Plain TCP, or TLS pinned to the server's certificate (M4-6).
    transport: Transport,
    reader: ServerStream,
    /// Bytes not yet accepted by the socket, in order.
    outbox: Vec<u8>,
    last_sent: Instant,
    /// The latest `DayUpdate` day returned by the last poll, acknowledged by the next.
    unacked: Option<u64>,
    next_client_seq: u32,
    next_nonce: u64,
    /// Why the connection ended. Once set, the connection does nothing more.
    closed: Option<String>,
    /// Whether `closed` has been reported by a poll.
    reported: bool,
    /// Sent with `Hello` (D24, protocol 1.6); `None` for a server without one.
    password: Option<String>,
    /// The player's name in `Hello`, which the lobby shows.
    name: String,
}

impl Connection {
    /// Connects to `addr` over plain TCP, waiting at most `timeout`: a local server
    /// (single player, or a multiplayer server on this machine).
    pub fn connect(addr: SocketAddr, timeout: Duration) -> std::io::Result<Connection> {
        Ok(Connection::over(Transport::Plain(Connection::socket(addr, timeout)?)))
    }

    /// Connects to `addr` over TLS, trusting only the certificate whose SHA-256 is
    /// `fingerprint` (D24, M4-6): a multiplayer server elsewhere. The handshake runs
    /// as the connection is polled; a wrong certificate ends it with a reason.
    pub fn connect_tls(addr: SocketAddr, timeout: Duration, fingerprint: &str) -> std::io::Result<Connection> {
        let pinned = transport::normalise(fingerprint).ok_or_else(|| {
            std::io::Error::new(ErrorKind::InvalidInput, "a fingerprint is 64 hex digits (the server prints it)")
        })?;
        Ok(Connection::over(Transport::tls(Connection::socket(addr, timeout)?, pinned)?))
    }

    fn socket(addr: SocketAddr, timeout: Duration) -> std::io::Result<TcpStream> {
        let socket = TcpStream::connect_timeout(&addr, timeout)?;
        socket.set_nodelay(true)?;
        socket.set_nonblocking(true)?;
        Ok(socket)
    }

    fn over(transport: Transport) -> Connection {
        Connection {
            transport,
            reader: ServerStream::default(),
            outbox: Vec::new(),
            last_sent: Instant::now(),
            unacked: None,
            next_client_seq: 1,
            next_nonce: 1,
            closed: None,
            password: None,
            name: "Iron and Blood (Godot)".to_owned(),
            reported: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.closed.is_none()
    }

    /// Queues `frame` and writes as much as the socket accepts now.
    fn send(&mut self, frame: Vec<u8>) {
        if self.closed.is_some() {
            return;
        }
        self.outbox.extend(frame);
        self.last_sent = Instant::now();
        self.flush();
    }

    fn flush(&mut self) {
        while !self.outbox.is_empty() {
            match self.transport.write(&self.outbox) {
                Ok(0) => return self.close("the server closed the connection".to_owned()),
                Ok(n) => {
                    self.outbox.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return self.close(format!("connection lost: {e}")),
            }
        }
        // TLS records rustls is still holding (a handshake's, or a full socket's).
        if let Err(e) = self.transport.flush() {
            self.close(format!("connection lost: {e}"));
        }
    }

    fn close(&mut self, reason: String) {
        if self.closed.is_none() {
            self.closed = Some(reason);
            let _ = self.transport.socket().shutdown(std::net::Shutdown::Both);
        }
    }

    /// Ends the connection from this side.
    pub fn disconnect(&mut self) {
        self.close("disconnected".to_owned());
        self.reported = true;
    }

    /// Acknowledges the previous poll's updates, keeps the session alive, sends what
    /// is queued, then decodes everything that arrived. Never blocks.
    pub fn poll(&mut self) -> Polled {
        let mut polled = Polled::default();
        if let Some(day) = self.unacked.take() {
            self.send(encode::ack(day));
        }
        if self.last_sent.elapsed() >= KEEP_ALIVE {
            let nonce = self.next_nonce;
            self.next_nonce += 1;
            self.send(encode::ping(nonce));
        }
        self.flush();
        let mut buf = [0u8; 64 * 1024];
        while self.closed.is_none() {
            match self.transport.read(&mut buf) {
                Ok(0) => self.close("the server closed the connection".to_owned()),
                Ok(n) => {
                    for event in self.reader.push(&buf[..n]) {
                        match &event {
                            ServerEvent::DayUpdate(u) => self.unacked = Some(u.day),
                            ServerEvent::Goodbye { reason } => {
                                self.close(format!("the server ended the session: {reason}"))
                            }
                            ServerEvent::Rejected { reason } => {
                                self.close(format!("the server refused this client: {reason}"))
                            }
                            _ => {}
                        }
                        polled.events.push(event);
                    }
                    if let Some(e) = self.reader.error() {
                        self.close(e.to_string());
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => self.close(format!("connection lost: {e}")),
            }
        }
        if self.closed.is_some() && !self.reported {
            self.reported = true;
            polled.closed = self.closed.clone();
        }
        polled
    }

    /// The password the next `Hello` carries (`None`: none).
    pub fn set_password(&mut self, password: Option<String>) {
        self.password = password;
    }

    /// The name the next `Hello` carries (the lobby shows it).
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn hello(&mut self, nation: Option<u32>) {
        let frame = encode::hello(&self.name, nation, 0, self.password.as_deref());
        self.send(frame);
    }

    /// Reclaims the seat a dropped session kept (D24): `token` is its `Welcome`'s
    /// resume token.
    pub fn resume(&mut self, token: u64) {
        let frame = encode::hello(&self.name, None, token, self.password.as_deref());
        self.send(frame);
    }

    pub fn subscribe(&mut self, mode: wire::MapMode, good: u16, market: Option<u32>, province: Option<u32>) {
        self.send(encode::subscribe(mode, good, market, province));
    }

    pub fn set_speed(&mut self, speed: wire::Speed) {
        self.send(encode::set_speed(speed));
    }

    /// Sends a policy command and returns its `client_seq`, which its
    /// `CommandResult` will carry.
    pub fn submit(&mut self, policy: encode::Policy, nation: u32, rate_raw: i64) -> u32 {
        let seq = self.next_client_seq;
        self.next_client_seq += 1;
        self.send(encode::submit_command(seq, policy, nation, rate_raw));
        seq
    }

    pub fn save_game(&mut self, name: &str) {
        self.reader.expect(SaveRequest::Save);
        self.send(encode::save_game(name));
    }

    /// Asks to load a save. The answer is a new `Welcome` (the decoder accepts it
    /// only now) or a `SaveResult` with the error.
    pub fn load_game(&mut self, name: &str) {
        self.reader.expect(SaveRequest::Load);
        self.send(encode::load_game(name));
    }

    pub fn list_saves(&mut self) {
        self.send(encode::list_saves());
    }

    /// The lobby (M4-2); each is answered with a `LobbyState`.
    pub fn claim_nation(&mut self, nation: Option<u32>) {
        self.send(encode::claim_nation(nation));
    }

    pub fn set_ready(&mut self, ready: bool) {
        self.send(encode::set_ready(ready));
    }

    pub fn start_game(&mut self) {
        self.send(encode::start_game());
    }

    pub fn kick(&mut self, player: u16) {
        self.send(encode::kick(player));
    }
}

/// A `pax_server` the client launched (NETWORK_PROTOCOL §6). It runs with
/// `--exit-when-stdin-closes`, and this holds its stdin: [`LocalServer::stop`] (or
/// dropping this, or the client's process ending) closes it, and the server tells its
/// players and exits. A single-player server also stops by itself when its player
/// leaves (`--exit-when-idle`).
#[derive(Debug)]
pub struct LocalServer {
    /// `None` once stopped or killed.
    child: Option<Child>,
    /// Where this machine reaches it (loopback).
    pub addr: SocketAddr,
}

/// A game this client hosts (M4-9): the server, and its TLS certificate's SHA-256
/// (normalised), which the host shares with the players it invites (M4-6).
#[derive(Debug)]
pub struct HostedServer {
    pub server: LocalServer,
    pub fingerprint: String,
}

/// How to reach a multiplayer game over TLS (D24, M4-6, M4-9): its address, the
/// pinned fingerprint, and what the player gave. The bridge keeps the last one, so
/// a rejoin replays the whole handshake in Rust, the same way as the first time.
#[derive(Clone)]
pub struct JoinTarget {
    pub addr: SocketAddr,
    pub fingerprint: String,
    pub password: Option<String>,
    pub name: Option<String>,
}

impl std::fmt::Debug for JoinTarget {
    /// Never the password: the bridge keeps this for the client's lifetime, and a
    /// log of it must not leak one (as the server's `Secret`, M4-6).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinTarget")
            .field("addr", &self.addr)
            .field("fingerprint", &self.fingerprint)
            .field("password", &self.password.as_ref().map(|_| "…"))
            .field("name", &self.name)
            .finish()
    }
}

impl JoinTarget {
    /// Connects over TLS pinned to the fingerprint, with the password and the name
    /// set for the next `hello` or `resume`.
    pub fn connect(&self, timeout: Duration) -> std::io::Result<Connection> {
        let mut c = Connection::connect_tls(self.addr, timeout, &self.fingerprint)?;
        c.set_password(self.password.clone());
        if let Some(name) = &self.name {
            c.set_name(name.clone());
        }
        Ok(c)
    }
}

impl LocalServer {
    /// Starts `server` on a free local port for `scenario`, saving into `saves`, and
    /// waits (at most 20 s) for it to write its port: single player.
    pub fn launch(server: &Path, scenario: &Path, saves: &Path) -> Result<LocalServer, String> {
        // Single player plays sandbox (any nation), which a server allows only with
        // --sandbox (D24).
        LocalServer::start(server, scenario, saves, &["--bind", "127.0.0.1:0", "--sandbox", "--exit-when-idle"])
    }

    /// Starts `server` as a player-hosted multiplayer game for `players` (M4-9):
    /// reachable from other machines on a free port, over TLS with a fresh
    /// certificate (D24, M4-6). Its fingerprint comes back for the host to share.
    pub fn host(server: &Path, scenario: &Path, saves: &Path, players: u16) -> Result<HostedServer, String> {
        let players = players.to_string();
        let fingerprint_file = port_file_path().with_extension("fingerprint");
        let _ = std::fs::remove_file(&fingerprint_file);
        let file = fingerprint_file.to_string_lossy();
        // No --exit-when-idle: the game goes on while its host's client runs, even if
        // nobody is connected for a moment (the host can rejoin); the client stops it.
        let args = ["--bind", "0.0.0.0:0", "--players", &players, "--tls-self-signed", "--fingerprint-file", &file];
        let mut server = LocalServer::start(server, scenario, saves, &args)?;
        // The server writes the fingerprint before the port (NETWORK_PROTOCOL §6), so
        // by now it must be there: anything else is the launch's failure, said as
        // such, not a fingerprint the player got wrong.
        let read = std::fs::read_to_string(&fingerprint_file);
        let _ = std::fs::remove_file(&fingerprint_file);
        let fingerprint = match read {
            Ok(text) => transport::normalise(&text).ok_or_else(|| format!("{:?} is not a fingerprint", text.trim())),
            Err(e) => Err(e.to_string()),
        };
        match fingerprint {
            Ok(fingerprint) => Ok(HostedServer { server, fingerprint }),
            Err(e) => {
                server.kill();
                Err(format!(
                    "the server gave its port, but not its certificate fingerprint ({}): {e}",
                    fingerprint_file.display()
                ))
            }
        }
    }

    /// Starts `server` with `args` and waits (at most 20 s) for its port.
    fn start(server: &Path, scenario: &Path, saves: &Path, args: &[&str]) -> Result<LocalServer, String> {
        let port_file = port_file_path();
        let _ = std::fs::remove_file(&port_file);
        let mut command = Command::new(server);
        command.arg("--scenario").arg(scenario).args(args).arg("--exit-when-stdin-closes");
        command.arg("--port-file").arg(&port_file).arg("--saves").arg(saves);
        // Its stdin stays open while this client holds it: closing it (`stop`, or
        // this process ending in any way) stops the server cleanly.
        let mut child =
            command.stdin(Stdio::piped()).spawn().map_err(|e| format!("cannot start {}: {e}", server.display()))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(text) = std::fs::read_to_string(&port_file)
                && let Ok(port) = text.trim().parse::<u16>()
            {
                let _ = std::fs::remove_file(&port_file);
                return Ok(LocalServer { child: Some(child), addr: SocketAddr::from(([127, 0, 0, 1], port)) });
            }
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!("the server exited before it was ready ({status})"));
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err("the server did not start within 20 s".to_owned());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl LocalServer {
    /// Kills the server at once, as a crash would.
    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Stops the server cleanly without waiting for it: closing its stdin makes it
    /// tell every player "the server is shutting down" (`Goodbye`) and exit
    /// (`--exit-when-stdin-closes`). A background thread reaps it, and kills it if it
    /// hasn't exited after `STOP_GRACE`, so the caller (Godot's main thread) never
    /// blocks.
    pub fn stop(&mut self) {
        let Some(mut child) = self.child.take() else { return };
        drop(child.stdin.take());
        std::thread::spawn(move || {
            let deadline = Instant::now() + STOP_GRACE;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = child.kill();
            let _ = child.wait();
        });
    }
}

/// How long a stopped server has to say goodbye and exit before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);

impl Drop for LocalServer {
    /// Ending the server is always [`LocalServer::stop`]: single player or a hosted
    /// game alike, its players are told, and nothing blocks.
    fn drop(&mut self) {
        self.stop();
    }
}

/// A port file no other launch on this machine uses: the process id, a counter for
/// launches within this process (two at the same instant must not share a file, or
/// both clients read one server's port), and the time, for a reused process id.
fn port_file_path() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAUNCHES: AtomicU64 = AtomicU64::new(0);
    let launch = LAUNCHES.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("pax-port-{}-{launch}-{nanos}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two launches at the same instant in one process get different port files
    /// (CI once connected two tests' clients to one server).
    #[test]
    fn port_files_are_unique_within_a_process() {
        let a = port_file_path();
        let b = port_file_path();
        assert_ne!(a, b);
    }

    #[test]
    fn the_keep_alive_is_well_inside_the_idle_timeout() {
        assert!(KEEP_ALIVE > Duration::ZERO);
        assert!(KEEP_ALIVE * 4 < pax_protocol::IDLE_TIMEOUT);
    }
}
