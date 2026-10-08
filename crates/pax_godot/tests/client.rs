//! The bridge against a real `pax_server` (critic #36: no hand-made demo frames). The
//! same `Connection` GDScript drives, here driven by a test. If `PAX_SERVER` names a
//! `pax_server` binary (absolute, or relative to the workspace root), the launcher
//! is tested too; CI sets it.

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

/// Polls until `want` matches an event (returned), failing after 10 s or on close.
fn wait_for(c: &mut Connection, mut want: impl FnMut(&ServerEvent) -> bool) -> ServerEvent {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let polled = c.poll();
        if let Some(event) = polled.events.into_iter().find(|e| want(e)) {
            return event;
        }
        assert_eq!(polled.closed, None, "the connection closed");
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A whole session: Welcome with the tables, every view, a command, speed changes,
/// acks (updates keep coming past the flow-control window), a save and a load.
fn play(c: &mut Connection) {
    c.hello(None);
    let ServerEvent::Welcome(w) = wait_for(c, |e| matches!(e, ServerEvent::Welcome(_))) else { unreachable!() };
    assert_eq!((w.scenario.as_str(), w.nations.len(), w.day), ("Two States", 2, 0));
    assert!(w.map_hash.is_some(), "two_states has a map");

    c.subscribe(MapMode::Price, 0, Some(0), Some(0));
    let ServerEvent::DayUpdate(u) = wait_for(c, |e| matches!(e, ServerEvent::DayUpdate(_))) else { unreachable!() };
    assert_eq!(u.map.as_ref().map(|m| m.values.len()), Some(w.provinces.len()));
    assert_eq!(u.market.as_ref().map(|m| m.price.len()), Some(w.goods.len()));
    assert!(u.province.as_ref().is_some_and(|p| !p.pop_people.is_empty()));
    assert!(u.world.population > 0);

    let seq = c.submit(Policy::IncomeTax, 1, 175_000);
    let result = wait_for(c, |e| matches!(e, ServerEvent::CommandResult { .. }));
    assert_eq!(result, ServerEvent::CommandResult { client_seq: seq, error: CommandError::None, applies_on_day: 0 });

    // Past the 3-update window: only the automatic acks keep updates coming.
    c.set_speed(Speed::Fastest);
    let ServerEvent::DayUpdate(u) = wait_for(c, |e| matches!(e, ServerEvent::DayUpdate(u) if u.day >= 12)) else {
        unreachable!()
    };
    assert_eq!(u.nations.income_tax_rate_raw[1], 175_000, "the command applied");
    c.set_speed(Speed::Paused);
    wait_for(c, |e| matches!(e, ServerEvent::ServerState { speed: Speed::Paused, .. }));

    c.save_game("bridge");
    let saved = wait_for(c, |e| matches!(e, ServerEvent::SaveResult { .. }));
    assert_eq!(saved, ServerEvent::SaveResult { name: "bridge".into(), error: String::new() });
    c.list_saves();
    let listed = wait_for(c, |e| matches!(e, ServerEvent::SaveList { .. }));
    assert_eq!(listed, ServerEvent::SaveList { names: vec!["bridge".into()] });
    c.load_game("bridge");
    wait_for(c, |e| matches!(e, ServerEvent::Welcome(_)));
    // A failed load answers SaveResult, and the session goes on.
    c.load_game("missing");
    let failed = wait_for(c, |e| matches!(e, ServerEvent::SaveResult { .. }));
    assert!(matches!(failed, ServerEvent::SaveResult { error, .. } if error.contains("no save")));
    c.subscribe(MapMode::Population, 0, None, None);
    wait_for(c, |e| matches!(e, ServerEvent::DayUpdate(_)));
}

#[test]
fn a_whole_session_against_a_real_server() {
    let saves = TempDir::new("session");
    let mut config = pax_server::Config::local(repo().join("scenarios/two_states"));
    config.saves_dir = saves.0.clone();
    let server = pax_server::Server::start(config).expect("server starts");
    let mut c = Connection::connect(server.local_addr(), Duration::from_secs(5)).unwrap();
    play(&mut c);

    // Paused and silent for longer than the keep-alive: the session survives.
    let quiet = Instant::now() + pax_godot::connection::KEEP_ALIVE * 2;
    while Instant::now() < quiet {
        assert_eq!(c.poll().closed, None);
        std::thread::sleep(Duration::from_millis(50));
    }
    c.disconnect();
    server.shutdown().expect("clean shutdown");
}

#[test]
fn a_closed_server_is_reported_once() {
    let server = pax_server::Server::start(pax_server::Config::local(repo().join("scenarios/two_states"))).unwrap();
    let mut c = Connection::connect(server.local_addr(), Duration::from_secs(5)).unwrap();
    c.hello(None);
    wait_for(&mut c, |e| matches!(e, ServerEvent::Welcome(_)));
    server.shutdown().expect("clean shutdown");
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

#[test]
fn the_launcher_starts_a_local_server() {
    let Some(binary) = std::env::var_os("PAX_SERVER") else {
        eprintln!("PAX_SERVER is not set: skipping the launcher test");
        return;
    };
    let saves = TempDir::new("launcher");
    let server = LocalServer::launch(&repo().join(binary), &repo().join("scenarios/two_states"), &saves.0).unwrap();
    let mut c = Connection::connect(server.addr, Duration::from_secs(5)).unwrap();
    play(&mut c);
    c.disconnect();
    // --exit-when-idle: dropping the handle finds the server already gone or ends it.
    drop(server);
}
