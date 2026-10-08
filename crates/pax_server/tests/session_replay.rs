//! M3-9: the session replay test, the server's determinism gate (D10, D11, D23).
//!
//! A scripted headless client plays a session over real TCP. It submits commands on
//! several days for both nations, two of them on the same day, changes speed while
//! running and while paused, changes its subscription, and saves. Then replaying the
//! save from the scenario (`pax_data::save::load_by_replay`, which `pax_cli replay`
//! runs) must reach the server's final `state_hash`, at any thread count. When the
//! `PAX_CLI` environment variable names a `pax_cli` binary (absolute, or relative to
//! the workspace root), the test also runs
//! `pax_cli replay` on the save and checks the hash it prints. CI sets it (and
//! `PAX_CLI_REQUIRED`, which makes a missing `PAX_CLI` a failure), and runs this test
//! on Linux, Windows and macOS.

mod common;

use common::*;
use pax_protocol::wire::{CommandError, MapMode, Speed};
use pax_server::Config;

/// Submits a tax change and checks the server accepted it for the next tick.
fn set_tax(c: &mut Client, client_seq: u32, nation: u32, rate_raw: i64, today: u64) {
    c.set_income_tax(client_seq, nation, rate_raw);
    loop {
        match c.next() {
            Got::CommandResult { client_seq: seq, error, applies_on_day } if seq == client_seq => {
                assert_eq!((error, applies_on_day), (CommandError::None, today), "command {client_seq}");
                return;
            }
            Got::DayUpdate { day, .. } => c.ack(day),
            Got::Closed => panic!("server closed the connection"),
            _ => {}
        }
    }
}

/// Resubscribes, which answers at once with an update for the current day, and
/// returns that update's `(day, state_hash)`.
fn current_state(c: &mut Client) -> (u64, u64) {
    c.subscribe(MapMode::Population, 0, Some(0), Some(0));
    loop {
        match c.next() {
            Got::DayUpdate { day, state_hash, market: Some(0), .. } => return (day, state_hash),
            Got::Closed => panic!("server closed the connection"),
            _ => {}
        }
    }
}

#[test]
fn a_saved_session_replays_to_the_servers_final_state() {
    let saves = std::env::temp_dir().join(format!("pax-session-replay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&saves);
    let mut config = Config::local(scenario("two_states"));
    config.saves_dir = saves.clone();
    let server = start(config);
    let mut c = Client::connect(server.local_addr());
    c.hello(None); // sandbox: may command both nations (D24)
    assert!(matches!(c.next(), Got::Welcome { day: 0, .. }));
    c.subscribe(MapMode::Nation, 0, None, None);
    assert!(matches!(c.next(), Got::DayUpdate { day: 0, .. }));

    // Day 0, paused: one command per nation.
    set_tax(&mut c, 1, 0, 150_000, 0);
    set_tax(&mut c, 2, 1, 120_000, 0);
    // A few days at a paced speed, then more commands while paused.
    c.set_speed(Speed::Fast);
    let mut paced = 0;
    while paced < 2 {
        match c.next() {
            Got::DayUpdate { day, .. } => {
                c.ack(day);
                paced += 1;
            }
            Got::Closed => panic!("server closed the connection"),
            _ => {}
        }
    }
    let day = play_until(&mut c, 3);
    // Two commands for one nation on the same day: the later stamp wins (D10).
    set_tax(&mut c, 3, 0, 90_000, day);
    set_tax(&mut c, 4, 0, 95_000, day);
    let day = play_until(&mut c, day + 25);
    set_tax(&mut c, 5, 1, 200_000, day);
    // Past a checkpoint (day 30), with a different subscription.
    c.subscribe(MapMode::Unemployment, 0, Some(1), None);
    let day = play_until(&mut c, day + 20);
    set_tax(&mut c, 6, 1, 110_000, day);
    let day = play_until(&mut c, day + 5);
    assert!(day > 30, "the session passes a checkpoint");

    let (final_day, final_hash) = current_state(&mut c);
    assert_eq!(final_day, day);
    c.save_game("session");
    assert_eq!(c.next(), Got::SaveResult { name: "session".into(), error: String::new() });
    server.shutdown().expect("clean shutdown");

    let path = saves.join("session.toml");
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        let replayed = pool.install(|| pax_data::save::load_by_replay(&path)).expect("the save replays");
        let world = &replayed.scenario.world;
        assert_eq!(
            (world.day, world.state_hash()),
            (final_day, final_hash),
            "replaying at {threads} threads reaches the server's final state"
        );
        let players: Vec<_> = replayed.save.commands.iter().filter(|c| c.player.is_some()).collect();
        assert_eq!(players.len(), 6, "every accepted command is in the save's log");
    }
    let cli = std::env::var_os("PAX_CLI");
    if cli.is_none() {
        // CI sets PAX_CLI_REQUIRED, so a renamed variable or a rewritten step can't
        // quietly drop the binary's check.
        assert!(std::env::var_os("PAX_CLI_REQUIRED").is_none(), "PAX_CLI_REQUIRED is set but PAX_CLI is not");
        eprintln!("note: PAX_CLI is not set, so the pax_cli replay check is skipped");
    }
    if let Some(cli) = cli {
        let cli = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(cli);
        let out = std::process::Command::new(cli).arg("replay").arg(&path).output().expect("pax_cli runs");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "pax_cli replay failed: {}", String::from_utf8_lossy(&out.stderr));
        let expected = format!("to day {final_day}: state_hash {final_hash:#018x}");
        assert!(stdout.contains(&expected), "pax_cli replay printed {stdout:?}, expected {expected:?}");
    }
    // The snapshot is the same state, without replaying.
    let loaded = pax_data::save::load(&path).expect("the snapshot loads");
    assert_eq!(loaded.scenario.world.state_hash(), final_hash);
    std::fs::remove_dir_all(&saves).unwrap();
}
