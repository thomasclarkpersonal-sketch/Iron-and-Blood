//! The bridge against a real `pax_server` (critic #36: no hand-made demo frames): the
//! same `Connection` and `LocalServer` GDScript drives, here driven by a test.
//!
//! The server runs as a child process, as it does for the client, so `pax_godot`
//! never links the engine, even in tests (D12). Its binary is `PAX_SERVER` (absolute,
//! or relative to the workspace root) or else the one built next to this test
//! (`cargo test --all` builds it; otherwise run `cargo build -p pax_server` first).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pax_godot::connection::{Connection, JoinTarget, LocalServer};
use pax_godot::decode::{SaveRequest, ServerEvent};
use pax_godot::encode::Policy;
use pax_protocol::wire::{CommandError, MapMode, Speed};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("pax-godot-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A connection whose events are never lost between waits: what one poll delivers
/// beyond the awaited event stays queued for the next wait.
struct Tester {
    c: Connection,
    backlog: VecDeque<ServerEvent>,
}

impl Tester {
    fn new(c: Connection) -> Tester {
        Tester { c, backlog: VecDeque::new() }
    }

    /// The first event, in arrival order, that `want` matches (earlier ones are
    /// skipped); polls as needed, failing after 10 s or on close.
    fn wait_for(&mut self, mut want: impl FnMut(&ServerEvent) -> bool) -> ServerEvent {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Some(event) = self.backlog.pop_front() {
                if want(&event) {
                    return event;
                }
            }
            let polled = self.c.poll();
            self.backlog.extend(polled.events);
            assert_eq!(polled.closed, None, "the connection closed");
            assert!(Instant::now() < deadline, "timed out");
            if self.backlog.is_empty() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

impl std::ops::Deref for Tester {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.c
    }
}

impl std::ops::DerefMut for Tester {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.c
    }
}

/// A whole session: Welcome with the tables, every view, a command, speed changes,
/// acks (updates keep coming past the flow-control window), a save and a load.
fn play(c: &mut Tester) {
    c.hello(None);
    let ServerEvent::Welcome(w) = c.wait_for(|e| matches!(e, ServerEvent::Welcome(_))) else { unreachable!() };
    assert_eq!((w.scenario.as_str(), w.nations.len(), w.day), ("Two States", 2, 0));
    assert!(w.map_hash.is_some(), "two_states has a map");
    assert_eq!(w.map_dir.as_deref(), Some("map"), "the server says where the map is");

    c.subscribe(MapMode::Price, 0, Some(0), Some(0));
    let ServerEvent::DayUpdate(u) = c.wait_for(|e| matches!(e, ServerEvent::DayUpdate(_))) else { unreachable!() };
    assert_eq!(u.map.as_ref().map(|m| m.values.len()), Some(w.provinces.len()));
    assert_eq!(u.market.as_ref().map(|m| m.price.len()), Some(w.goods.len()));
    assert!(u.province.as_ref().is_some_and(|p| !p.pop_people.is_empty()));
    assert!(u.world.population > 0);

    let seq = c.submit(Policy::IncomeTax, 1, 175_000);
    let result = c.wait_for(|e| matches!(e, ServerEvent::CommandResult { .. }));
    assert_eq!(result, ServerEvent::CommandResult { client_seq: seq, error: CommandError::None, applies_on_day: 0 });

    // Past the 3-update window: only the automatic acks keep updates coming.
    c.set_speed(Speed::Fastest);
    let ServerEvent::DayUpdate(u) = c.wait_for(|e| matches!(e, ServerEvent::DayUpdate(u) if u.day >= 12)) else {
        unreachable!()
    };
    assert_eq!(u.nations.income_tax_rate_raw[1], 175_000, "the command applied");
    c.set_speed(Speed::Paused);
    c.wait_for(|e| matches!(e, ServerEvent::ServerState { speed: Speed::Paused, .. }));

    c.save_game("bridge");
    let saved = c.wait_for(|e| matches!(e, ServerEvent::SaveResult { .. }));
    assert_eq!(
        saved,
        ServerEvent::SaveResult { request: SaveRequest::Save, name: "bridge".into(), error: String::new() }
    );
    c.list_saves();
    let listed = c.wait_for(|e| matches!(e, ServerEvent::SaveList { .. }));
    assert_eq!(listed, ServerEvent::SaveList { names: vec!["bridge".into()] });
    c.load_game("bridge");
    c.wait_for(|e| matches!(e, ServerEvent::Welcome(_)));
    // Save then load at once, before either answer: each answer pairs with its request.
    c.save_game("quick");
    c.load_game("quick");
    let saved = c.wait_for(|e| matches!(e, ServerEvent::SaveResult { .. }));
    assert_eq!(
        saved,
        ServerEvent::SaveResult { request: SaveRequest::Save, name: "quick".into(), error: String::new() }
    );
    c.wait_for(|e| matches!(e, ServerEvent::Welcome(_)));
    // A failed load answers SaveResult, and the session goes on.
    c.load_game("missing");
    let failed = c.wait_for(|e| matches!(e, ServerEvent::SaveResult { .. }));
    assert!(
        matches!(failed, ServerEvent::SaveResult { request: SaveRequest::Load, error, .. } if error.contains("no save"))
    );
    c.subscribe(MapMode::Population, 0, None, None);
    c.wait_for(|e| matches!(e, ServerEvent::DayUpdate(_)));
}

/// The `pax_server` binary: `PAX_SERVER`, or the one in this test's target directory.
fn server_binary() -> PathBuf {
    if let Some(binary) = std::env::var_os("PAX_SERVER") {
        return repo().join(binary);
    }
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().and_then(Path::parent).expect("target/<profile>/deps/<test>");
    let binary = dir.join(format!("pax_server{}", std::env::consts::EXE_SUFFIX));
    assert!(
        binary.exists(),
        "no pax_server at {}: run `cargo build -p pax_server` or set PAX_SERVER",
        binary.display()
    );
    binary
}

fn launch(saves: &TempDir) -> LocalServer {
    LocalServer::launch(&server_binary(), &repo().join("scenarios/two_states"), &saves.0).expect("the server starts")
}

#[test]
fn a_whole_session_against_a_real_server() {
    let saves = TempDir::new("session");
    let server = launch(&saves);
    let mut c = Tester::new(Connection::connect(server.addr, Duration::from_secs(5)).unwrap());
    play(&mut c);

    // Paused and silent for longer than the keep-alive: the session survives.
    let quiet = Instant::now() + pax_godot::connection::KEEP_ALIVE * 2;
    while Instant::now() < quiet {
        assert_eq!(c.poll().closed, None);
        std::thread::sleep(Duration::from_millis(50));
    }
    c.disconnect();
    // --exit-when-idle: dropping the handle finds the server already gone or ends it.
    drop(server);
}

#[test]
fn a_closed_server_is_reported_once() {
    let saves = TempDir::new("closed");
    let mut server = launch(&saves);
    let mut c = Tester::new(Connection::connect(server.addr, Duration::from_secs(5)).unwrap());
    c.hello(None);
    c.wait_for(|e| matches!(e, ServerEvent::Welcome(_)));
    server.kill();
    let deadline = Instant::now() + Duration::from_secs(10);
    let reason = loop {
        if let Some(reason) = c.poll().closed {
            break reason;
        }
        assert!(Instant::now() < deadline, "the close was never reported");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(!reason.is_empty());
    assert!(!c.is_open());
    assert_eq!(c.poll().closed, None, "reported once");
}

/// A multiplayer server with TLS (M4-6) as a child process: the port and the
/// certificate's fingerprint, read from the files it writes once it listens.
struct TlsServer {
    child: std::process::Child,
    port: u16,
    fingerprint: String,
}

impl Drop for TlsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn tls_server(dir: &TempDir) -> TlsServer {
    std::fs::create_dir_all(&dir.0).unwrap();
    let (port_file, fingerprint_file) = (dir.0.join("port"), dir.0.join("fingerprint"));
    let child = std::process::Command::new(server_binary())
        .arg("--scenario")
        .arg(repo().join("scenarios/two_states"))
        .args(["--bind", "127.0.0.1:0", "--players", "2", "--tls-self-signed"])
        .arg("--port-file")
        .arg(&port_file)
        .arg("--fingerprint-file")
        .arg(&fingerprint_file)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("pax_server starts");
    let deadline = Instant::now() + Duration::from_secs(10);
    // The fingerprint is written before the port, so the port's arrival means both.
    let port = loop {
        if let Some(port) = std::fs::read_to_string(&port_file).ok().and_then(|p| p.trim().parse().ok()) {
            break port;
        }
        assert!(Instant::now() < deadline, "the server wrote no port file");
        std::thread::sleep(Duration::from_millis(20));
    };
    let fingerprint = std::fs::read_to_string(&fingerprint_file).unwrap().trim().to_owned();
    TlsServer { child, port, fingerprint }
}

/// M4-6: the bridge plays over TLS, pinned to the server's certificate, and refuses
/// a server whose certificate isn't the pinned one.
#[test]
fn a_session_over_tls_pinned_to_the_servers_certificate() {
    let dir = TempDir::new("tls");
    let server = tls_server(&dir);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], server.port));
    assert_eq!(server.fingerprint.len(), 64, "SHA-256 in hex: {}", server.fingerprint);

    let mut c = Tester::new(Connection::connect_tls(addr, Duration::from_secs(5), &server.fingerprint).unwrap());
    c.hello(Some(0));
    let ServerEvent::Welcome(w) = c.wait_for(|e| matches!(e, ServerEvent::Welcome(_))) else { unreachable!() };
    assert_eq!((w.player, w.nation), (0, Some(0)));
    c.wait_for(|e| matches!(e, ServerEvent::LobbyState { .. }));

    // Another certificate's fingerprint: the handshake fails, and the reason says why.
    let wrong = "00".repeat(32);
    let mut bad = Connection::connect_tls(addr, Duration::from_secs(5), &wrong).unwrap();
    bad.hello(Some(1));
    let deadline = Instant::now() + Duration::from_secs(10);
    let reason = loop {
        if let Some(reason) = bad.poll().closed {
            break reason;
        }
        assert!(Instant::now() < deadline, "a wrong certificate must end the connection");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(reason.contains("not the one its fingerprint names"), "{reason}");
    assert!(Connection::connect_tls(addr, Duration::from_secs(5), "not hex").is_err());
}

/// M4-9: a dropped player rejoins the way it first joined (`JoinTarget`, which
/// `PaxClient.rejoin` replays): the same pinned TLS and name, and the resume token
/// from its `Welcome`, reclaim its seat and nation in the started game (D24).
#[test]
fn a_dropped_player_rejoins_with_its_join_target() {
    let dir = TempDir::new("rejoin");
    let server = tls_server(&dir);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], server.port));
    let target = |name: &str| JoinTarget {
        addr,
        fingerprint: server.fingerprint.clone(),
        password: None,
        name: Some(name.to_owned()),
    };
    let welcome = |c: &mut Tester| {
        let ServerEvent::Welcome(w) = c.wait_for(|e| matches!(e, ServerEvent::Welcome(_))) else { unreachable!() };
        w
    };
    // The host first (the first player is host, D24), then the guest.
    let mut host = Tester::new(target("ada").connect(Duration::from_secs(5)).unwrap());
    host.hello(Some(0));
    welcome(&mut host);
    let guest_target = target("bo");
    let mut guest = Tester::new(guest_target.connect(Duration::from_secs(5)).unwrap());
    guest.hello(Some(1));
    let w = welcome(&mut guest);
    let (player, token) = (w.player, w.resume_token);
    assert_ne!(token, 0, "a multiplayer Welcome carries a resume token");
    host.set_ready(true);
    guest.set_ready(true);
    host.wait_for(|e| {
        matches!(e, ServerEvent::LobbyState { players, .. }
            if players.len() == 2 && players.iter().all(|p| p.ready)
                && players.iter().any(|p| p.name == "bo"))
    });
    host.start_game();
    guest.wait_for(|e| matches!(e, ServerEvent::LobbyState { started: true, .. }));

    // The guest drops; its seat waits for the token.
    drop(guest);
    host.wait_for(|e| {
        matches!(e, ServerEvent::LobbyState { players, .. } if players.iter().any(|p| p.player == player && p.away))
    });
    let mut back = Tester::new(guest_target.connect(Duration::from_secs(5)).unwrap());
    back.resume(token);
    let w = welcome(&mut back);
    assert_eq!((w.player, w.nation), (player, Some(1)), "the same seat and nation");
    host.wait_for(|e| {
        matches!(e, ServerEvent::LobbyState { players, .. }
            if players.iter().any(|p| p.player == player && !p.away && p.name == "bo"))
    });
}
