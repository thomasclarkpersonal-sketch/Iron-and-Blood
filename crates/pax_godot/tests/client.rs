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

use pax_godot::connection::{Connection, LocalServer};
use pax_godot::decode::ServerEvent;
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
    assert_eq!(saved, ServerEvent::SaveResult { name: "bridge".into(), error: String::new() });
    c.list_saves();
    let listed = c.wait_for(|e| matches!(e, ServerEvent::SaveList { .. }));
    assert_eq!(listed, ServerEvent::SaveList { names: vec!["bridge".into()] });
    c.load_game("bridge");
    c.wait_for(|e| matches!(e, ServerEvent::Welcome(_)));
    // Save then load at once, before either answer: each answer pairs with its request.
    c.save_game("quick");
    c.load_game("quick");
    let saved = c.wait_for(|e| matches!(e, ServerEvent::SaveResult { .. }));
    assert_eq!(saved, ServerEvent::SaveResult { name: "quick".into(), error: String::new() });
    c.wait_for(|e| matches!(e, ServerEvent::Welcome(_)));
    // A failed load answers SaveResult, and the session goes on.
    c.load_game("missing");
    let failed = c.wait_for(|e| matches!(e, ServerEvent::SaveResult { .. }));
    assert!(matches!(failed, ServerEvent::SaveResult { error, .. } if error.contains("no save")));
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
