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

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use pax_protocol::wire;

use crate::decode::{SaveRequest, ServerEvent, ServerStream};
use crate::encode;

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
    stream: TcpStream,
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
}

impl Connection {
    /// Connects to `addr`, waiting at most `timeout`.
    pub fn connect(addr: SocketAddr, timeout: Duration) -> std::io::Result<Connection> {
        let stream = TcpStream::connect_timeout(&addr, timeout)?;
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        Ok(Connection {
            stream,
            reader: ServerStream::default(),
            outbox: Vec::new(),
            last_sent: Instant::now(),
            unacked: None,
            next_client_seq: 1,
            next_nonce: 1,
            closed: None,
            reported: false,
        })
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
            match self.stream.write(&self.outbox) {
                Ok(0) => return self.close("the server closed the connection".to_owned()),
                Ok(n) => {
                    self.outbox.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return self.close(format!("connection lost: {e}")),
            }
        }
    }

    fn close(&mut self, reason: String) {
        if self.closed.is_none() {
            self.closed = Some(reason);
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
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
            match self.stream.read(&mut buf) {
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

    pub fn hello(&mut self, nation: Option<u32>) {
        self.send(encode::hello("Iron and Blood (Godot)", nation));
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

/// A `pax_server` the client launched (NETWORK_PROTOCOL §6). With `--exit-when-idle`
/// it stops by itself when the client disconnects; dropping this waits briefly for
/// that, then kills it.
#[derive(Debug)]
pub struct LocalServer {
    child: Child,
    pub addr: SocketAddr,
}

impl LocalServer {
    /// Starts `server` on a free local port for `scenario`, saving into `saves`, and
    /// waits (at most 20 s) for it to write its port.
    pub fn launch(server: &Path, scenario: &Path, saves: &Path) -> Result<LocalServer, String> {
        let port_file = port_file_path();
        let _ = std::fs::remove_file(&port_file);
        let mut child = Command::new(server)
            .arg("--scenario")
            .arg(scenario)
            // Single player plays sandbox (any nation), which a server allows only with
            // --sandbox (D24).
            .args(["--bind", "127.0.0.1:0", "--sandbox", "--exit-when-idle"])
            .arg("--port-file")
            .arg(&port_file)
            .arg("--saves")
            .arg(saves)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", server.display()))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(text) = std::fs::read_to_string(&port_file)
                && let Ok(port) = text.trim().parse::<u16>()
            {
                let _ = std::fs::remove_file(&port_file);
                return Ok(LocalServer { child, addr: SocketAddr::from(([127, 0, 0, 1], port)) });
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
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
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
